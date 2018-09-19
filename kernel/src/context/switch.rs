use context::{arch, contexts, Context, SharedContext, Status, CONTEXT_ID};
use core::sync::atomic::Ordering;
use gdt;
use interrupt;
use interrupt::irq::PIT_TICKS;
use time;

unsafe fn update(context: &mut Context, cpu_id: usize) {
    // Take ownership if not already owned
    if context.cpu_id == None {
        context.cpu_id = Some(cpu_id);
        // println!("{}: take {} {}", cpu_id, context.id, ::core::str::from_utf8_unchecked(&context.name.lock()));
    }

    // Wake from sleep
    if context.status == Status::Blocked && context.wake.is_some() {
        let wake = context.wake.expect("context::switch: wake not set");

        let current = time::monotonic();
        if current.0 > wake.0 || (current.0 == wake.0 && current.1 >= wake.1) {
            context.wake = None;
            context.unblock();
        }
    }
}

fn runnable(context: &Context, cpu_id: usize) -> bool {
    // Switch to context if it needs to run, is not currently running, and is owned by the current CPU
    (context.status == Status::Runnable || context.status == Status::New)
        && context.cpu_id == Some(cpu_id)
}

/// Switch to the next context
///
/// # Safety
///
/// Do not call this while holding locks!
pub unsafe fn switch() -> bool {
    use core::ops::DerefMut;

    //println!("Switch called");

    //set PIT Interrupt counter to 0, giving each process same amount of PIT ticks
    PIT_TICKS.store(0, Ordering::SeqCst);

    // Set the global lock to avoid the unsafe operations below from causing issues
    while arch::CONTEXT_SWITCH_LOCK.compare_and_swap(false, true, Ordering::SeqCst) {
        interrupt::pause();
    }

    let cpu_id = ::cpu_id();

    let from_ptr;
    let mut to_ptr = 0 as *mut Context;
    {
        let contexts = contexts();
        {
            let context_lock = contexts
                .current()
                .expect("context::switch: not inside of context");
            let mut context = context_lock.write();
            from_ptr = context.deref_mut() as *mut Context;
        }

        for (_pid, context_lock) in contexts.iter() {
            let mut context = context_lock.write();
            update(&mut context, cpu_id);
        }

        for (pid, context_lock) in contexts.iter() {
            if *pid > (*from_ptr).id {
                let mut context = context_lock.write();
                if runnable(&mut context, cpu_id) {
                    to_ptr = context.deref_mut() as *mut Context;
                    break;
                }
            }
        }

        if to_ptr as usize == 0 {
            for (pid, context_lock) in contexts.iter() {
                if *pid < (*from_ptr).id {
                    let mut context = context_lock.write();
                    if runnable(&mut context, cpu_id) {
                        to_ptr = context.deref_mut() as *mut Context;
                        break;
                    }
                }
            }
        }
    };
    let from = &mut *from_ptr;
    // Switch process states, TSS stack pointer, and store new context ID
    let mut to_user = false;
    if to_ptr as usize != 0 {
        let to = &mut *to_ptr;
        // NOTE is this correct assumption?
        if from.status == Status::Running {
            from.status = Status::Runnable;
        }
        if to.status == Status::New {
            to_user = true;
        }
        to.status = Status::Running;
        if let Some(ref stack) = to.kstack {
            gdt::set_tss_stack(
                stack
                    .kernel_address()
                    .expect("Kernel stack is not mapped")
                    .get() as usize
                    + stack.len_bytes(),
            );
        }
        CONTEXT_ID.store(to.id, Ordering::SeqCst);
    }

    // Unset global lock before switch, as arch is only usable by the current CPU at this time
    arch::CONTEXT_SWITCH_LOCK.store(false, Ordering::SeqCst);

    if to_ptr as usize == 0 {
        // No target was found, return
        false
    } else {
        let to = &mut *to_ptr;
        println!("Switch gonna switch {:?}, {}", to.id, to_user);
        if to_user {
            let sp = ::USER_STACK_OFFSET + ::USER_STACK_SIZE - 256;
            from.arch.switch_user(
                &mut to.arch,
                to.function,
                sp,
                to.args
                    .as_ref()
                    .map(|a| a.context_address().get())
                    .unwrap_or(0),
            );
        } else {
            from.arch.switch_to(&mut to.arch);
        }
        true
    }
}

/// Switch to the next context
///
/// # Safety
///
/// Do not call this while holding locks!
pub unsafe fn fuse_return(from_context: SharedContext, to_context: SharedContext) -> () {
    use core::ops::{Deref, DerefMut};

    //println!("Fuse return called");

    // Set the global lock to avoid the unsafe operations below from causing issues
    while arch::CONTEXT_SWITCH_LOCK.compare_and_swap(false, true, Ordering::SeqCst) {
        interrupt::pause();
    }

    // NOTE It is neccessary to leak pointers, as when arch::switch_* is called the locks will remain being held.
    let (from_ptr, to_ptr) = {
        // Switch process states, TSS stack pointer, and store new context ID
        // NOTE is this correct assumption?
        let mut from = from_context
            .try_write()
            .expect("You must not hold locks to contexts being switched");
        from.unblock();
        let mut to = to_context
            .try_write()
            .expect("You must not hold locks to contexts being switched");
        to.status = Status::Running;
        if let Some(ref stack) = to.kstack {
            gdt::set_tss_stack(
                stack
                    .kernel_address()
                    .expect("Kernel stack is not mapped")
                    .get() as usize
                    + stack.len_bytes(),
            );
        }
        CONTEXT_ID.store(to.id, Ordering::SeqCst);

        (
            from.deref() as *const Context,
            to.deref_mut() as *mut Context,
        )
    };

    // Unset global lock before switch, as arch is only usable by the current CPU at this time
    arch::CONTEXT_SWITCH_LOCK.store(false, Ordering::SeqCst);

    println!("Return to {:?}", (*to_ptr).id);

    (&*from_ptr)
        .arch
        .switch_discarding(&mut (&mut *to_ptr).arch);
}

pub unsafe fn fuse_switch(to_context: SharedContext, func: usize) -> () {
    use core::ops::DerefMut;
    //set PIT Interrupt counter to 0, giving each process same amount of PIT ticks
    PIT_TICKS.store(0, Ordering::SeqCst);

    // Set the global lock to avoid the unsafe operations below from causing issues
    while arch::CONTEXT_SWITCH_LOCK.compare_and_swap(false, true, Ordering::SeqCst) {
        interrupt::pause();
    }

    let (from_ptr, to_ptr) = {
        let mut to = to_context
            .try_write()
            .expect("You must not hold locks to contexts being switched");

        to.status = Status::Running;

        CONTEXT_ID.store(to.id, Ordering::SeqCst);

        // NOTE I'm not so sure about that, switches stack tss.
        if let Some(ref stack) = to.kstack {
            gdt::set_tss_stack(
                stack
                    .kernel_address()
                    .expect("Kernel stack is not mapped")
                    .get() as usize
                    + stack.len_bytes(),
            );
        }

        let from = {
            let from_context = to
                .ret_link
                .as_mut()
                .expect("Attempting to fuse without parent");
            let mut from = from_context
                .try_write()
                .expect("You must not hold locks to contexts being switched");
            from.block();
            from.deref_mut() as *mut Context
        };

        (from, to.deref_mut() as *mut Context)
    };

    // Unset global lock before switch, as arch is only usable by the current CPU at this time
    arch::CONTEXT_SWITCH_LOCK.store(false, Ordering::SeqCst);

    let sp = ::USER_STACK_OFFSET + ::USER_STACK_SIZE - 256;

    let to = &mut *to_ptr;

    (&mut *from_ptr).arch.switch_user(
        &mut to.arch,
        func,
        sp,
        to.args
            .as_ref()
            .map(|a| a.context_address().get())
            .unwrap_or(0),
    );
}
