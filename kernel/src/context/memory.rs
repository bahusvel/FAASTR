use alloc::collections::vec_deque::VecDeque;
use alloc::sync::Arc;
use core::ops::{Deref, DerefMut};
use core::{intrinsics, slice};

use memory::{
    allocate_frames, allocate_unmapped_pages, deallocate_frames, Frame, FrameIter, VallocPages,
};
use paging::entry::EntryFlags;
use paging::mapper::{Mapper, MapperFlushAll};
use paging::temporary_page::TemporaryPage;
use paging::{ActivePageTable, InactivePageTable, Page, PhysicalAddress};
use sos::SOS;

pub use paging::{VirtualAddress, PAGE_SIZE};

#[derive(Debug)]
pub struct Grant {
    start: VirtualAddress,
    size: usize,
    flags: EntryFlags,
    mapped: bool,
}

impl Grant {
    pub fn physmap(
        from: PhysicalAddress,
        to: VirtualAddress,
        size: usize,
        flags: EntryFlags,
    ) -> Grant {
        let mut active_table = unsafe { ActivePageTable::new() };

        let mut flush_all = MapperFlushAll::new();

        let start_page = Page::containing_address(to);
        let end_page = Page::containing_address(VirtualAddress::new(to.get() + size - 1));
        for page in Page::range_inclusive(start_page, end_page) {
            let frame = Frame::containing_address(PhysicalAddress::new(
                page.start_address().get() - to.get() + from.get(),
            ));
            let result = active_table.map_to(page, frame, flags);
            flush_all.consume(result);
        }

        flush_all.flush(&mut active_table);

        Grant {
            start: to,
            size: size,
            flags: flags,
            mapped: true,
        }
    }

    pub fn map_inactive(
        from: VirtualAddress,
        to: VirtualAddress,
        size: usize,
        flags: EntryFlags,
        new_table: &mut InactivePageTable,
        temporary_page: &mut TemporaryPage,
    ) -> Grant {
        let mut active_table = unsafe { ActivePageTable::new() };

        let mut frames = VecDeque::new();

        let start_page = Page::containing_address(from);
        let end_page = Page::containing_address(VirtualAddress::new(from.get() + size - 1));
        for page in Page::range_inclusive(start_page, end_page) {
            let frame = active_table
                .translate_page(page)
                .expect("grant references unmapped memory");
            frames.push_back(frame);
        }

        active_table.with(new_table, temporary_page, |mapper| {
            let start_page = Page::containing_address(to);
            let end_page = Page::containing_address(VirtualAddress::new(to.get() + size - 1));
            for page in Page::range_inclusive(start_page, end_page) {
                let frame = frames
                    .pop_front()
                    .expect("grant did not find enough frames");
                let result = mapper.map_to(page, frame, flags);
                // Ignore result due to mapping on inactive table
                unsafe {
                    result.ignore();
                }
            }
        });

        Grant {
            start: to,
            size: size,
            flags: flags,
            mapped: true,
        }
    }

    pub fn start_address(&self) -> VirtualAddress {
        self.start
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn flags(&self) -> EntryFlags {
        self.flags
    }

    pub fn unmap(mut self) {
        assert!(self.mapped);

        let mut active_table = unsafe { ActivePageTable::new() };

        let mut flush_all = MapperFlushAll::new();

        let start_page = Page::containing_address(self.start);
        let end_page =
            Page::containing_address(VirtualAddress::new(self.start.get() + self.size - 1));
        for page in Page::range_inclusive(start_page, end_page) {
            let (result, _frame) = active_table.unmap_return(page, false);
            flush_all.consume(result);
        }

        flush_all.flush(&mut active_table);

        self.mapped = false;
    }

    pub fn unmap_inactive(
        mut self,
        new_table: &mut InactivePageTable,
        temporary_page: &mut TemporaryPage,
    ) {
        assert!(self.mapped);

        let mut active_table = unsafe { ActivePageTable::new() };

        active_table.with(new_table, temporary_page, |mapper| {
            let start_page = Page::containing_address(self.start);
            let end_page =
                Page::containing_address(VirtualAddress::new(self.start.get() + self.size - 1));
            for page in Page::range_inclusive(start_page, end_page) {
                let (result, _frame) = mapper.unmap_return(page, false);
                // This is not the active table, so the flush can be ignored
                unsafe {
                    result.ignore();
                }
            }
        });

        self.mapped = false;
    }
}

impl Drop for Grant {
    fn drop(&mut self) {
        assert!(!self.mapped);
    }
}

#[derive(Debug)]
struct VallocMapping {
    pub pages: VallocPages,
    flags: EntryFlags,
    frames: Arc<Frames>,
}

impl VallocMapping {
    pub fn new(flags: EntryFlags, frames: Arc<Frames>) -> Option<Self> {
        let pages = allocate_unmapped_pages(frames.count)?;

        let mut active_table = unsafe { ActivePageTable::new() };
        let mut flush_all = MapperFlushAll::new();

        for (page, frame) in pages.iter().zip(frames.iter()) {
            let result = active_table.map_to(page, frame, flags);
            flush_all.consume(result);
        }

        flush_all.flush(&mut active_table);

        Some(VallocMapping {
            pages,
            flags,
            frames,
        })
    }
}

impl Drop for VallocMapping {
    fn drop(&mut self) {
        let mut active_table = unsafe { ActivePageTable::new() };
        let mut flush_all = MapperFlushAll::new();

        for page in self.pages.iter() {
            let (result, _) = active_table.unmap_return(page, false);
            flush_all.consume(result);
        }
        flush_all.flush(&mut active_table);
    }
}

impl Deref for VallocMapping {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let addr = self.pages.start_address().get() as *const u8;
        unsafe { slice::from_raw_parts(addr, self.frames.count * PAGE_SIZE) }
    }
}

impl DerefMut for VallocMapping {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if !self.flags.contains(EntryFlags::WRITABLE) {
            panic!("Attemped to mutable dereference non-writable mapping");
        }
        let addr = self.pages.start_address().get() as *mut u8;
        unsafe { slice::from_raw_parts_mut(addr, self.frames.count * PAGE_SIZE) }
    }
}

#[derive(Debug)]
pub struct Frames {
    start: Frame,
    count: usize,
}

impl Frames {
    pub fn new(count: usize) -> Option<Self> {
        Some(Frames {
            start: allocate_frames(count)?,
            count: count,
        })
    }

    pub fn iter(&self) -> FrameIter {
        Frame::range_inclusive(
            self.start.clone(),
            Frame::containing_address(PhysicalAddress::new(
                self.start.start_address().get() + self.count * PAGE_SIZE - 1,
            )),
        )
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        deallocate_frames(self.start.clone(), self.count)
    }
}

// TODO downgrade the Arc's to Rc's if safe
#[derive(Debug)]
pub struct ContextMemory {
    valloc_mapping: Option<Arc<VallocMapping>>,
    context_address: VirtualAddress,
    context_mapped: bool,
    frames: Arc<Frames>,
    flags: EntryFlags,
}

impl ContextMemory {
    pub fn new(count: usize, context_address: VirtualAddress, flags: EntryFlags) -> Option<Self> {
        assert!(count != 0);
        Some(ContextMemory {
            valloc_mapping: None,
            context_address,
            context_mapped: false,
            frames: Arc::new(Frames::new(count)?),
            flags: flags,
        })
    }

    pub fn new_kernel(count: usize, flags: EntryFlags) -> Option<(Self, VirtualAddress)> {
        assert!(count != 0);
        let mut memory = ContextMemory {
            valloc_mapping: None,
            context_address: VirtualAddress::new(0),
            context_mapped: false,
            frames: Arc::new(Frames::new(count)?),
            flags: flags,
        };
        let address = memory.map_to_kernel(flags)?;
        memory.context_address = address;
        Some((memory, address))
    }

    pub fn kernel_address(&self) -> Option<VirtualAddress> {
        Some(self.valloc_mapping.as_ref()?.pages.start_address())
    }

    pub fn len_bytes(&self) -> usize {
        self.frames.count * PAGE_SIZE
    }

    pub fn map_to_kernel(&mut self, flags: EntryFlags) -> Option<VirtualAddress> {
        if self.valloc_mapping.is_some() {
            return Some(self.valloc_mapping.as_ref().unwrap().pages.start_address());
        }
        let mapping = VallocMapping::new(flags, self.frames.clone())?;
        self.valloc_mapping = Some(Arc::new(mapping));
        Some(self.valloc_mapping.as_ref().unwrap().pages.start_address())
    }

    pub fn context_address(&self) -> VirtualAddress {
        self.context_address
    }

    pub fn page_count(&self) -> usize {
        self.frames.count
    }

    pub fn flags(&self) -> EntryFlags {
        self.flags
    }

    pub fn drop_kernel_mapping(&mut self) {
        drop(self.valloc_mapping.take())
    }

    pub fn as_slice(&self) -> &[u8] {
        self.valloc_mapping
            .as_ref()
            .expect("Map the memory to kernel first, before attempting to access it")
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        Arc::get_mut(
            self.valloc_mapping
                .as_mut()
                .expect("Map the memory to kernel first, before attempting to access it"),
        ).expect("Cannot mutate ref_cloned ContextMemory")
    }

    pub fn zero(&mut self) {
        let slice = self.as_slice_mut();
        unsafe { intrinsics::write_bytes(slice.as_mut_ptr(), 0, slice.len()) };
    }

    pub fn map_context(&mut self, mapper: &mut Mapper) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        if self.context_mapped {
            return flush_all;
        }

        let pages = Page::range_inclusive(
            Page::containing_address(self.context_address),
            Page::containing_address(VirtualAddress::new(
                self.context_address.get() + self.frames.count * PAGE_SIZE - 1,
            )),
        );

        for (page, frame) in pages.zip(self.frames.iter()) {
            let result = mapper.map_to(page, frame, self.flags);
            flush_all.consume(result);
        }

        self.context_mapped = true;

        flush_all
    }

    pub fn unmap_context(&mut self, mapper: &mut Mapper) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        if !self.context_mapped {
            return flush_all;
        }

        let pages = Page::range_inclusive(
            Page::containing_address(self.context_address),
            Page::containing_address(VirtualAddress::new(
                self.context_address.get() + self.frames.count * PAGE_SIZE - 1,
            )),
        );

        for page in pages {
            let (result, _) = mapper.unmap_return(page, false);
            flush_all.consume(result);
        }

        self.context_mapped = false;

        flush_all
    }

    pub fn remap_context(&mut self, mapper: &mut Mapper) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        if self.context_mapped {
            return flush_all;
        }

        let pages = Page::range_inclusive(
            Page::containing_address(self.context_address),
            Page::containing_address(VirtualAddress::new(
                self.context_address.get() + self.frames.count * PAGE_SIZE - 1,
            )),
        );

        for page in pages {
            let result = mapper.remap(page, self.flags);
            flush_all.consume(result);
        }

        self.context_mapped = true;

        flush_all
    }

    pub fn ref_clone(&self, new_address: Option<VirtualAddress>) -> Self {
        ContextMemory {
            valloc_mapping: self.valloc_mapping.clone(),
            context_address: new_address.unwrap_or(self.context_address),
            context_mapped: false,
            frames: self.frames.clone(),
            flags: self.flags,
        }
    }

    fn clone_internal(
        &self,
        new_address: Option<VirtualAddress>,
        new_count: Option<usize>,
    ) -> Option<Self> {
        let frames = Arc::new(Frames::new(new_count.unwrap_or(self.frames.count))?);

        let new_mapping = VallocMapping::new(
            EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE,
            frames.clone(),
        )?;

        Some(ContextMemory {
            valloc_mapping: Some(Arc::new(new_mapping)),
            context_address: new_address.unwrap_or(self.context_address),
            context_mapped: false,
            flags: self.flags,
            frames: frames,
        })
    }

    pub fn copy_clone(&self, new_address: Option<VirtualAddress>) -> Option<Self> {
        let old_mapping = self
            .valloc_mapping
            .as_ref()
            .map(|m| m.clone())
            .unwrap_or(Arc::new(VallocMapping::new(
                EntryFlags::NO_EXECUTE,
                self.frames.clone(),
            )?));

        let mut memory = self.clone_internal(new_address, None)?;

        memory.as_slice_mut().copy_from_slice(&old_mapping);

        Some(memory)
    }

    // TODO implement move procedure for new ContextMemory
    /// A complicated operation to move a piece of memory to a new page table
    /// It also allows for changing the address at the same time
    /*
    pub fn move_to(
        &mut self,
        new_start: VirtualAddress,
        new_table: &mut InactivePageTable,
        temporary_page: &mut TemporaryPage,
    ) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        for page in self.pages() {
            let (result, frame) = old_mapper.unmap_return(page, false);
            flush_all.consume(result);

            let new_page = Page::containing_address(VirtualAddress::new(
                page.start_address().get() - self.start.get() + new_start.get(),
            ));
            let result = new_mapper.map_to(new_page, frame, self.flags);
            flush_all.consume(result);
        }
        self.start = new_start;
        flush_all
    }
    */

    // FIXME very inefficient resize, it will reallocate and copy
    // This function may only run in active table
    pub fn resize(mut self, new_count: usize) -> Option<Self> {
        let mut active_table = unsafe { ActivePageTable::new() };

        let old_mapping = (&self)
            .valloc_mapping
            .as_ref()
            .map(|m| m.clone())
            .unwrap_or(Arc::new(VallocMapping::new(
                EntryFlags::NO_EXECUTE,
                self.frames.clone(),
            )?));

        let mut memory = (&self).clone_internal(None, Some(new_count))?;

        {
            let dst = memory.as_slice_mut();
            let src = &old_mapping;
            let dst_len = dst.len();
            if dst.len() > src.len() {
                dst[..src.len()].copy_from_slice(src);
                for i in src.len()..dst.len() {
                    dst[i] = 0;
                }
            } else {
                dst.copy_from_slice(&src[..dst_len]);
            }
        }

        let mut flush_all = self.unmap_context(&mut active_table);
        flush_all.consume_flush_all(memory.map_context(&mut active_table));
        flush_all.flush(&mut active_table);

        Some(memory)
    }
}

#[derive(Debug)]
pub struct ContextValues {
    memory: Option<ContextMemory>,
    offset: usize,
}

impl Deref for ContextValues {
    type Target = Option<ContextMemory>;

    fn deref(&self) -> &Self::Target {
        &self.memory
    }
}

impl ContextValues {
    pub fn new_no_memory() -> Self {
        ContextValues {
            memory: None,
            offset: 0,
        }
    }

    pub fn set_memory(&mut self, memory: ContextMemory) {
        self.memory = Some(memory)
    }

    // TODO this should be Result
    pub fn append_encode<T: SOS>(&mut self, values: &T) -> Option<VirtualAddress> {
        let length = self.memory.as_ref()?.len_bytes();
        let need = values.encoded_len();
        if length - self.offset < need {
            self.memory = self
                .memory
                .take()
                .unwrap()
                .resize(align_up!(length + need, PAGE_SIZE) / PAGE_SIZE);
        }
        let vaddr =
            VirtualAddress::new(self.memory.as_ref()?.context_address().get() + self.offset);
        let slice = &mut self.memory.as_mut()?.as_slice_mut()[self.offset..self.offset + need];
        self.offset += need;
        values.encode(slice);
        Some(vaddr)
    }
}
