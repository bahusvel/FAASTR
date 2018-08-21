use alloc::arc::Arc;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::{BTreeMap, Vec};
use core::alloc::{GlobalAlloc, Layout};
use core::ops::{Deref, DerefMut};
use memory::Frame;
use paging::entry::EntryFlags;
use paging::{ActivePageTable, VirtualAddress};

pub static KERNEL_MODULE: SharedModule = Arc::new(Module {
    name: String::from("kernel"),
    func_table: BTreeMap::new(),
    image: Vec::new(),
    actions: BTreeMap::new(),
    env: BTreeMap::new(),
    bindings: BTreeMap::new(),
});

type FunctionPtr = usize;

pub type SharedModule = Arc<Module>;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Module {
    name: String,
    func_table: BTreeMap<String, FunctionPtr>,
    pub image: Vec<Section>,
    actions: BTreeMap<usize, usize>,
    env: BTreeMap<String, Vec<u8>>,
    bindings: BTreeMap<usize, FunctionPtr>,
}

impl Module {
    pub fn to_shared(self) -> SharedModule {
        Arc::new(self)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Section {
    pub start: VirtualAddress,
    pub flags: EntryFlags,
    pub pages: MappingPages,
}

impl Section {
    pub fn size(&self) -> usize {
        self.pages.len()
    }
}

#[derive(Debug)]
pub struct MappingPages(Box<[u8]>);

impl MappingPages {
    pub unsafe fn new(num: usize) -> Self {
        MappingPages(
            Vec::from_raw_parts(
                ::ALLOCATOR.alloc(Layout::from_size_align_unchecked(num * 4096, 4096)) as *mut u8,
                num * 4096,
                num * 4096,
            ).into_boxed_slice(),
        )
    }
    pub fn frames(&self) -> MappingIter {
        MappingIter {
            pages: self,
            table: unsafe { ActivePageTable::new() },
            next: 0,
        }
    }
}

impl Deref for MappingPages {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl DerefMut for MappingPages {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

pub struct MappingIter<'a> {
    pages: &'a MappingPages,
    table: ActivePageTable,
    next: isize,
}

impl<'a> Iterator for MappingIter<'a> {
    type Item = Frame;

    fn next(&mut self) -> Option<Frame> {
        if self.next as usize >= (self.pages.0.len() / 4096) {
            return None;
        }
        let phys_addr = unsafe {
            self.table
                .translate(VirtualAddress::new(
                    self.pages.as_ptr().offset(self.next) as usize
                )).expect("Mapping page is unmapped")
        };
        self.next += 1;
        Some(Frame::containing_address(phys_addr))
    }
}
