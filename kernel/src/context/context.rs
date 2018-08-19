use alloc::arc::Arc;
use alloc::boxed::Box;
use alloc::{BTreeMap, Vec, VecDeque};
use core::cmp::Ordering;
use core::mem;
use spin::Mutex;

use context::arch;
use context::memory::{Grant, Memory, SharedMemory};
use device;
use sync::WaitMap;
use syscall::data::SigAction;
use syscall::flag::SIG_DFL;

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
    /// The ID of the parent context
    pub ppid: ContextId,
    /// Status of context
    pub status: Status,
    /// CPU ID, if locked
    pub cpu_id: Option<usize>,
    /// Context is halting parent
    pub vfork: bool,
    /// Context is being waited on
    pub waitpid: Arc<WaitMap<WaitpidKey, (ContextId, usize)>>,
    /// Context should handle pending signals
    pub pending: VecDeque<u8>,
    /// Context should wake up at specified time
    pub wake: Option<(u64, u64)>,
    /// The architecture specific context
    pub arch: arch::Context,
    /// Kernel FX - used to store SIMD and FPU registers on context switch
    pub kfx: Option<Box<[u8]>>,
    /// Kernel stack
    pub kstack: Option<Box<[u8]>>,
    /// Kernel signal backup
    pub ksig: Option<(arch::Context, Option<Box<[u8]>>, Option<Box<[u8]>>)>,
    /// Restore ksig context on next switch
    pub ksig_restore: bool,
    /// Executable image
    pub image: Vec<SharedMemory>,
    /// User heap
    pub heap: Option<SharedMemory>,
    /// User stack
    pub stack: Option<Memory>,
    /// User signal stack
    pub sigstack: Option<Memory>,
    /// User grants
    pub grants: Vec<Grant>,
    /// The name of the context
    pub name: Box<[u8]>,
    /// The process environment
    pub env: BTreeMap<Box<[u8]>, Arc<Mutex<Vec<u8>>>>,
    /// Singal actions
    pub actions: Arc<Mutex<Vec<(SigAction, usize)>>>,
}

impl Context {
    pub fn new(id: ContextId) -> Context {
        Context {
            id: id,
            ppid: ContextId::from(0),
            status: Status::Blocked,
            cpu_id: None,
            vfork: false,
            waitpid: Arc::new(WaitMap::new()),
            pending: VecDeque::new(),
            wake: None,
            arch: arch::Context::new(),
            kfx: None,
            kstack: None,
            ksig: None,
            ksig_restore: false,
            image: Vec::new(),
            heap: None,
            stack: None,
            sigstack: None,
            grants: Vec::new(),
            name: Vec::new().into_boxed_slice(),
            env: BTreeMap::new(),
            actions: Arc::new(Mutex::new(vec![
                (
                    SigAction {
                        sa_handler: unsafe { mem::transmute(SIG_DFL) },
                        sa_mask: [0; 2],
                        sa_flags: 0,
                    },
                    0
                );
                128
            ])),
        }
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
