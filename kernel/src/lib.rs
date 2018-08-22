//! # The Redox OS Kernel, version 2
//!
//! The Redox OS Kernel is a microkernel that supports `x86_64` systems and
//! provides Unix-like syscalls for primarily Rust applications
// FIXME this is wrong, and is not a solution, I need to implement Debug properly on affected structs
#![allow(safe_packed_borrows)]
//#![deny(warnings)]
#![cfg_attr(feature = "clippy", allow(if_same_then_else))]
#![cfg_attr(feature = "clippy", allow(inline_always))]
#![cfg_attr(feature = "clippy", allow(many_single_char_names))]
#![cfg_attr(feature = "clippy", allow(module_inception))]
#![cfg_attr(feature = "clippy", allow(new_without_default))]
#![cfg_attr(feature = "clippy", allow(not_unsafe_ptr_arg_deref))]
#![cfg_attr(feature = "clippy", allow(or_fun_call))]
#![cfg_attr(feature = "clippy", allow(too_many_arguments))]
#![feature(alloc)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(collections)]
#![feature(concat_idents)]
#![feature(const_atomic_usize_new)]
#![feature(const_fn)]
#![feature(const_max_value)]
#![feature(const_size_of)]
#![feature(core_intrinsics)]
#![feature(integer_atomics)]
#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(never_type)]
#![feature(panic_implementation)]
#![feature(ptr_internals)]
#![feature(thread_local)]
#![feature(unique)]
#![no_std]

pub extern crate x86;

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate bitflags;
extern crate goblin;
extern crate linked_list_allocator;
#[cfg(feature = "slab")]
extern crate slab_allocator;
extern crate spin;

use core::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

pub use consts::*;

#[macro_use]
/// Shared data structures
pub mod common;

/// Architecture-dependent stuff
#[macro_use]
pub mod arch;
pub use arch::*;

/// Constants like memory locations
pub mod consts;

/// Heap allocators
pub mod allocator;

/// ACPI table parsing
#[cfg(feature = "acpi")]
mod acpi;

/// Context management
pub mod context;

/// Architecture-independent devices
pub mod devices;

/// ELF file parsing
#[cfg(not(feature = "doc"))]
pub mod elf;

/// External functions
pub mod externs;

/// Memory management
pub mod memory;

/// Panic
#[cfg(not(any(feature = "doc", test)))]
pub mod panic;

/// Synchronization primitives
pub mod sync;

/// Syscall handlers
pub mod syscall;

/// Time
pub mod time;

/// Tests
#[cfg(test)]
pub mod tests;

#[global_allocator]
static ALLOCATOR: allocator::Allocator = allocator::Allocator;

/// A unique number that identifies the current CPU - used for scheduling
#[thread_local]
static CPU_ID: AtomicUsize = ATOMIC_USIZE_INIT;

/// Get the current CPU's scheduling ID
#[inline(always)]
pub fn cpu_id() -> usize {
    CPU_ID.load(Ordering::Relaxed)
}

/// The count of all CPUs that can have work scheduled
static CPU_COUNT: AtomicUsize = ATOMIC_USIZE_INIT;

/// Get the number of CPUs currently active
#[inline(always)]
pub fn cpu_count() -> usize {
    CPU_COUNT.load(Ordering::Relaxed)
}

include!(concat!(env!("OUT_DIR"), "/gen.rs"));

/// This is the kernel entry point for the primary CPU. The arch crate is responsible for calling this
pub fn kmain(cpus: usize, env: &[u8]) -> ! {
    CPU_ID.store(0, Ordering::SeqCst);
    CPU_COUNT.store(cpus, Ordering::SeqCst);

    //Initialize the first context, stored in kernel/src/context/mod.rs
    context::init();

    let pid = syscall::getpid();
    println!("BSP: {:?} {}", pid, cpus);
    println!("Env: {:?}", ::core::str::from_utf8(env));

    println!("Denis was here");

    /*
    match context::contexts_mut().spawn(userspace_init) {
        Ok(context_lock) => {
            let mut context = context_lock.write();
            context.status = context::Status::Runnable;
        }
        Err(err) => {
            panic!("failed to spawn userspace_init: {:?}", err);
        }
    }
    */

    let module = context::load_and_cache(
        "exit",
        initfs_get_file(b"/exit").expect("Could not find exit in initfs"),
    ).expect("Failed to load module");

    println!("Loaded");

    context::cast(context::cached_module("exit").unwrap(), 4162).expect("Failed to call");
    context::cast(context::cached_module("exit").unwrap(), 4162).expect("Failed to call");
    context::cast(context::cached_module("exit").unwrap(), 4162).expect("Failed to call");

    /*
    context::fuse(module.clone(), 4162).expect("Failed to call");
    println!("Exited to caller");
    context::fuse(module, 4162).expect("Failed to call");
    println!("Exited to caller");

    loop {
        unsafe {
            interrupt::disable();
            interrupt::halt();
        }
    }
    */

    loop {
        unsafe {
            interrupt::disable();
            if context::switch() {
                interrupt::enable_and_nop();
            } else {
                // Enable interrupts, then halt CPU (to save power) until the next interrupt is actually fired.
                interrupt::enable_and_halt();
            }
        }
    }
}

/// This is the main kernel entry point for secondary CPUs
#[allow(unreachable_code, unused_variables)]
pub fn kmain_ap(id: usize) -> ! {
    CPU_ID.store(id, Ordering::SeqCst);

    if cfg!(feature = "multi_core") {
        context::init();

        let pid = syscall::getpid();
        println!("AP {}: {:?}", id, pid);

        loop {
            unsafe {
                interrupt::disable();
                if context::switch() {
                    interrupt::enable_and_nop();
                } else {
                    // Enable interrupts, then halt CPU (to save power) until the next interrupt is actually fired.
                    interrupt::enable_and_halt();
                }
            }
        }
    } else {
        println!("AP {}: Disabled", id);

        loop {
            unsafe {
                interrupt::disable();
                interrupt::halt();
            }
        }
    }
}

/// Allow exception handlers to send signal to arch-independant kernel
#[no_mangle]
pub extern "C" fn ksignal(signal: usize) {
    println!(
        "SIGNAL {}, CPU {}, PID {:?}",
        signal,
        cpu_id(),
        context::context_id()
    );
    {
        let contexts = context::contexts();
        if let Some(context_lock) = contexts.current() {
            let context = context_lock.read();
            println!("NAME {}", context.name);
        }
    }
    syscall::exit(signal & 0x7F);
}
