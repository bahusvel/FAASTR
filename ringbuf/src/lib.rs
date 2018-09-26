#![feature(allocator_api)]
#![feature(alloc)]
#![no_std]

extern crate alloc;

#[cfg(feature = "alloc")]
use core::alloc::Layout;
use core::cell::Cell;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::usize;

const CACHELINE_LEN: usize = 64;
const SIZEOF_HEADER: usize = mem::size_of::<Header>();

macro_rules! cacheline_pad {
    ($N:expr) => {
        CACHELINE_LEN / core::mem::size_of::<usize>() - $N
    };
}

#[repr(C)]
pub struct Header {
    capacity: usize,
    allocated_size: usize,
    _padding1: [usize; cacheline_pad!(2)],

    // Consumer cache line
    head: AtomicUsize,
    shadow_tail: Cell<usize>,
    _padding2: [usize; cacheline_pad!(2)],

    // Producer cache line
    tail: AtomicUsize,
    shadow_head: Cell<usize>,
    _padding3: [usize; cacheline_pad!(2)],
}

unsafe impl Sync for Header {}

fn prev_power_of_two(mut x: usize) -> usize {
    x = x | (x >> 1);
    x = x | (x >> 2);
    x = x | (x >> 4);
    x = x | (x >> 8);
    x = x | (x >> 16);
    x - (x >> 1)
}

impl Header {
    pub unsafe fn new_inline_at(buff: &mut [u8]) -> &Self {
        assert!(buff.len() > SIZEOF_HEADER);
        let buff_ptr = buff.as_ptr() as *mut Header;
        let capacity = prev_power_of_two(buff.len() - SIZEOF_HEADER);

        *buff_ptr = Header {
            capacity,
            allocated_size: capacity,
            _padding1: [0; cacheline_pad!(2)],

            head: AtomicUsize::new(0),
            shadow_tail: Cell::new(0),
            _padding2: [0; cacheline_pad!(2)],

            tail: AtomicUsize::new(0),
            shadow_head: Cell::new(0),
            _padding3: [0; cacheline_pad!(2)],
        };

        &*buff_ptr
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn size(&self) -> usize {
        self.tail.load(Ordering::Acquire) - self.head.load(Ordering::Acquire)
    }

    pub fn free_space(&self) -> usize {
        self.capacity() - self.size()
    }
}

#[cfg(feature = "alloc")]
impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        // Pop the rest of the values off the queue.  By moving them into this scope,
        // we implicitly call their destructor

        // TODO this could be optimized to avoid the atomic operations / book-keeping...but
        // since this is the destructor, there shouldn't be any contention... so meh?
        while let Some(_) = self.try_pop() {}

        unsafe {
            let layout = Layout::from_size_align(
                self.allocated_size * mem::size_of::<T>(),
                mem::align_of::<T>(),
            ).unwrap();
            alloc::dealloc(self.buffer as *mut u8, layout);
        }
    }
}

#[cfg(feature = "alloc")]
pub fn make(capacity: usize) -> (Producer, Consumer) {
    let ptr = unsafe { allocate_buffer(capacity) };

    let arc = Arc::new(Buffer {
        buffer: ptr,
        capacity,
        allocated_size: capacity.next_power_of_two(),
        _padding1: [0; cacheline_pad!(2)],

        head: AtomicUsize::new(0),
        shadow_tail: Cell::new(0),
        _padding2: [0; cacheline_pad!(2)],

        tail: AtomicUsize::new(0),
        shadow_head: Cell::new(0),
        _padding3: [0; cacheline_pad!(2)],
    });

    (
        Producer {
            buffer: arc.clone(),
        },
        Consumer {
            buffer: arc.clone(),
        },
    )
}

#[cfg(feature = "alloc")]
unsafe fn allocate_buffer<T>(capacity: usize) -> *mut T {
    let adjusted_size = capacity.next_power_of_two();
    let size = adjusted_size
        .checked_mul(mem::size_of::<T>())
        .expect("capacity overflow");

    let layout = Layout::from_size_align(size, mem::align_of::<T>()).unwrap();
    let ptr = alloc::alloc(layout);
    if ptr.is_null() {
        alloc::handle_alloc_error(layout)
    } else {
        ptr as *mut T
    }
}

pub struct Producer<'a>(&'a Header, &'a mut [u8]);
unsafe impl<'a> Send for Producer<'a> {}

impl<'b, 'a: 'b> Producer<'a> {
    pub unsafe fn from_slice(buff: &'a mut [u8]) -> Self {
        let capacity = prev_power_of_two(buff.len() - SIZEOF_HEADER);
        let buffer = &*(buff.as_ptr() as *const Header);
        assert!(capacity == buffer.capacity);
        Producer(buffer, &mut buff[SIZEOF_HEADER..SIZEOF_HEADER + capacity])
    }

    pub fn try_write(&'b mut self, n: usize) -> Option<WriteHandle<'b, 'a>> {
        let current_tail = self.0.tail.load(Ordering::Relaxed);

        if self.0.shadow_head.get() + self.0.capacity <= current_tail + n {
            self.0.shadow_head.set(self.0.head.load(Ordering::Relaxed));
            if self.0.shadow_head.get() + self.0.capacity <= current_tail + n {
                return None;
            }
        }

        Some(WriteHandle {
            buffer: self,
            n,
            current_tail,
        })
    }

    pub fn write(&'b mut self, n: usize) -> WriteHandle<'b, 'a> {
        let current_tail = self.0.tail.load(Ordering::Relaxed);
        while self.0.shadow_head.get() + self.0.capacity <= current_tail + n {
            self.0.shadow_head.set(self.0.head.load(Ordering::Relaxed));
        }

        WriteHandle {
            buffer: self,
            n,
            current_tail,
        }
    }
}

impl<'a> Deref for Producer<'a> {
    type Target = Header;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct WriteHandle<'a, 'b: 'a> {
    buffer: &'a mut Producer<'b>,
    current_tail: usize,
    n: usize,
}

impl<'a, 'b> Deref for WriteHandle<'a, 'b> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        let offset = self.current_tail & (self.buffer.0.allocated_size - 1);
        &self.buffer.1[offset..offset + self.n]
    }
}

impl<'a, 'b> DerefMut for WriteHandle<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let offset = self.current_tail & (self.buffer.0.allocated_size - 1);
        &mut self.buffer.1[offset..offset + self.n]
    }
}

impl<'a, 'b> Drop for WriteHandle<'a, 'b> {
    fn drop(&mut self) {
        self.buffer
            .0
            .tail
            .store(self.current_tail.wrapping_add(self.n), Ordering::Release);
    }
}

pub struct Consumer<'a>(&'a Header, &'a [u8]);
unsafe impl<'a> Send for Consumer<'a> {}

impl<'b, 'a: 'b> Consumer<'a> {
    pub unsafe fn from_slice(buff: &'a mut [u8]) -> Self {
        let capacity = prev_power_of_two(buff.len() - SIZEOF_HEADER);
        let buffer = &*(buff.as_ptr() as *const Header);
        assert!(capacity == buffer.capacity);
        Consumer(buffer, &buff[SIZEOF_HEADER..SIZEOF_HEADER + capacity])
    }

    pub fn try_read(&'b mut self, n: usize) -> Option<ReadHandle<'b, 'a>> {
        let current_head = self.0.head.load(Ordering::Relaxed);

        if self.0.shadow_tail.get().wrapping_sub(current_head) <= n {
            self.0.shadow_tail.set(self.0.tail.load(Ordering::Acquire));
            if self.0.shadow_tail.get().wrapping_sub(current_head) <= n {
                return None;
            }
        }

        Some(ReadHandle {
            buffer: self,
            current_head,
            n,
        })
    }

    pub fn skip_n(&mut self, n: usize) -> usize {
        let current_head = self.0.head.load(Ordering::Relaxed);

        self.0.shadow_tail.set(self.0.tail.load(Ordering::Acquire));
        if current_head == self.0.shadow_tail.get() {
            return 0;
        }
        let mut diff = self.0.shadow_tail.get().wrapping_sub(current_head);
        if diff > n {
            diff = n
        }
        self.0
            .head
            .store(current_head.wrapping_add(diff), Ordering::Release);
        diff
    }

    pub fn read(&'b mut self, n: usize) -> ReadHandle<'b, 'a> {
        let current_head = self.0.head.load(Ordering::Relaxed);

        while self.0.shadow_tail.get().wrapping_sub(current_head) <= n {
            self.0.shadow_tail.set(self.0.tail.load(Ordering::Acquire));
        }

        ReadHandle {
            buffer: self,
            current_head,
            n,
        }
    }
}

impl<'a> Deref for Consumer<'a> {
    type Target = Header;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct ReadHandle<'a, 'b: 'a> {
    buffer: &'a mut Consumer<'b>,
    current_head: usize,
    n: usize,
}

impl<'a, 'b> Deref for ReadHandle<'a, 'b> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        let offset = self.current_head & (self.buffer.0.allocated_size - 1);
        &self.buffer.1[offset..offset + self.n]
    }
}

impl<'a, 'b> Drop for ReadHandle<'a, 'b> {
    fn drop(&mut self) {
        self.buffer
            .0
            .head
            .store(self.current_head.wrapping_add(self.n), Ordering::Release);
    }
}
