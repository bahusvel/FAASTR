use super::SharedModule;
use alloc::arc::Arc;
use alloc::boxed::Box;
use arch::interrupt;
use context;
use context::{Context, SharedContext};
use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use memory::allocate_frames;
use paging::entry::EntryFlags;
use paging::temporary_page::TemporaryPage;
use paging::{ActivePageTable, InactivePageTable, Page, VirtualAddress};
use spin::RwLock;
use start::usermode;
use syscall::error::*;

pub extern "C" fn userspace_trampoline() {
    println!("Exited into trampoline");
    unsafe {
        let sp = ::USER_STACK_OFFSET + ::USER_STACK_SIZE - 256;
        // Go to usermode
        usermode(4162, sp, 0);
    }
}

pub fn spawn(module: SharedModule) -> Result<Context> {
    let mut stack = vec![0; 65_536].into_boxed_slice();
    let mut fx = unsafe {
        Box::from_raw(
            ::ALLOCATOR.alloc(Layout::from_size_align_unchecked(512, 16)) as *mut [u8; 512],
        )
    };
    let mut offset = 0;

    {
        //zero out FX storage
        for b in fx.iter_mut() {
            *b = 0;
        }
        offset = stack.len() - mem::size_of::<usize>();
        unsafe {
            let func_ptr = stack.as_mut_ptr().offset(offset as isize);
            *(func_ptr as *mut usize) = userspace_trampoline as usize;
        }
    }

    let mut context = Context::new(module.clone());

    {
        //Initializse some basics
        context.arch.set_fx(fx.as_ptr() as usize);
        context.arch.set_stack(stack.as_ptr() as usize + offset);
        context.kfx = Some(fx);
        context.kstack = Some(stack);

        // Create a new page table
        let mut active_table = unsafe { ActivePageTable::new() };

        let mut temporary_page = TemporaryPage::new(Page::containing_address(VirtualAddress::new(
            ::USER_TMP_MISC_OFFSET,
        )));

        let mut new_table = {
            let frame = allocate_frames(1).expect("no more frames in syscall::clone new_table");
            InactivePageTable::new(frame, &mut active_table, &mut temporary_page)
        };

        context.arch.set_page_table(unsafe { new_table.address() });

        // Copy kernel image mapping and heap mappings
        {
            let frame_kernel = active_table.p4()[::KERNEL_PML4]
                .pointed_frame()
                .expect("kernel image not mapped");
            let flags_kernel = active_table.p4()[::KERNEL_PML4].flags();
            let frame_heap = active_table.p4()[::KERNEL_HEAP_PML4]
                .pointed_frame()
                .expect("kernel heap not mapped");
            let flags_heap = active_table.p4()[::KERNEL_HEAP_PML4].flags();
            active_table.with(&mut new_table, &mut temporary_page, |mapper| {
                mapper.p4_mut()[::KERNEL_PML4].set(frame_kernel, flags_kernel);
                mapper.p4_mut()[::KERNEL_HEAP_PML4].set(frame_heap, flags_heap);
            });
        }

        // Also need to copy kernel TLS mappings, this really needs to be in its own PML4, so I can copy it above.
        for cpu_id in 0..::cpu_count() {
            extern "C" {
                // The starting byte of the thread data segment
                static mut __tdata_start: u8;
                // The ending byte of the thread BSS segment
                static mut __tbss_end: u8;
            }

            let size =
                unsafe { &__tbss_end as *const _ as usize - &__tdata_start as *const _ as usize };

            let start = ::KERNEL_PERCPU_OFFSET + ::KERNEL_PERCPU_SIZE * cpu_id;
            let end = start + size;

            let start_page = Page::containing_address(VirtualAddress::new(start));
            let end_page = Page::containing_address(VirtualAddress::new(end - 1));
            for page in Page::range_inclusive(start_page, end_page) {
                let frame = active_table
                    .translate_page(page)
                    .expect("kernel percpu not mapped");
                active_table.with(&mut new_table, &mut temporary_page, |mapper| {
                    let result = mapper.map_to(
                        page,
                        frame,
                        EntryFlags::PRESENT | EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                    );
                    // Ignore result due to operating on inactive table
                    unsafe {
                        result.ignore();
                    }
                });
            }
        }

        // TODO merge these into one closure.

        println!("Fine until here");

        // Setup user image
        active_table.with(&mut new_table, &mut temporary_page, |mapper| {
            // Copy writable image
            for memory in module.image.iter() {
                // Map non writable parts of image
                if memory.flags & EntryFlags::WRITABLE != EntryFlags::WRITABLE {
                    let mut page_ctr = 0;
                    for frame in memory.pages.frames() {
                        let page = Page::containing_address(VirtualAddress::new(
                            memory.start.get() + (page_ctr * 4096),
                        ));
                        // Ignoring result due to operating on inactive table.
                        unsafe {
                            mapper
                                .map_to(page, frame, memory.flags | EntryFlags::USER_ACCESSIBLE)
                                .ignore()
                        };
                        page_ctr += 1;
                    }
                    println!(
                        "Mapped {}-{}",
                        memory.start.get(),
                        memory.start.get() + memory.size()
                    );
                    continue;
                }
                /*
                // Copy writable parts of image
                let mut new_memory = context::memory::Memory::new(
                    VirtualAddress::new(memory.start.get()),
                    memory.size(),
                    memory.flags,
                    false,
                );

                unsafe {
                    intrinsics::copy(
                        memory.pages.as_ptr() as *const u8,
                        new_memory.start_address().get() as *mut u8,
                        memory.size(),
                    );
                }
                */
            }
        });

        // Map heap
        /*
        active_table.with(&mut new_table, &mut temporary_page, |mapper| {
            context.heap = Some(
                context::memory::Memory::new(
                    VirtualAddress::new(::USER_HEAP_OFFSET),
                    0,
                    EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
                    true,
                ).to_shared(),
            );
        });
        */
        // Setup user stack

        active_table.with(&mut new_table, &mut temporary_page, |mapper| {
            let stack_start = Page::containing_address(VirtualAddress::new(::USER_STACK_OFFSET));
            let stack_end = Page::containing_address(VirtualAddress::new(
                ::USER_STACK_OFFSET + ::USER_STACK_SIZE,
            ));
            for page in Page::range_inclusive(stack_start, stack_end) {
                unsafe {
                    mapper
                        .map(
                            page,
                            EntryFlags::NO_EXECUTE
                                | EntryFlags::WRITABLE
                                | EntryFlags::USER_ACCESSIBLE,
                        ).ignore() // ignore is ok, because not operating on current page table
                }
            }

            /*
            context.stack = Some(context::memory::Memory::new(
                VirtualAddress::new(::USER_STACK_OFFSET),
                ::USER_STACK_SIZE,
                EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
                true,
            ));
            */
        });
    }

    Ok(context)
}

pub fn fuse(module: SharedModule, func: usize) -> Result<()> {
    let inserted = {
        let mut context = spawn(module)?;
        let mut contexts_lock = context::contexts_mut();
        {
            let context_lock = contexts_lock.current().expect("No current context");
            context.ret_link = Some(context_lock.clone());
        }

        contexts_lock.insert(context)?.clone()
    };

    unsafe {
        interrupt::disable();
        context::fuse_switch(inserted.clone(), func)
    };

    // NOTE it may seem counter intuitive but fuse will return here!
    Ok(())
}

pub fn cast(module: SharedModule, func: usize) -> Result<SharedContext> {
    let mut context = spawn(module)?;
    context.status = context::Status::Runnable;

    Ok(context::contexts_mut().insert(context)?.clone())
}
