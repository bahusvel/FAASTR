use alloc::arc::Arc;
use alloc::boxed::Box;
use alloc::{BTreeMap, Vec};
use core::alloc::{GlobalAlloc, Layout};
use core::{intrinsics, mem, str};
use spin::Mutex;

use context;
use context::{ContextId, Status, WaitpidKey};
#[cfg(not(feature = "doc"))]
use elf::{self, program_header};
use interrupt;
use memory::allocate_frames;
use paging::entry::EntryFlags;
use paging::temporary_page::TemporaryPage;
use paging::{ActivePageTable, InactivePageTable, Page, VirtualAddress};
use start::usermode;

use syscall::data::SigAction;
use syscall::error::*;
use syscall::flag::{
    wifcontinued, wifstopped, CLONE_SIGHAND, CLONE_VFORK, CLONE_VM, SIGCONT, SIGTERM, SIG_DFL,
    WCONTINUED, WNOHANG, WUNTRACED,
};
use syscall::validate::{validate_slice, validate_slice_mut};

pub fn brk(address: usize) -> Result<usize> {
    let contexts = context::contexts();
    let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
    let context = context_lock.read();

    //println!("{}: {}: BRK {:X}", unsafe { ::core::str::from_utf8_unchecked(&context.name.lock()) },
    //                             context.id.into(), address);

    let current = if let Some(ref heap_shared) = context.heap {
        heap_shared.with(|heap| heap.start_address().get() + heap.size())
    } else {
        panic!("user heap not initialized");
    };

    if address == 0 {
        //println!("Brk query {:X}", current);
        Ok(current)
    } else if address >= ::USER_HEAP_OFFSET {
        //TODO: out of memory errors
        if let Some(ref heap_shared) = context.heap {
            heap_shared.with(|heap| {
                heap.resize(address - ::USER_HEAP_OFFSET, true);
            });
        } else {
            panic!("user heap not initialized");
        }

        //println!("Brk resize {:X}", address);
        Ok(address)
    } else {
        //println!("Brk no mem");
        Err(Error::new(ENOMEM))
    }
}

pub fn clone(flags: usize, stack_base: usize) -> Result<ContextId> {
    let ppid;
    let pid;
    {
        let mut cpu_id = None;
        let arch;
        let vfork;
        let mut kfx_option = None;
        let mut kstack_option = None;
        let mut offset = 0;
        let mut image = vec![];
        let mut heap_option = None;
        let mut stack_option = None;
        let mut sigstack_option = None;
        let grants;
        let name;
        let env;
        let actions;

        // Copy from old process
        {
            let contexts = context::contexts();
            let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
            let context = context_lock.read();

            ppid = context.id;

            if flags & CLONE_VM == CLONE_VM {
                cpu_id = context.cpu_id;
            }

            arch = context.arch.clone();

            if let Some(ref fx) = context.kfx {
                let mut new_fx = unsafe {
                    Box::from_raw(
                        ::ALLOCATOR.alloc(Layout::from_size_align_unchecked(512, 16))
                            as *mut [u8; 512],
                    )
                };
                for (new_b, b) in new_fx.iter_mut().zip(fx.iter()) {
                    *new_b = *b;
                }
                kfx_option = Some(new_fx);
            }

            if let Some(ref stack) = context.kstack {
                offset = stack_base - stack.as_ptr() as usize - mem::size_of::<usize>(); // Add clone ret
                let mut new_stack = stack.clone();

                unsafe {
                    let func_ptr = new_stack.as_mut_ptr().offset(offset as isize);
                    *(func_ptr as *mut usize) = interrupt::syscall::clone_ret as usize;
                }

                kstack_option = Some(new_stack);
            }

            if flags & CLONE_VM == CLONE_VM {
                for memory_shared in context.image.iter() {
                    image.push(memory_shared.clone());
                }

                if let Some(ref heap_shared) = context.heap {
                    heap_option = Some(heap_shared.clone());
                }
            } else {
                for memory_shared in context.image.iter() {
                    memory_shared.with(|memory| {
                        let mut new_memory = context::memory::Memory::new(
                            VirtualAddress::new(memory.start_address().get() + ::USER_TMP_OFFSET),
                            memory.size(),
                            EntryFlags::PRESENT | EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                            false,
                        );

                        unsafe {
                            intrinsics::copy(
                                memory.start_address().get() as *const u8,
                                new_memory.start_address().get() as *mut u8,
                                memory.size(),
                            );
                        }

                        new_memory.remap(memory.flags());
                        image.push(new_memory.to_shared());
                    });
                }

                if let Some(ref heap_shared) = context.heap {
                    heap_shared.with(|heap| {
                        let mut new_heap = context::memory::Memory::new(
                            VirtualAddress::new(::USER_TMP_HEAP_OFFSET),
                            heap.size(),
                            EntryFlags::PRESENT | EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                            false,
                        );

                        unsafe {
                            intrinsics::copy(
                                heap.start_address().get() as *const u8,
                                new_heap.start_address().get() as *mut u8,
                                heap.size(),
                            );
                        }

                        new_heap.remap(heap.flags());
                        heap_option = Some(new_heap.to_shared());
                    });
                }
            }

            if let Some(ref stack) = context.stack {
                let mut new_stack = context::memory::Memory::new(
                    VirtualAddress::new(::USER_TMP_STACK_OFFSET),
                    stack.size(),
                    EntryFlags::PRESENT | EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                    false,
                );

                unsafe {
                    intrinsics::copy(
                        stack.start_address().get() as *const u8,
                        new_stack.start_address().get() as *mut u8,
                        stack.size(),
                    );
                }

                new_stack.remap(stack.flags());
                stack_option = Some(new_stack);
            }

            if let Some(ref sigstack) = context.sigstack {
                let mut new_sigstack = context::memory::Memory::new(
                    VirtualAddress::new(::USER_TMP_SIGSTACK_OFFSET),
                    sigstack.size(),
                    EntryFlags::PRESENT | EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                    false,
                );

                unsafe {
                    intrinsics::copy(
                        sigstack.start_address().get() as *const u8,
                        new_sigstack.start_address().get() as *mut u8,
                        sigstack.size(),
                    );
                }

                new_sigstack.remap(sigstack.flags());
                sigstack_option = Some(new_sigstack);
            }

            grants = Vec::new();

            // Copy the name
            name = context.name.clone();

            //Copy the environment
            let mut new_env = BTreeMap::new();
            for item in context.env.iter() {
                new_env.insert(item.0.clone(), Arc::new(Mutex::new(item.1.lock().clone())));
            }
            env = new_env;

            if flags & CLONE_SIGHAND == CLONE_SIGHAND {
                actions = Arc::clone(&context.actions);
            } else {
                actions = Arc::new(Mutex::new(context.actions.lock().clone()));
            }
        }

        // If vfork, block the current process
        // This has to be done after the operations that may require context switches
        if flags & CLONE_VFORK == CLONE_VFORK {
            let contexts = context::contexts();
            let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
            let mut context = context_lock.write();
            context.block();
            vfork = true;
        } else {
            vfork = false;
        }

        // Set up new process
        {
            let mut contexts = context::contexts_mut();
            let context_lock = contexts.new_context()?;
            let mut context = context_lock.write();

            pid = context.id;

            context.ppid = ppid;

            context.cpu_id = cpu_id;

            context.status = context::Status::Runnable;

            context.vfork = vfork;

            context.arch = arch;

            let mut active_table = unsafe { ActivePageTable::new() };

            let mut temporary_page = TemporaryPage::new(Page::containing_address(
                VirtualAddress::new(::USER_TMP_MISC_OFFSET),
            ));

            let mut new_table = {
                let frame = allocate_frames(1).expect("no more frames in syscall::clone new_table");
                InactivePageTable::new(frame, &mut active_table, &mut temporary_page)
            };

            context.arch.set_page_table(unsafe { new_table.address() });

            // Copy kernel image mapping
            {
                let frame = active_table.p4()[::KERNEL_PML4]
                    .pointed_frame()
                    .expect("kernel image not mapped");
                let flags = active_table.p4()[::KERNEL_PML4].flags();
                active_table.with(&mut new_table, &mut temporary_page, |mapper| {
                    mapper.p4_mut()[::KERNEL_PML4].set(frame, flags);
                });
            }

            // Copy kernel heap mapping
            {
                let frame = active_table.p4()[::KERNEL_HEAP_PML4]
                    .pointed_frame()
                    .expect("kernel heap not mapped");
                let flags = active_table.p4()[::KERNEL_HEAP_PML4].flags();
                active_table.with(&mut new_table, &mut temporary_page, |mapper| {
                    mapper.p4_mut()[::KERNEL_HEAP_PML4].set(frame, flags);
                });
            }

            if let Some(fx) = kfx_option.take() {
                context.arch.set_fx(fx.as_ptr() as usize);
                context.kfx = Some(fx);
            }

            // Set kernel stack
            if let Some(stack) = kstack_option.take() {
                context.arch.set_stack(stack.as_ptr() as usize + offset);
                context.kstack = Some(stack);
            }

            // Setup heap
            if flags & CLONE_VM == CLONE_VM {
                // Copy user image mapping, if found
                if !image.is_empty() {
                    let frame = active_table.p4()[::USER_PML4]
                        .pointed_frame()
                        .expect("user image not mapped");
                    let flags = active_table.p4()[::USER_PML4].flags();
                    active_table.with(&mut new_table, &mut temporary_page, |mapper| {
                        mapper.p4_mut()[::USER_PML4].set(frame, flags);
                    });
                }
                context.image = image;

                // Copy user heap mapping, if found
                if let Some(heap_shared) = heap_option {
                    let frame = active_table.p4()[::USER_HEAP_PML4]
                        .pointed_frame()
                        .expect("user heap not mapped");
                    let flags = active_table.p4()[::USER_HEAP_PML4].flags();
                    active_table.with(&mut new_table, &mut temporary_page, |mapper| {
                        mapper.p4_mut()[::USER_HEAP_PML4].set(frame, flags);
                    });
                    context.heap = Some(heap_shared);
                }

                context.grants = grants;
            } else {
                // Copy percpu mapping
                for cpu_id in 0..::cpu_count() {
                    extern "C" {
                        // The starting byte of the thread data segment
                        static mut __tdata_start: u8;
                        // The ending byte of the thread BSS segment
                        static mut __tbss_end: u8;
                    }

                    let size = unsafe {
                        &__tbss_end as *const _ as usize - &__tdata_start as *const _ as usize
                    };

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

                // Move copy of image
                for memory_shared in image.iter_mut() {
                    memory_shared.with(|memory| {
                        let start = VirtualAddress::new(
                            memory.start_address().get() - ::USER_TMP_OFFSET + ::USER_OFFSET,
                        );
                        memory.move_to(start, &mut new_table, &mut temporary_page);
                    });
                }
                context.image = image;

                // Move copy of heap
                if let Some(heap_shared) = heap_option {
                    heap_shared.with(|heap| {
                        heap.move_to(
                            VirtualAddress::new(::USER_HEAP_OFFSET),
                            &mut new_table,
                            &mut temporary_page,
                        );
                    });
                    context.heap = Some(heap_shared);
                }
            }

            // Setup user stack
            if let Some(mut stack) = stack_option {
                stack.move_to(
                    VirtualAddress::new(::USER_STACK_OFFSET),
                    &mut new_table,
                    &mut temporary_page,
                );
                context.stack = Some(stack);
            }

            // Setup user sigstack
            if let Some(mut sigstack) = sigstack_option {
                sigstack.move_to(
                    VirtualAddress::new(::USER_SIGSTACK_OFFSET),
                    &mut new_table,
                    &mut temporary_page,
                );
                context.sigstack = Some(sigstack);
            }

            context.name = name;

            context.env = env;

            context.actions = actions;
        }
    }

    let _ = unsafe { context::switch() };

    Ok(pid)
}

fn empty(context: &mut context::Context, reaping: bool) {
    if reaping {
        // Memory should already be unmapped
        assert!(context.image.is_empty());
        assert!(context.heap.is_none());
        assert!(context.stack.is_none());
        assert!(context.sigstack.is_none());
    } else {
        // Unmap previous image, heap, grants, stack, and tls
        context.image.clear();
        drop(context.heap.take());
        drop(context.stack.take());
        drop(context.sigstack.take());
    }

    let grants = &mut context.grants;
    for grant in grants.drain(..) {
        if reaping {
            println!(
                "{}: {}: Grant should not exist: {:?}",
                context.id.into(),
                unsafe { ::core::str::from_utf8_unchecked(&context.name) },
                grant
            );

            let mut new_table =
                unsafe { InactivePageTable::from_address(context.arch.get_page_table()) };
            let mut temporary_page = TemporaryPage::new(Page::containing_address(
                VirtualAddress::new(::USER_TMP_GRANT_OFFSET),
            ));

            grant.unmap_inactive(&mut new_table, &mut temporary_page);
        } else {
            grant.unmap();
        }
    }
}

fn exec_noreturn(canonical: Box<[u8]>, data: Box<[u8]>, args: Box<[Box<[u8]>]>) -> ! {
    let entry;
    let mut sp = ::USER_STACK_OFFSET + ::USER_STACK_SIZE - 256;

    {
        let (vfork, ppid) = {
            let contexts = context::contexts();
            let context_lock = contexts
                .current()
                .ok_or(Error::new(ESRCH))
                .expect("exec_noreturn pid not found");
            let mut context = context_lock.write();

            // Set name
            context.name = canonical;

            empty(&mut context, false);

            {
                let elf = elf::Elf::from(&data).unwrap();
                entry = elf.entry();
                for segment in elf.segments() {
                    if segment.p_type == program_header::PT_LOAD {
                        let voff = segment.p_vaddr % 4096;
                        let vaddr = segment.p_vaddr - voff;

                        let mut memory = context::memory::Memory::new(
                            VirtualAddress::new(vaddr as usize),
                            segment.p_memsz as usize + voff as usize,
                            EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                            true,
                        );

                        unsafe {
                            // Copy file data
                            intrinsics::copy(
                                (elf.data.as_ptr() as usize + segment.p_offset as usize)
                                    as *const u8,
                                segment.p_vaddr as *mut u8,
                                segment.p_filesz as usize,
                            );
                        }

                        let mut flags = EntryFlags::NO_EXECUTE | EntryFlags::USER_ACCESSIBLE;

                        if segment.p_flags & program_header::PF_R == program_header::PF_R {
                            flags.insert(EntryFlags::PRESENT);
                        }

                        // W ^ X. If it is executable, do not allow it to be writable, even if requested
                        if segment.p_flags & program_header::PF_X == program_header::PF_X {
                            flags.remove(EntryFlags::NO_EXECUTE);
                        } else if segment.p_flags & program_header::PF_W == program_header::PF_W {
                            flags.insert(EntryFlags::WRITABLE);
                        }

                        memory.remap(flags);

                        context.image.push(memory.to_shared());
                    }
                }
            }

            // Data no longer required, can deallocate
            drop(data);

            // Map heap
            context.heap = Some(
                context::memory::Memory::new(
                    VirtualAddress::new(::USER_HEAP_OFFSET),
                    0,
                    EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
                    true,
                ).to_shared(),
            );

            // Map stack
            context.stack = Some(context::memory::Memory::new(
                VirtualAddress::new(::USER_STACK_OFFSET),
                ::USER_STACK_SIZE,
                EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
                true,
            ));

            // Map stack
            context.sigstack = Some(context::memory::Memory::new(
                VirtualAddress::new(::USER_SIGSTACK_OFFSET),
                ::USER_SIGSTACK_SIZE,
                EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
                true,
            ));

            // Push arguments
            let mut arg_size = 0;
            for arg in args.iter().rev() {
                sp -= mem::size_of::<usize>();
                unsafe {
                    *(sp as *mut usize) = ::USER_ARG_OFFSET + arg_size;
                }
                sp -= mem::size_of::<usize>();
                unsafe {
                    *(sp as *mut usize) = arg.len();
                }

                arg_size += arg.len();
            }

            sp -= mem::size_of::<usize>();
            unsafe {
                *(sp as *mut usize) = args.len();
            }

            if arg_size > 0 {
                let mut memory = context::memory::Memory::new(
                    VirtualAddress::new(::USER_ARG_OFFSET),
                    arg_size,
                    EntryFlags::NO_EXECUTE | EntryFlags::WRITABLE,
                    true,
                );

                let mut arg_offset = 0;
                for arg in args.iter().rev() {
                    unsafe {
                        intrinsics::copy(
                            arg.as_ptr(),
                            (::USER_ARG_OFFSET + arg_offset) as *mut u8,
                            arg.len(),
                        );
                    }

                    arg_offset += arg.len();
                }

                memory.remap(EntryFlags::NO_EXECUTE | EntryFlags::USER_ACCESSIBLE);

                context.image.push(memory.to_shared());
            }

            // Args no longer required, can deallocate
            drop(args);

            context.actions = Arc::new(Mutex::new(vec![
                (
                    SigAction {
                        sa_handler: unsafe { mem::transmute(SIG_DFL) },
                        sa_mask: [0; 2],
                        sa_flags: 0,
                    },
                    0
                );
                128
            ]));

            let vfork = context.vfork;
            context.vfork = false;

            (vfork, context.ppid)
        };

        if vfork {
            let contexts = context::contexts();
            if let Some(context_lock) = contexts.get(ppid) {
                let mut context = context_lock.write();
                if !context.unblock() {
                    println!("{} not blocked for exec vfork unblock", ppid.into());
                }
            } else {
                println!("{} not found for exec vfork unblock", ppid.into());
            }
        }
    }

    // Go to usermode
    unsafe {
        usermode(entry, sp, 0);
    }
}

pub fn exec(name: &[u8], data: &[u8], arg_ptrs: &[[usize; 2]]) -> Result<usize> {
    let mut args = Vec::new();
    for arg_ptr in arg_ptrs {
        let arg = validate_slice(arg_ptr[0] as *const u8, arg_ptr[1])?;
        // Argument must be moved into kernel space before exec unmaps all memory
        args.push(arg.to_vec().into_boxed_slice());
    }

    // The argument list is limited to avoid using too much userspace stack
    // This check is done last to allow all hashbangs to be resolved
    //
    // This should be based on the size of the userspace stack, divided
    // by the cost of each argument, which should be usize * 2, with
    // one additional argument added to represent the total size of the
    // argument pointer array and potential padding
    //
    // A limit of 4095 would mean a stack of (4095 + 1) * 8 * 2 = 65536, or 64KB
    if args.len() > 4095 {
        return Err(Error::new(E2BIG));
    }

    match elf::Elf::from(&data) {
        Ok(elf) => {
            // We check the validity of all loadable sections here
            for segment in elf.segments() {
                if segment.p_type == program_header::PT_LOAD {
                    let voff = segment.p_vaddr % 4096;
                    let vaddr = segment.p_vaddr - voff;

                    // Due to the Userspace and kernel TLS bases being located right above 2GB,
                    // limit any loadable sections to lower than that. Eventually we will need
                    // to replace this with a more intelligent TLS address
                    if vaddr >= 0x8000_0000 {
                        println!("exec: invalid section address {:X}", segment.p_vaddr);
                        return Err(Error::new(ENOEXEC));
                    }
                }
            }
        }
        Err(err) => {
            println!(
                "exec: failed to execute {}: {}",
                unsafe { str::from_utf8_unchecked(name) },
                err
            );
            return Err(Error::new(ENOEXEC));
        }
    }

    // Drop so that usage is not allowed after unmapping context
    drop(name);
    drop(arg_ptrs);

    // This is the point of no return, quite literaly. Any checks for validity need
    // to be done before, and appropriate errors returned. Otherwise, we have nothing
    // to return to.
    exec_noreturn(
        Vec::from(name).into_boxed_slice(),
        Vec::from(data).into_boxed_slice(),
        args.into_boxed_slice(),
    );
}

pub fn exit(status: usize) -> ! {
    {
        let context_lock = {
            let contexts = context::contexts();
            let context_lock = contexts
                .current()
                .ok_or(Error::new(ESRCH))
                .expect("exit failed to find context");
            Arc::clone(&context_lock)
        };

        // PGID and PPID must be grabbed after close, as context switches could change PGID or PPID if parent exits
        let (pid, ppid) = {
            let context = context_lock.read();
            (context.id, context.ppid)
        };

        // Deallocate and Notify children of parent death.
        let (vfork, children) = {
            let mut context = context_lock.write();

            empty(&mut context, false);

            let vfork = context.vfork;
            context.vfork = false;

            context.status = context::Status::Exited(status);

            let children = context.waitpid.receive_all();

            (vfork, children)
        };

        // Unblock parent and notify waiters
        {
            let contexts = context::contexts();
            if let Some(parent_lock) = contexts.get(ppid) {
                let waitpid = {
                    let mut parent = parent_lock.write();
                    if vfork {
                        if !parent.unblock() {
                            println!(
                                "{}: {} not blocked for exit vfork unblock",
                                pid.into(),
                                ppid.into()
                            );
                        }
                    }
                    Arc::clone(&parent.waitpid)
                };

                for (c_pid, c_status) in children {
                    waitpid.send(c_pid, c_status);
                }

                waitpid.send(
                    WaitpidKey {
                        pid: Some(pid),
                        pgid: Some(ppid),
                    },
                    (pid, status),
                );
            } else {
                println!(
                    "{}: {} not found for exit vfork unblock",
                    pid.into(),
                    ppid.into()
                );
            }
        }

        // Stop CPU if kernel exits.
        if pid == ContextId::from(1) {
            println!("Main kernel thread exited with status {:X}", status);

            extern "C" {
                fn kreset() -> !;
                fn kstop() -> !;
            }

            if status == SIGTERM {
                unsafe {
                    kreset();
                }
            } else {
                unsafe {
                    kstop();
                }
            }
        } else {
            //reap(pid).expect("Failed to reap context");
        }
        println!("PID {:?} exited", pid);
    }

    let _ = unsafe { context::switch() };

    unreachable!();
}

pub fn getpid() -> Result<ContextId> {
    let contexts = context::contexts();
    let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
    let context = context_lock.read();
    Ok(context.id)
}

pub fn kill(pid: ContextId, sig: usize) -> Result<usize> {
    if sig < 0x7F {
        let mut found = 0;
        let mut sent = 0;

        {
            let contexts = context::contexts();
            let send = |context: &mut context::Context| -> bool {
                // If sig = 0, test that process exists and can be
                // signalled, but don't send any signal.
                if sig != 0 {
                    context.pending.push_back(sig as u8);
                    // Convert stopped processes to blocked if sending SIGCONT
                    if sig == SIGCONT {
                        if let context::Status::Stopped(_sig) = context.status {
                            context.status = context::Status::Blocked;
                        }
                    }
                }
                true
            };

            if pid.into() as isize > 0 {
                // Send to a single process
                if let Some(context_lock) = contexts.get(pid) {
                    let mut context = context_lock.write();

                    found += 1;
                    if send(&mut context) {
                        sent += 1;
                    }
                }
            } else if pid.into() as isize == -1 {
                // Send to every process with permission, except for init
                for (_id, context_lock) in contexts.iter() {
                    let mut context = context_lock.write();

                    if context.id.into() > 2 {
                        found += 1;

                        if send(&mut context) {
                            sent += 1;
                        }
                    }
                }
            }
        }

        if found == 0 {
            Err(Error::new(ESRCH))
        } else if sent == 0 {
            Err(Error::new(EPERM))
        } else {
            // Switch to ensure delivery to self
            unsafe {
                context::switch();
            }

            Ok(0)
        }
    } else {
        Err(Error::new(EINVAL))
    }
}

pub fn sigaction(
    sig: usize,
    act_opt: Option<&SigAction>,
    oldact_opt: Option<&mut SigAction>,
    restorer: usize,
) -> Result<usize> {
    if sig > 0 && sig <= 0x7F {
        let contexts = context::contexts();
        let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
        let context = context_lock.read();
        let mut actions = context.actions.lock();

        if let Some(oldact) = oldact_opt {
            *oldact = actions[sig].0;
        }

        if let Some(act) = act_opt {
            actions[sig] = (*act, restorer);
        }

        Ok(0)
    } else {
        Err(Error::new(EINVAL))
    }
}

pub fn sigreturn() -> Result<usize> {
    {
        let contexts = context::contexts();
        let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
        let mut context = context_lock.write();
        context.ksig_restore = true;
        context.block();
    }

    let _ = unsafe { context::switch() };

    unreachable!();
}

fn reap(pid: ContextId) -> Result<ContextId> {
    // Spin until not running
    let mut status = Status::Running;
    while status == Status::Running {
        {
            let contexts = context::contexts();
            let context_lock = contexts.get(pid).ok_or(Error::new(ESRCH))?;
            let context = context_lock.read();
            status = context.status;
        }

        interrupt::pause();
    }

    let mut contexts = context::contexts_mut();
    let context_lock = contexts.remove(pid).ok_or(Error::new(ESRCH))?;
    {
        let mut context = context_lock.write();
        empty(&mut context, true);
    }
    drop(context_lock);

    Ok(pid)
}

pub fn waitpid(pid: ContextId, status_ptr: usize, flags: usize) -> Result<ContextId> {
    let (ppid, waitpid) = {
        let contexts = context::contexts();
        let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
        let context = context_lock.read();
        (context.id, Arc::clone(&context.waitpid))
    };

    let mut tmp = [0];
    let status_slice = if status_ptr != 0 {
        validate_slice_mut(status_ptr as *mut usize, 1)?
    } else {
        &mut tmp
    };

    let mut grim_reaper = |w_pid: ContextId, status: usize| -> Option<Result<ContextId>> {
        if wifcontinued(status) {
            if flags & WCONTINUED == WCONTINUED {
                status_slice[0] = status;
                Some(Ok(w_pid))
            } else {
                None
            }
        } else if wifstopped(status) {
            if flags & WUNTRACED == WUNTRACED {
                status_slice[0] = status;
                Some(Ok(w_pid))
            } else {
                None
            }
        } else {
            status_slice[0] = status;
            Some(reap(w_pid))
        }
    };

    loop {
        let res_opt = if pid.into() == 0 {
            // Check for existence of child
            {
                let mut found = false;

                let contexts = context::contexts();
                for (_id, context_lock) in contexts.iter() {
                    let context = context_lock.read();
                    if context.ppid == ppid {
                        found = true;
                        break;
                    }
                }

                if !found {
                    return Err(Error::new(ECHILD));
                }
            }

            if flags & WNOHANG == WNOHANG {
                if let Some((_wid, (w_pid, status))) = waitpid.receive_any_nonblock() {
                    grim_reaper(w_pid, status)
                } else {
                    Some(Ok(ContextId::from(0)))
                }
            } else {
                let (_wid, (w_pid, status)) = waitpid.receive_any();
                grim_reaper(w_pid, status)
            }
        } else {
            let hack_status = {
                let contexts = context::contexts();
                let context_lock = contexts.get(pid).ok_or(Error::new(ECHILD))?;
                let mut context = context_lock.write();
                if context.ppid != ppid {
                    println!(
                        "Hack for rustc - changing ppid of {} from {} to {}",
                        context.id.into(),
                        context.ppid.into(),
                        ppid.into()
                    );
                    context.ppid = ppid;
                    //return Err(Error::new(ECHILD));
                    Some(context.status)
                } else {
                    None
                }
            };

            if let Some(context::Status::Exited(status)) = hack_status {
                let _ = waitpid.receive_nonblock(&WaitpidKey {
                    pid: Some(pid),
                    pgid: None,
                });
                grim_reaper(pid, status)
            } else if flags & WNOHANG == WNOHANG {
                if let Some((w_pid, status)) = waitpid.receive_nonblock(&WaitpidKey {
                    pid: Some(pid),
                    pgid: None,
                }) {
                    grim_reaper(w_pid, status)
                } else {
                    Some(Ok(ContextId::from(0)))
                }
            } else {
                let (w_pid, status) = waitpid.receive(&WaitpidKey {
                    pid: Some(pid),
                    pgid: None,
                });
                grim_reaper(w_pid, status)
            }
        };

        if let Some(res) = res_opt {
            return res;
        }
    }
}
