//!
//! This module provides syscall definitions and the necessary resources to parse incoming
//! syscalls

extern crate syscall;

pub use self::syscall::{data, error, flag, io, number};

pub use self::driver::*;
//pub use self::futex::futex;
pub use self::call::*;
pub use self::process::*;
pub use self::time::*;
pub use self::validate::*;

use self::call::*;
use self::number::*;
use alloc::vec::Vec;
use context;
use core::convert::TryInto;
use sos::{EncodedValues, JustError, Value};

use interrupt::syscall::SyscallStack;

/// Debug
pub mod debug;

/// Driver syscalls
pub mod driver;

/// Fast userspace mutex
//pub mod futex;

/// Process syscalls
pub mod process;

/// Time syscalls
pub mod time;

/// Validate input
pub mod validate;

mod call;

/// This function is the syscall handler of the kernel, it is composed of an inner function that returns a `Result<usize>`. After the inner function runs, the syscall function calls [`Error::mux`] on it.
pub fn syscall(a: usize, b: usize, c: usize, stack: &mut SyscallStack) -> usize {
    #[inline(always)]
    fn inner<'a, 'b>(
        a: usize,
        args: EncodedValues<'a>,
        _stack: &mut SyscallStack,
    ) -> Result<(usize), JustError<'static>> {
        //SYS_* is declared in kernel/syscall/src/number.rs
        let ret = match a {
            SYS_FUSE => sys_fuse(args),
            SYS_CAST => {
                sys_cast(args)?;
                Ok(EncodedValues::from(Vec::new()))
            }
            SYS_RETURN => sys_return(args),
            SYS_WRITE => {
                let string: &str = args
                    .decode()
                    .ok_or(JustError::new("Could not decode SOS"))?
                    .next()
                    .ok_or(JustError::new("First argument to write must be String"))?
                    .try_into()
                    .map_err(|e| JustError::new(e))?;

                let contexts = ::context::contexts();
                if let Some(context_lock) = contexts.current() {
                    let context = context_lock.read();
                    println!("{}: {}", context.name(), string);
                }
                Ok(sos![Value::UInt64(string.len() as u64)].into())
            }
            /*
            SYS_YIELD => {
                sched_yield().map_err(|_| JustError::new("Scheduler operation failed"))?;
                Ok(Vec::new())
            }
            SYS_NANOSLEEP => {
                nanosleep(
                    validate_slice(b as *const TimeSpec, 1).map(|req| &req[0])?,
                    if c == 0 {
                        None
                    } else {
                        Some(validate_slice_mut(c as *mut TimeSpec, 1).map(|rem| &mut rem[0])?)
                    },
                ).map_err(|_| JustError::new("Time operation failed"))?;
                Ok(Vec::new())
            }
            SYS_CLOCK_GETTIME => {
                clock_gettime(
                    b,
                    validate_slice_mut(c as *mut TimeSpec, 1).map(|time| &mut time[0])?,
                ).map_err(|_| JustError::new("Time operation failed"))?;
                Ok(Vec::new())
            }
            SYS_BRK => {
                brk(b).map_err(|_| JustError::new("Memory operation failed"))?;
                Ok(Vec::new())
            }
            SYS_EXIT => {
                exit((b & 0xFF) << 8);
            }
            SYS_KILL => kill(ContextId::from(b), c),
            SYS_WAITPID => waitpid(ContextId::from(b), c, d).map(ContextId::into),
            SYS_IOPL => {
                iopl(b, stack).map_err(|_| JustError::new("IOPL failed"))?;
                Ok(Vec::new())
            }
            SYS_PHYSALLOC => {
                physalloc(b).map_err(|_| JustError::new("Memory operation failed"))?;
                Ok(Vec::new())
            }
            SYS_PHYSFREE => {
                physfree(b, c).map_err(|_| JustError::new("Memory operation failed"))?;
                Ok(Vec::new())
            }
            SYS_PHYSMAP => {
                physmap(b, c, d).map_err(|_| JustError::new("Memory operation failed"))?;
                Ok(Vec::new())
            }
            SYS_PHYSUNMAP => {
                physunmap(b).map_err(|_| JustError::new("Memory operation failed"))?;
                Ok(Vec::new())
            }
            SYS_VIRTTOPHYS => {
                virttophys(b).map_err(|_| JustError::new("Memory operation failed"))?;
                Ok(Vec::new())
            }
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
            _ => Err(JustError::new("Invalid system call")),
        }?;
        let current_lock = context::contexts_mut()
            .current()
            .expect("No current context")
            .clone();
        let mut current_context = current_lock.write();
        Ok(current_context
            .args
            .append_encode(&ret)
            .expect("Failed to encode syscall return value")
            .get())
    }

    let slice = validate_slice(b as *const u8, c);
    let result = if slice.is_err() {
        Err(slice.unwrap_err())
    } else {
        let sos = EncodedValues::from(slice.unwrap());
        inner(a, sos, stack)
    };

    let current_lock = context::contexts_mut()
        .current()
        .expect("No current context")
        .clone();
    let mut current_context = current_lock.write();

    if result.is_err() {
        current_context
            .args
            .append_encode(&result.unwrap_err())
            .expect("Failed to encode syscall return value")
            .get()
    } else {
        println!("Exit addr {:x}", result.as_ref().unwrap());
        result.unwrap()
    }
}
