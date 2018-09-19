use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cmp::Ordering;
use hashmap_core::fnv::FnvHashMap;
use spin::{Mutex, RwLock};

use super::{ModuleFuncPtr, SharedModule, INVALID_FUNCTION};
use context::arch;
use context::memory::{ContextMemory, ContextValues, Grant};
use device;
use sync::WaitMap;

pub type SharedContext = Arc<RwLock<Context>>;

/// Unique identifier for a context (i.e. `pid`).
use core::sync::atomic::AtomicUsize;
int_like!(ContextId, AtomicContextId, usize, AtomicUsize);

/// The status of a context - used for scheduling
/// See `syscall::process::waitpid` and the `sync` module for examples of usage
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Running,
    Runnable,
    Blocked,
    New,
    Stopped(usize),
    Exited(usize),
}

#[derive(Copy, Clone, Debug)]
pub struct WaitpidKey {
    pub pid: Option<ContextId>,
    pub pgid: Option<ContextId>,
}

impl Ord for WaitpidKey {
    fn cmp(&self, other: &WaitpidKey) -> Ordering {
        // If both have pid set, compare that
        if let Some(s_pid) = self.pid {
            if let Some(o_pid) = other.pid {
                return s_pid.cmp(&o_pid);
            }
        }

        // If both have pgid set, compare that
        if let Some(s_pgid) = self.pgid {
            if let Some(o_pgid) = other.pgid {
                return s_pgid.cmp(&o_pgid);
            }
        }

        // If either has pid set, it is greater
        if self.pid.is_some() {
            return Ordering::Greater;
        }

        if other.pid.is_some() {
            return Ordering::Less;
        }

        // If either has pgid set, it is greater
        if self.pgid.is_some() {
            return Ordering::Greater;
        }

        if other.pgid.is_some() {
            return Ordering::Less;
        }

        // If all pid and pgid are None, they are equal
        Ordering::Equal
    }
}

impl PartialOrd for WaitpidKey {
    fn partial_cmp(&self, other: &WaitpidKey) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for WaitpidKey {
    fn eq(&self, other: &WaitpidKey) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for WaitpidKey {}

/// A context, which identifies either a process or a thread
#[derive(Debug)]
pub struct Context {
    /// The ID of this context
    pub id: ContextId,
    /// Status of context
    pub status: Status,
    /// Link to a blocked, owned context to return to
    pub ret_link: Option<SharedContext>,
    /// CPU ID, if locked
    pub cpu_id: Option<usize>,
    /// Context is being waited on
    pub waitpid: Arc<WaitMap<WaitpidKey, (ContextId, usize)>>,
    /// Context should wake up at specified time
    pub wake: Option<(u64, u64)>,
    /// The architecture specific context
    pub arch: arch::Context,
    /// Kernel FX - used to store SIMD and FPU registers on context switch
    pub kfx: Option<Box<[u8]>>,
    /// Kernel stack
    pub kstack: Option<ContextMemory>,
    // Copy of executable image mappings
    pub image: Vec<ContextMemory>,
    // Memory area where kernel places arguments to userspace
    pub args: ContextValues,
    /// User heap
    pub heap: Option<ContextMemory>,
    /// User stack
    pub stack: Option<ContextMemory>,
    /// User grants
    pub grants: Vec<Grant>,
    /// Pointer to the function
    pub function: ModuleFuncPtr,
    /// The name of the function
    pub name: Option<String>,
    /// The process environment
    pub env: FnvHashMap<Box<[u8]>, Arc<Mutex<Vec<u8>>>>,
    /// Module this function was spawned ALLOCATOR
    pub module: SharedModule,
}

impl Context {
    pub fn new(module: SharedModule) -> Context {
        Context {
            id: ContextId::from(0),
            status: Status::Blocked,
            ret_link: None,
            cpu_id: None,
            waitpid: Arc::new(WaitMap::new()),
            wake: None,
            arch: arch::Context::new(),
            kfx: None,
            kstack: None,
            image: Vec::new(),
            args: ContextValues::new_no_memory(),
            heap: None,
            stack: None,
            grants: Vec::new(),
            function: INVALID_FUNCTION,
            name: None,
            env: FnvHashMap::new(),
            module: module,
        }
    }

    pub fn name(&self) -> String {
        format!(
            "{}::{}(0x{:x})",
            self.module.name(),
            self.name.as_ref().unwrap_or(&String::from("")),
            self.function
        )
    }

    /// Block the context, and return true if it was runnable before being blocked
    pub fn block(&mut self) -> bool {
        if self.status == Status::Runnable {
            self.status = Status::Blocked;
            true
        } else {
            false
        }
    }

    /// Unblock context, and return true if it was blocked before being marked runnable
    pub fn unblock(&mut self) -> bool {
        if self.status == Status::Blocked {
            self.status = Status::Runnable;
            if cfg!(feature = "multi_core") {
                if let Some(cpu_id) = self.cpu_id {
                    if cpu_id != ::cpu_id() {
                        // Send IPI if not on current CPU
                        // TODO: Make this more architecture independent
                        unsafe { device::local_apic::LOCAL_APIC.set_icr(3 << 18 | 1 << 14 | 0x40) };
                    }
                }
            }
            true
        } else {
            false
        }
    }
}
