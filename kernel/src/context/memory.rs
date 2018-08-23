use alloc::arc::{Arc, Weak};
use alloc::VecDeque;
use core::intrinsics;
use spin::Mutex;

use memory::Frame;
use paging::entry::EntryFlags;
use paging::mapper::{Mapper, MapperFlushAll};
use paging::temporary_page::TemporaryPage;
use paging::{ActivePageTable, InactivePageTable, Page, PageIter, PhysicalAddress, VirtualAddress};

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
pub struct Memory {
    start: VirtualAddress,
    size: usize,
    flags: EntryFlags,
    mapped: bool,
    page_table: usize,
}

impl Memory {
    pub fn new(start: VirtualAddress, size: usize, flags: EntryFlags, page_table: usize) -> Self {
        let mut memory = Memory {
            start: start,
            size: size,
            flags: flags,
            mapped: false,
            page_table: page_table,
        };

        memory
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

    pub fn pages(&self) -> PageIter {
        let start_page = Page::containing_address(self.start);
        let end_page =
            Page::containing_address(VirtualAddress::new(self.start.get() + self.size - 1));
        Page::range_inclusive(start_page, end_page)
    }

    fn map(&mut self, mapper: &mut Mapper) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        for page in self.pages() {
            let result = mapper.map(page, self.flags);
            flush_all.consume(result);
        }

        self.mapped = true;

        flush_all
    }

    fn unmap(&mut self, mapper: &mut Mapper) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        for page in self.pages() {
            let result = mapper.unmap(page);
            flush_all.consume(result);
        }

        self.mapped = false;

        flush_all
    }

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

    pub fn remap(&mut self, new_flags: EntryFlags, mapper: &mut Mapper) -> MapperFlushAll {
        let mut flush_all = MapperFlushAll::new();

        for page in self.pages() {
            let result = mapper.remap(page, new_flags);
            flush_all.consume(result);
        }

        self.flags = new_flags;

        flush_all
    }

    pub fn resize(&mut self, new_size: usize, clear: bool) {
        let mut active_table = unsafe { ActivePageTable::new() };

        // Sanity check that ensures that we can only resize the memory if its in the current page table
        assert_eq!(unsafe { active_table.address() }, self.page_table);

        //TODO: Calculate page changes to minimize operations
        if new_size > self.size {
            let mut flush_all = MapperFlushAll::new();

            let start_page =
                Page::containing_address(VirtualAddress::new(self.start.get() + self.size));
            let end_page =
                Page::containing_address(VirtualAddress::new(self.start.get() + new_size - 1));
            for page in Page::range_inclusive(start_page, end_page) {
                if active_table.translate_page(page).is_none() {
                    let result = active_table.map(page, self.flags);
                    flush_all.consume(result);
                }
            }

            flush_all.flush(&mut active_table);

            if clear {
                unsafe {
                    intrinsics::write_bytes(
                        (self.start.get() + self.size) as *mut u8,
                        0,
                        new_size - self.size,
                    );
                }
            }
        } else if new_size < self.size {
            let mut flush_all = MapperFlushAll::new();

            let start_page =
                Page::containing_address(VirtualAddress::new(self.start.get() + new_size));
            let end_page =
                Page::containing_address(VirtualAddress::new(self.start.get() + self.size - 1));
            for page in Page::range_inclusive(start_page, end_page) {
                if active_table.translate_page(page).is_some() {
                    let result = active_table.unmap(page);
                    flush_all.consume(result);
                }
            }

            flush_all.flush(&mut active_table);
        }

        self.size = new_size;
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        if self.mapped {
            panic!("Memory handle dropped while mapped");
        }
    }
}
