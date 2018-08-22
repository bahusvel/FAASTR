use alloc::arc::Arc;
use alloc::boxed::Box;
use alloc::BTreeMap;
use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::sync::atomic::Ordering;
use paging;
use spin::RwLock;

use super::context::{Context, ContextId, SharedContext};
use super::load::KERNEL_MODULE;
use syscall::error::{Error, Result, EAGAIN};

/// Context list type
pub struct ContextList {
    map: BTreeMap<ContextId, SharedContext>,
    next_id: usize,
}

impl ContextList {
    /// Create a new context list.
    pub fn new() -> Self {
        ContextList {
            map: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Get the nth context.
    pub fn get(&self, id: ContextId) -> Option<&SharedContext> {
        self.map.get(&id)
    }

    /// Get the current context.
    pub fn current(&self) -> Option<&SharedContext> {
        self.map.get(&super::CONTEXT_ID.load(Ordering::SeqCst))
    }

    pub fn iter(&self) -> ::alloc::btree_map::Iter<ContextId, SharedContext> {
        self.map.iter()
    }

    /// Enqueue the context to the global list
    pub fn insert(&mut self, mut context: Context) -> Result<&SharedContext> {
        if self.next_id >= super::CONTEXT_MAX_CONTEXTS {
            self.next_id = 1;
        }

        while self.map.contains_key(&ContextId::from(self.next_id)) {
            self.next_id += 1;
        }

        if self.next_id >= super::CONTEXT_MAX_CONTEXTS {
            return Err(Error::new(EAGAIN));
        }

        let id = ContextId::from(self.next_id);
        self.next_id += 1;

        context.id = id;

        assert!(
            self.map
                .insert(id, Arc::new(RwLock::new(context)))
                .is_none()
        );

        Ok(self
            .map
            .get(&id)
            .expect("Failed to insert new context. ID is out of bounds."))
    }

    /// Spawn a context from a kernel function
    pub fn spawn(&mut self, func: extern "C" fn()) -> Result<Context> {
        let mut context = Context::new(KERNEL_MODULE.clone());
        {
            let mut fx = unsafe {
                Box::from_raw(
                    ::ALLOCATOR.alloc(Layout::from_size_align_unchecked(512, 16)) as *mut [u8; 512],
                )
            };
            for b in fx.iter_mut() {
                *b = 0;
            }
            let mut stack = vec![0; 65_536].into_boxed_slice();
            let offset = stack.len() - mem::size_of::<usize>();
            unsafe {
                let offset = stack.len() - mem::size_of::<usize>();
                let func_ptr = stack.as_mut_ptr().offset(offset as isize);
                *(func_ptr as *mut usize) = func as usize;
            }
            context
                .arch
                .set_page_table(unsafe { paging::ActivePageTable::new().address() });
            context.arch.set_fx(fx.as_ptr() as usize);
            context.arch.set_stack(stack.as_ptr() as usize + offset);
            context.kfx = Some(fx);
            context.kstack = Some(stack);
        }
        Ok(context)
    }

    pub fn remove(&mut self, id: ContextId) -> Option<SharedContext> {
        self.map.remove(&id)
    }
}
