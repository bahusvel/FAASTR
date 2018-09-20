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
#![feature(concat_idents)]
#![feature(const_fn)]
#![feature(core_intrinsics)]
#![feature(integer_atomics)]
#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(never_type)]
#![feature(panic_implementation)]
#![feature(ptr_internals)]
#![feature(thread_local)]
#![feature(tool_attributes)]
#![feature(try_from)]
#![feature(slice_patterns)]
#![no_std]

pub extern crate x86;

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate serde_derive;
extern crate goblin;
extern crate hashmap_core;
extern crate linked_list_allocator;
extern crate serde_json_core;
#[cfg(feature = "slab")]
extern crate slab_allocator;
#[macro_use]
extern crate sos;
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

pub mod error;
/// Time
pub mod time;

/// Tests

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

/// This is the kernel entry point for the primary CPU. The arch crate is responsible for calling this
pub fn kmain(cpus: usize, env: &[u8]) -> ! {
    CPU_ID.store(0, Ordering::SeqCst);
    CPU_COUNT.store(cpus, Ordering::SeqCst);

    //Initialize the first context, stored in kernel/src/context/mod.rs
    context::init();

    let pid = syscall::getpid();
    println!("BSP: {:?} {}", pid, cpus);
    println!("Env: {:?}", ::core::str::from_utf8(env));

    let module = context::initfs_module("call").expect("Failed to load module");

    println!("Loaded");

    context::cast_name(module.clone(), "passthrough", &sos!("hello")).expect("Failed to call");

    context::fuse_name(module.clone(), "call", &sos!()).expect("Failed to call");
    println!("Exited to caller");

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
            println!("NAME {}", context.name());
        }
    }
    syscall::exit(signal & 0x7F);
}
