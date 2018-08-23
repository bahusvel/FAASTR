use super::memory::Memory;
use alloc::arc::Arc;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::{BTreeMap, Vec};
use core::alloc::{GlobalAlloc, Layout};
use core::ops::{Deref, DerefMut};
use core::str;
use elf::{self, program_header, Elf};
use memory::{allocate_mapped_pages, Frame, VallocPages};
use paging::entry::EntryFlags;
use paging::mapper::MapperFlushAll;
use paging::{ActivePageTable, Mapper, VirtualAddress};
use spin::{Once, RwLock};
use syscall::error::*;

lazy_static! {
    pub static ref KERNEL_MODULE: SharedModule = Arc::new(Module {
        name: String::from("kernel"),
        func_table: BTreeMap::new(),
        image: Vec::new(),
        actions: BTreeMap::new(),
        env: BTreeMap::new(),
        bindings: BTreeMap::new(),
    });
    static ref MODULE_CACHE: RwLock<BTreeMap<String, SharedModule>> = RwLock::new(BTreeMap::new());
}

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
    pub fn map_to_context(&self, mapper: &mut Mapper) -> Memory {
        Memory::new(self.start, 0, self.flags, 0)
    }
}

#[derive(Debug)]
pub struct MappingPages(VallocPages);

impl MappingPages {
    pub unsafe fn new(num: usize) -> Self {
        MappingPages(
            allocate_mapped_pages(num, EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE)
                .expect("Failed to allocate mapped pages"),
        )
    }
}

impl Deref for MappingPages {
    type Target = VallocPages;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MappingPages {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
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

            println!("Entrypoint {}", elf.entry());
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
            let num_pages = ((size + 4095) & (!4095)) / 4096;
            println!(
                "Segment voff={}, vaddr={}, size={}, num_pages={}",
                voff, vaddr, size, num_pages
            );

            let mut pages = unsafe { MappingPages::new(num_pages) };
            {
                let mut pages_mem = unsafe { pages.to_slice_mut() };
                //Zero out head
                for i in 0..voff {
                    pages_mem[i as usize] = 0;
                }
                //Load in the section
                pages_mem[voff as usize..size].copy_from_slice(
                    &elf.data
                        [segment.p_offset as usize..(segment.p_offset + segment.p_filesz) as usize],
                );
                //Zero out tail
                for i in size..pages_mem.len() {
                    pages_mem[i] = 0;
                }
            }

            let mut flags = EntryFlags::NO_EXECUTE;

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

pub fn load_and_cache(name: &str, data: &[u8]) -> Result<SharedModule> {
    let module = load(name, data)?.to_shared();
    MODULE_CACHE
        .write()
        .insert(String::from(name), module.clone());
    Ok(module)
}

pub fn cached_module(name: &str) -> Option<SharedModule> {
    MODULE_CACHE.read().get(name).map(|v| v.clone())
}
