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

fn empty(context: &mut context::Context, reaping: bool) {
    if reaping {
        // Memory should already be unmapped
        assert!(context.image.is_empty());
        assert!(context.heap.is_none());
        assert!(context.stack.is_none());
    } else {
        // Unmap previous image, heap, grants, stack, and tls
        context.image.clear();
        drop(context.heap.take());
        drop(context.stack.take());
    }

    let grants = &mut context.grants;
    for grant in grants.drain(..) {
        if reaping {
            println!(
                "{}: {}: Grant should not exist: {:?}",
                context.id.into(),
                context.name,
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

pub fn exit(status: usize) -> ! {
    {
        let contexts_lock = context::contexts_mut();
        let context_lock = contexts_lock.current().expect("No current context");

        let (pid, parent) = {
            let mut context = context_lock.write();
            context.status = Status::Exited(status);
            (context.id, context.ret_link.take())
        };

        if let Some(parent) = parent {
            context::fuse_return(&context, &mut parent);
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

pub fn sigreturn() -> Result<usize> {
    {
        let contexts = context::contexts();
        let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
        let mut context = context_lock.write();
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
