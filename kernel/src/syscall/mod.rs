//!
//! This module provides syscall definitions and the necessary resources to parse incoming
//! syscalls

extern crate syscall;

pub use self::syscall::{data, error, flag, io, number};

pub use self::driver::*;
pub use self::futex::futex;
pub use self::process::*;
pub use self::time::*;
pub use self::validate::*;

use self::data::TimeSpec;
use self::error::{Error, Result, EINVAL, ENOSYS};
use self::number::*;
use alloc::str::from_utf8;

use context::ContextId;
use interrupt::syscall::SyscallStack;

/// Debug
pub mod debug;

/// Driver syscalls
pub mod driver;

/// Fast userspace mutex
pub mod futex;

/// Process syscalls
pub mod process;

/// Time syscalls
pub mod time;

/// Validate input
pub mod validate;

/// This function is the syscall handler of the kernel, it is composed of an inner function that returns a `Result<usize>`. After the inner function runs, the syscall function calls [`Error::mux`] on it.
pub fn syscall(
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    bp: usize,
    stack: &mut SyscallStack,
) -> usize {
    #[inline(always)]
    fn inner(
        a: usize,
        b: usize,
        c: usize,
        d: usize,
        e: usize,
        f: usize,
        bp: usize,
        stack: &mut SyscallStack,
    ) -> Result<usize> {
        //SYS_* is declared in kernel/syscall/src/number.rs
        match a {
            SYS_YIELD => sched_yield(),
            SYS_NANOSLEEP => nanosleep(
                validate_slice(b as *const TimeSpec, 1).map(|req| &req[0])?,
                if c == 0 {
                    None
                } else {
                    Some(validate_slice_mut(c as *mut TimeSpec, 1).map(|rem| &mut rem[0])?)
                },
            ),
            SYS_CLOCK_GETTIME => clock_gettime(
                b,
                validate_slice_mut(c as *mut TimeSpec, 1).map(|time| &mut time[0])?,
            ),
            /*
            SYS_FUTEX => {
                futex(
                    validate_slice_mut(b as *mut i32, 1).map(
                        |uaddr| &mut uaddr[0],
                    )?,
                    c,
                    d as i32,
                    e,
                    f as *mut i32,
                )
            }
            */
            SYS_WRITE => {
                let slice = validate_slice(b as *const u8, c)?;
                let string = from_utf8(slice).map_err(|_| Error::new(EINVAL))?;
                let contexts = ::context::contexts();
                if let Some(context_lock) = contexts.current() {
                    let context = context_lock.read();
                    println!("{}: {}", context.name, string);
                }
                Ok(10)
            }
            SYS_BRK => brk(b),
            SYS_GETPID => getpid().map(ContextId::into),
            SYS_EXIT => exit((b & 0xFF) << 8),
            //SYS_KILL => kill(ContextId::from(b), c),
            //SYS_WAITPID => waitpid(ContextId::from(b), c, d).map(ContextId::into),
            SYS_IOPL => iopl(b, stack),
            SYS_PHYSALLOC => physalloc(b),
            SYS_PHYSFREE => physfree(b, c),
            SYS_PHYSMAP => physmap(b, c, d),
            SYS_PHYSUNMAP => physunmap(b),
            SYS_VIRTTOPHYS => virttophys(b),
            _ => Err(Error::new(ENOSYS)),
        }
    }

    /*
    let debug = {
        let contexts = ::context::contexts();
        if let Some(context_lock) = contexts.current() {
            let context = context_lock.read();
            let name_raw = context.name.lock();
            let name = unsafe { ::core::str::from_utf8_unchecked(&name_raw) };
            if name == "file:/bin/cargo" || name == "file:/bin/rustc" {
                if (a == SYS_WRITE || a == SYS_FSYNC) && (b == 1 || b == 2) {
                    false
                } else {
                    true
                }
            } else {
                false
            }
        } else {
            false
        }
/// This function
    };

    if debug {
        let contexts = ::context::contexts();
        if let Some(context_lock) = contexts.current() {
            let context = context_lock.read();
            print!("{} ({}): ", unsafe { ::core::str::from_utf8_unchecked(&context.name.lock()) }, context.id.into());
        }

/// This function
        println!("{}", debug::format_call(a, b, c, d, e, f));
    }


    */

    // The next lines set the current syscall in the context struct, then once the inner() function
    // completes, we set the current syscall to none.
    //
    // When the code below falls out of scope it will release the lock
    // see the spin crate for details

    let result = inner(a, b, c, d, e, f, bp, stack);

    /*
    if debug {
        let contexts = ::context::contexts();
        if let Some(context_lock) = contexts.current() {
            let context = context_lock.read();
            print!("{} ({}): ", unsafe { ::core::str::from_utf8_unchecked(&context.name.lock()) }, context.id.into());
        }

        print!("{} = ", debug::format_call(a, b, c, d, e, f));

        match result {
            Ok(ref ok) => {
                println!("Ok({} ({:#X}))", ok, ok);
            },
            Err(ref err) => {
                println!("Err({} ({:#X}))", err, err.errno);
            }
        }
    }
    */

    // errormux turns Result<usize> into -errno
    Error::mux(result)
}
