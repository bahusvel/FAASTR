//! # Memory management
//! Some code was borrowed from [Phil Opp's Blog](http://os.phil-opp.com/allocating-frames.html)

pub use paging::{Page, PhysicalAddress, VirtualAddress, PAGE_SIZE};

use self::bump::BumpAllocator;
use self::recycle::RecycleAllocator;
use self::valloc::Valloc;
use core::{mem, slice};
pub use paging::entry::EntryFlags;
use paging::{ActivePageTable, PageIter};

use spin::Mutex;

pub mod bump;
pub mod recycle;
mod valloc;

/// The current memory map. It's size is maxed out to 512 entries, due to it being
/// from 0x500 to 0x5000 (800 is the absolute total)
static mut MEMORY_MAP: [MemoryArea; 512] = [MemoryArea {
    base_addr: 0,
    length: 0,
    _type: 0,
    acpi: 0,
}; 512];

/// Memory does not exist
pub const MEMORY_AREA_NULL: u32 = 0;

/// Memory is free to use
pub const MEMORY_AREA_FREE: u32 = 1;

/// Memory is reserved
pub const MEMORY_AREA_RESERVED: u32 = 2;

/// Memory is used by ACPI, and can be reclaimed
pub const MEMORY_AREA_ACPI: u32 = 3;

/// A memory map area
#[derive(Copy, Clone, Debug, Default)]
#[repr(packed)]
pub struct MemoryArea {
    pub base_addr: u64,
    pub length: u64,
    pub _type: u32,
    pub acpi: u32,
}

#[derive(Clone)]
pub struct MemoryAreaIter {
    _type: u32,
    i: usize,
}

impl MemoryAreaIter {
    fn new(_type: u32) -> Self {
        MemoryAreaIter { _type: _type, i: 0 }
    }
}

impl Iterator for MemoryAreaIter {
    type Item = &'static MemoryArea;
    fn next(&mut self) -> Option<Self::Item> {
        while self.i < unsafe { MEMORY_MAP.len() } {
            let entry = unsafe { &MEMORY_MAP[self.i] };
            self.i += 1;
            if entry._type == self._type {
                return Some(entry);
            }
        }
        None
    }
}

static ALLOCATOR: Mutex<Option<RecycleAllocator<BumpAllocator>>> = Mutex::new(None);
static VALLOC: Mutex<Option<Valloc>> = Mutex::new(None);

/// Init memory module
/// Must be called once, and only once,
pub unsafe fn init(kernel_start: usize, kernel_end: usize) {
    // Copy memory map from bootloader location
    for (i, entry) in MEMORY_MAP.iter_mut().enumerate() {
        *entry = *(0x500 as *const MemoryArea).offset(i as isize);
        if entry._type != MEMORY_AREA_NULL {
            println!("{:?}", entry);
        }
    }

    *ALLOCATOR.lock() = Some(RecycleAllocator::new(BumpAllocator::new(
        kernel_start,
        kernel_end,
        MemoryAreaIter::new(MEMORY_AREA_FREE),
    )));
}

/// Init memory module after core
/// Must be called once, and only once,
pub unsafe fn init_noncore() {
    ALLOCATOR
        .lock()
        .as_mut()
        .expect("frame allocator not initialized")
        .set_noncore(true);
    *VALLOC.lock() = Some(Valloc::new(::KERNEL_VALLOC_OFFSET, ::KERNEL_VALLOC_SIZE));
    let valloc_page = allocate_unmapped_pages(1).expect("Out of virtual addres space");
    // This will map in the VALLOC_PML4 and keep it mapped forever.
    let mut active_table = ActivePageTable::new();
    active_table
        .map_to(
            valloc_page.start,
            Frame::containing_address(PhysicalAddress::new(0)),
            EntryFlags::GLOBAL | EntryFlags::NO_EXECUTE,
        ).flush(&mut active_table);
    mem::forget(valloc_page);
}

/// Get the number of frames available
pub fn free_frames() -> usize {
    if let Some(ref allocator) = *ALLOCATOR.lock() {
        allocator.free_frames()
    } else {
        panic!("frame allocator not initialized");
    }
}

/// Get the number of frames used
pub fn used_frames() -> usize {
    if let Some(ref allocator) = *ALLOCATOR.lock() {
        allocator.used_frames()
    } else {
        panic!("frame allocator not initialized");
    }
}

/// Allocate a range of frames
pub fn allocate_frames(count: usize) -> Option<Frame> {
    if let Some(ref mut allocator) = *ALLOCATOR.lock() {
        allocator.allocate_frames(count)
    } else {
        panic!("frame allocator not initialized");
    }
}

/// Deallocate a range of frames frame
pub fn deallocate_frames(frame: Frame, count: usize) {
    if let Some(ref mut allocator) = *ALLOCATOR.lock() {
        allocator.deallocate_frames(frame, count)
    } else {
        panic!("frame allocator not initialized");
    }
}

#[derive(Debug)]
pub struct VallocPages {
    start: Page,
    count: usize,
}

impl Drop for VallocPages {
    fn drop(&mut self) {
        deallocate_pages(self.start, self.count);
    }
}

impl VallocPages {
    pub fn iter(&self) -> PageIter {
        Page::range_inclusive(
            self.start,
            Page::containing_address(VirtualAddress::new(
                self.start.start_address().get() + self.count * PAGE_SIZE - 1,
            )),
        )
    }

    pub fn start_address(&self) -> VirtualAddress {
        self.start.start_address()
    }

    pub unsafe fn to_slice(&self) -> &[u8] {
        let start = self.start.start_address().get();
        let size = self.count * PAGE_SIZE;
        slice::from_raw_parts(start as *const u8, size)
    }

    pub unsafe fn to_slice_mut(&mut self) -> &mut [u8] {
        let start = self.start.start_address().get();
        let size = self.count * PAGE_SIZE;
        slice::from_raw_parts_mut(start as *mut u8, size)
    }
}

pub fn allocate_unmapped_pages(count: usize) -> Option<VallocPages> {
    let start = VALLOC
        .lock()
        .as_mut()
        .expect("valloc not initialiazed")
        .allocate_pages(count)?;
    Some(VallocPages { start, count })
}

pub fn deallocate_pages(page: Page, count: usize) {
    VALLOC
        .lock()
        .as_mut()
        .expect("valloc not initialiazed")
        .deallocate_pages(page, count);
}

/// A frame, allocated by the frame allocator.
/// Do not add more derives, or make anything `pub`!
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frame {
    number: usize,
}

impl Frame {
    /// Get the address of this frame
    pub fn start_address(&self) -> PhysicalAddress {
        PhysicalAddress::new(self.number * PAGE_SIZE)
    }

    //TODO: Set private
    pub fn clone(&self) -> Frame {
        Frame {
            number: self.number,
        }
    }

    /// Create a frame containing `address`
    pub fn containing_address(address: PhysicalAddress) -> Frame {
        Frame {
            number: address.get() / PAGE_SIZE,
        }
    }

    //TODO: Set private
    pub fn range_inclusive(start: Frame, end: Frame) -> FrameIter {
        FrameIter {
            start: start,
            end: end,
        }
    }
}

pub struct FrameIter {
    start: Frame,
    end: Frame,
}

impl Iterator for FrameIter {
    type Item = Frame;

    fn next(&mut self) -> Option<Frame> {
        if self.start <= self.end {
            let frame = self.start.clone();
            self.start.number += 1;
            Some(frame)
        } else {
            None
        }
    }
}

pub trait FrameAllocator {
    fn set_noncore(&mut self, noncore: bool);
    fn free_frames(&self) -> usize;
    fn used_frames(&self) -> usize;
    fn allocate_frames(&mut self, size: usize) -> Option<Frame>;
    fn deallocate_frames(&mut self, frame: Frame, size: usize);
}
