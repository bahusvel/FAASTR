use super::arch::*;
use super::data::{SigAction, TimeSpec};
use super::error::Result;
use super::number::*;

use core::ptr;

// Signal restorer
extern "C" fn restorer() -> ! {
    sigreturn().unwrap();
    unreachable!();
}

/// Set the end of the process's heap
///
/// When `addr` is `0`, this function will return the current break.
///
/// When `addr` is nonzero, this function will attempt to set the end of the process's
/// heap to `addr` and return the new program break. The new program break should be
/// checked by the allocator, it may not be exactly `addr`, as it may be aligned to a page
/// boundary.
///
/// On error, `Err(ENOMEM)` will be returned indicating that no memory is available
pub unsafe fn brk(addr: usize) -> Result<usize> {
    syscall1(SYS_BRK, addr)
}

/// Get the current system time
pub fn clock_gettime(clock: usize, tp: &mut TimeSpec) -> Result<usize> {
    unsafe { syscall2(SYS_CLOCK_GETTIME, clock, tp as *mut TimeSpec as usize) }
}

/// Exit the current process
pub fn exit(status: usize) -> Result<usize> {
    unsafe { syscall1(SYS_EXIT, status) }
}

/// Fast userspace mutex
pub unsafe fn futex(
    addr: *mut i32,
    op: usize,
    val: i32,
    val2: usize,
    addr2: *mut i32,
) -> Result<usize> {
    syscall5(
        SYS_FUTEX,
        addr as usize,
        op,
        (val as isize) as usize,
        val2,
        addr2 as usize,
    )
}

/// Get the current process ID
pub fn getpid() -> Result<usize> {
    unsafe { syscall0(SYS_GETPID) }
}

/// Set the I/O privilege level
pub unsafe fn iopl(level: usize) -> Result<usize> {
    syscall1(SYS_IOPL, level)
}

/// Send a signal `sig` to the process identified by `pid`
pub fn kill(pid: usize, sig: usize) -> Result<usize> {
    unsafe { syscall2(SYS_KILL, pid, sig) }
}

/// Sleep for the time specified in `req`
pub fn nanosleep(req: &TimeSpec, rem: &mut TimeSpec) -> Result<usize> {
    unsafe {
        syscall2(
            SYS_NANOSLEEP,
            req as *const TimeSpec as usize,
            rem as *mut TimeSpec as usize,
        )
    }
}

/// Allocate pages, linearly in physical memory
pub unsafe fn physalloc(size: usize) -> Result<usize> {
    syscall1(SYS_PHYSALLOC, size)
}

/// Free physically allocated pages
pub unsafe fn physfree(physical_address: usize, size: usize) -> Result<usize> {
    syscall2(SYS_PHYSFREE, physical_address, size)
}

/// Map physical memory to virtual memory
pub unsafe fn physmap(physical_address: usize, size: usize, flags: usize) -> Result<usize> {
    syscall3(SYS_PHYSMAP, physical_address, size, flags)
}

/// Unmap previously mapped physical memory
pub unsafe fn physunmap(virtual_address: usize) -> Result<usize> {
    syscall1(SYS_PHYSUNMAP, virtual_address)
}

/// Set up a signal handler
// Return from signal handler
pub fn sigreturn() -> Result<usize> {
    unsafe { syscall0(SYS_SIGRETURN) }
}
pub fn sigaction(
    sig: usize,
    act: Option<&SigAction>,
    oldact: Option<&mut SigAction>,
) -> Result<usize> {
    unsafe {
        syscall4(
            SYS_SIGACTION,
            sig,
            act.map(|x| x as *const _).unwrap_or_else(ptr::null) as usize,
            oldact.map(|x| x as *mut _).unwrap_or_else(ptr::null_mut) as usize,
            restorer as usize,
        )
    }
}

/// Convert a virtual address to a physical one
pub unsafe fn virttophys(virtual_address: usize) -> Result<usize> {
    syscall1(SYS_VIRTTOPHYS, virtual_address)
}

/// Check if a child process has exited or received a signal
pub fn waitpid(pid: usize, status: &mut usize, options: usize) -> Result<usize> {
    unsafe { syscall3(SYS_WAITPID, pid, status as *mut usize as usize, options) }
}

/// Yield the process's time slice to the kernel
///
/// This function will return Ok(0) on success
pub fn sched_yield() -> Result<usize> {
    unsafe { syscall0(SYS_YIELD) }
}
