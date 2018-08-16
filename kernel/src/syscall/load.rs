use syscall::error::*;
use alloc::{BTreeMap, Vec};
use elf::{self, program_header};
use core::str;
use alloc::boxed::Box;
use paging::{ActivePageTable, VirtualAddress};
use paging::entry::EntryFlags;
use alloc::string::String;
use core::alloc::{GlobalAlloc, Layout};
use memory::Frame;
use core::ops::{Deref, DerefMut};

type FunctionPtr = usize;

#[allow(dead_code)]
pub struct Module {
    name: String,
    func_table: BTreeMap<String, FunctionPtr>,
    image: Vec<Section>,
    actions: BTreeMap<usize, usize>,
    env: BTreeMap<String, Vec<u8>>,
    bindings: BTreeMap<usize, FunctionPtr>,
}

#[allow(dead_code)]
struct Section {
    start: VirtualAddress,
    flags: EntryFlags,
    pages: MappingPages,
}

struct MappingPages(Box<[u8]>);

impl MappingPages {
    unsafe fn new(num: usize) -> Self {
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

struct MappingIter<'a> {
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
        let addr = unsafe {
            self.table
                .translate(VirtualAddress::new(
                    self.pages.as_ptr().offset(self.next) as usize,
                ))
                .expect("Mapping page is unmapped")
        };
        self.next += 1;
        Some(Frame::containing_address(addr))
    }
}

pub fn load(name: &str, data: &[u8]) -> Result<Module> {
    match elf::Elf::from(&data) {
        Ok(elf) => {
            // We check the validity of all loadable sections here
            for segment in elf.segments() {
                if segment.p_type == program_header::PT_LOAD {
                    let voff = segment.p_vaddr % 4096;
                    let vaddr = segment.p_vaddr - voff;

                    // Due to the Userspace and kernel TLS bases being located right above 2GB,
                    // limit any loadable sections to lower than that. Eventually we will need
                    // to replace this with a more intelligent TLS address
                    if vaddr >= 0x8000_0000 {
                        println!("exec: invalid section address {:X}", segment.p_vaddr);
                        return Err(Error::new(ENOEXEC));
                    }
                }
            }
        }
        Err(err) => {
            println!("exec: failed to execute {}: {}", name, err);
            return Err(Error::new(ENOEXEC));
        }
    }

    let mut image = Vec::new();

    {
        let elf = elf::Elf::from(&data).unwrap();
        for segment in elf.segments() {
            if segment.p_type != program_header::PT_LOAD {
                continue;
            }
            let voff = segment.p_vaddr % 4096;
            let vaddr = segment.p_vaddr - voff;
            let size = segment.p_memsz as usize + voff as usize;

            let mut pages = unsafe { MappingPages::new(size / 4096) };

            //Zero out head
            for i in 0..voff {
                pages[i as usize] = 0;
            }
            //Load in the section
            pages.copy_from_slice(
                &elf.data[segment.p_offset as usize..segment.p_filesz as usize],
            );
            //Zero out tail
            for i in size..pages.len() {
                pages[i] = 0;
            }

            let mut flags = EntryFlags::NO_EXECUTE | EntryFlags::USER_ACCESSIBLE;

            if segment.p_flags & program_header::PF_R == program_header::PF_R {
                flags.insert(EntryFlags::PRESENT);
            }

            // W ^ X. If it is executable, do not allow it to be writable, even if requested
            if segment.p_flags & program_header::PF_X == program_header::PF_X {
                flags.remove(EntryFlags::NO_EXECUTE);
            } else if segment.p_flags & program_header::PF_W == program_header::PF_W {
                flags.insert(EntryFlags::WRITABLE);
            }

            let mut section = Section {
                start: VirtualAddress::new(vaddr as usize),
                pages: pages,
                flags: flags,
            };

            image.push(section);
        }
    }

    Ok(Module {
        name: String::from(name),
        func_table: BTreeMap::new(),
        image: image,
        actions: BTreeMap::new(),
        env: BTreeMap::new(),
        bindings: BTreeMap::new(),
    })
}
