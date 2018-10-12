//! # Context management
//!
//! For resources on contexts, please consult [wikipedia](https://en.wikipedia.org/wiki/Context_switch) and  [osdev](https://wiki.osdev.org/Context_Switching)
use self::load::KERNEL_MODULE;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::Ordering;
use gdt;
use memory::{EntryFlags, PAGE_SIZE};
use spin::{Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub use self::call::{cast_name, cast_ptr, fuse_name, fuse_ptr};
pub use self::context::{Context, ContextId, SharedContext, Status, WaitpidKey};
pub use self::list::ContextList;
pub use self::load::{
    cached_module, initfs_module, load_and_cache, FuncPtr, Module, ModuleFuncPtr, SharedModule,
    INVALID_FUNCTION,
};
pub use self::memory::ContextMemory;
pub use self::switch::{fuse_return, fuse_switch, switch};

#[path = "arch/x86_64.rs"]
mod arch;

/// Context struct
mod context;

/// Context list
mod list;

/// Context switch function
mod switch;

// Implements context instantiation and cast and fuse methods.
mod call;

// Implements loading modules.
mod load;

/// Memory struct - contains a set of pages for a context
pub mod memory;

/// Signal handling
pub mod signal;

/// Limit on number of contexts
pub const CONTEXT_MAX_CONTEXTS: usize = (isize::max_value() as usize) - 1;

/// Contexts list
static CONTEXTS: Once<RwLock<ContextList>> = Once::new();

#[thread_local]
pub static CONTEXT_ID: context::AtomicContextId = context::AtomicContextId::default();

#[thread_local]
pub static CURRENT_CONTEXT: Option<Arc<Context>> = None;

pub fn init() {
    let mut context = Context::new(KERNEL_MODULE.clone());

    let mut fx = unsafe {
        Box::from_raw(
            ::ALLOCATOR.alloc(Layout::from_size_align_unchecked(512, 16)) as *mut [u8; 512],
        )
    };
    for b in fx.iter_mut() {
        *b = 0;
    }

    context.arch.set_fx(fx.as_ptr() as usize);
    context.kfx = Some(fx);
    context.status = Status::Running;
    context.cpu_id = Some(::cpu_id());

    let (stack, address) = ContextMemory::new_kernel(
        65_536 / PAGE_SIZE,
        EntryFlags::GLOBAL | EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE,
    ).expect("Failed to allocate kernel stack");

    unsafe { gdt::set_tss_stack(address.get() as usize + stack.len_bytes()) };
    context.kstack = Some(stack);

    let inserted = contexts_mut()
        .insert(context)
        .expect("could not initialize first context")
        .clone();

    CONTEXT_ID.store(inserted.read().id, Ordering::SeqCst);
}

/// Initialize contexts, called if needed
fn init_contexts() -> RwLock<ContextList> {
    RwLock::new(ContextList::new())
}

/// Get the global schemes list, const
pub fn contexts() -> RwLockReadGuard<'static, ContextList> {
    //call once will init_contexts only once during the kernel's exececution, otherwise it will return the current context via a
    //cache.
    CONTEXTS.call_once(init_contexts).read()
}

/// Get the global schemes list, mutable
pub fn contexts_mut() -> RwLockWriteGuard<'static, ContextList> {
    CONTEXTS.call_once(init_contexts).write()
}

pub fn context_id() -> ContextId {
    CONTEXT_ID.load(Ordering::SeqCst)
}

pub fn current_context() -> SharedContext {
    CONTEXTS
        .call_once(init_contexts)
        .write()
        .current()
        .expect("No current context")
        .clone()
}
