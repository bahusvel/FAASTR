use super::memory::ContextMemory;
use super::{FuncPtr, ModuleFuncPtr, SharedModule};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use arch::interrupt;
use context;
use context::{Context, SharedContext, Status};
use core::alloc::{GlobalAlloc, Layout};
use error::*;
use memory::{allocate_frames, EntryFlags, PAGE_SIZE};
use paging::temporary_page::TemporaryPage;
use paging::{ActivePageTable, InactivePageTable, Page, VirtualAddress};

pub fn spawn(module: SharedModule) -> Result<'static, Context> {
    let mut fx = unsafe {
        Box::from_raw(
            ::ALLOCATOR.alloc(Layout::from_size_align_unchecked(512, 16)) as *mut [u8; 512],
        )
    };

    //zero out FX storage
    for b in fx.iter_mut() {
        *b = 0;
    }

    let mut context = Context::new(module.clone());

    {
        //Initializse some basics
        context.arch.set_fx(fx.as_ptr() as usize);

        context.kfx = Some(fx);

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

        let kernel_areas = {
            let area = |pml4| {
                (
                    pml4,
                    active_table.p4()[pml4]
                        .pointed_frame()
                        .expect("kernel image not mapped"),
                    active_table.p4()[pml4].flags(),
                )
            };
            [
                area(::KERNEL_PML4),
                area(::KERNEL_HEAP_PML4),
                area(::KERNEL_VALLOC_PML4),
                area(::KERNEL_PERCPU_PML4),
            ]
        };

        active_table.with(&mut new_table, &mut temporary_page, |mapper| {
            for (pml4, frame, flags) in &kernel_areas {
                // Not operating on current page table so don't need to flush
                mapper.p4_mut()[*pml4].set(frame.clone(), *flags);
            }
        });

        println!("Fine until here");

        // Parts of the image that are readonly
        let readonly = module
            .image
            .iter()
            .filter(|m| !m.flags().contains(EntryFlags::WRITABLE))
            .map(|m| m.ref_clone(None));

        // FIXME check if any of these failed to allocate
        let writable = module
            .image
            .iter()
            .filter(|m| m.flags().contains(EntryFlags::WRITABLE))
            .map(|m| {
                let mut mc = m
                    .copy_clone(None)
                    .expect("Failed to allocate writable memory during spawn");
                mc.drop_kernel_mapping();
                mc
            });

        let mut image: Vec<ContextMemory> = readonly.chain(writable).collect();

        //println!("Image is alright");

        let mut stack = ContextMemory::new(
            ::USER_STACK_SIZE / PAGE_SIZE,
            VirtualAddress::new(::USER_STACK_OFFSET),
            EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
        ).expect("Failed to allocate stack");

        stack
            .map_to_kernel(EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE)
            .expect("Map failed");
        stack.zero();

        let mut heap = ContextMemory::new(
            1,
            VirtualAddress::new(::USER_HEAP_OFFSET),
            EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
        ).expect("Failed to allocate heap");

        heap.map_to_kernel(EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE)
            .expect("Map failed");
        heap.zero();

        //println!("Stack and heap also aight");

        // Setup user image
        active_table.with(&mut new_table, &mut temporary_page, |mapper| unsafe {
            for memory in image.iter_mut() {
                memory.map_context(mapper).ignore();
            }
            stack.map_context(mapper).ignore();
            heap.map_context(mapper).ignore();
        });

        //println!("Mapping went well too!");

        // TODO zero out stack and heap
        context.stack = Some(stack);
        context.heap = Some(heap);
        context.image = image;
    }

    Ok(context)
}

pub fn fuse_name(module: SharedModule, func: &str) -> Result<'static, ()> {
    let f = module.function(func).ok_or("Function not found")?;
    let mut context = spawn(module)?;
    context.name = Some(String::from(func));
    fuse_inner(context, f)
}

pub fn fuse_ptr(func: FuncPtr) -> Result<'static, ()> {
    let context = spawn(func.0)?;
    fuse_inner(context, func.1)
}

fn fuse_inner(mut context: Context, func: ModuleFuncPtr) -> Result<'static, ()> {
    context.function = func;
    let inserted = {
        let mut contexts_lock = context::contexts_mut();
        {
            let mut context_lock = contexts_lock.current().expect("No current context");
            context.ret_link = Some(context_lock.clone());
            context.cpu_id = context_lock.read().cpu_id;
            //context.kstack = Some(vec![0; 65_536].into_boxed_slice());
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

pub fn cast_name(module: SharedModule, func: &str) -> Result<'static, SharedContext> {
    let f = module.function(func).ok_or("Function not found")?;
    let mut context = spawn(module)?;
    context.name = Some(String::from(func));
    cast_inner(context, f)
}

pub fn cast_ptr(func: FuncPtr) -> Result<'static, SharedContext> {
    let context = spawn(func.0)?;
    cast_inner(context, func.1)
}

fn cast_inner(mut context: Context, func: ModuleFuncPtr) -> Result<'static, SharedContext> {
    context.function = func;
    context.status = Status::New;
    let (stack, address) = ContextMemory::new_kernel(
        65_536 / PAGE_SIZE,
        EntryFlags::GLOBAL | EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE,
    ).ok_or("Failed to allocate kernel stack")?;
    context
        .arch
        .set_stack(address.get() as usize + stack.len_bytes());
    context.kstack = Some(stack);

    Ok(context::contexts_mut().insert(context)?.clone())
}
