use super::memory::ContextMemory;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::str;
use elf::{self, program_header};
use error::*;
use hashmap_core::fnv::FnvHashMap;
use paging::entry::EntryFlags;
use paging::VirtualAddress;
use serde_json_core::de::from_slice;
use spin::RwLock;

pub const INVALID_FUNCTION: ModuleFuncPtr = 0;

lazy_static! {
    pub static ref KERNEL_MODULE: SharedModule = Arc::new(Module {
        name: String::from("kernel"),
        func_table: FnvHashMap::new(),
        image: Vec::new(),
        actions: FnvHashMap::new(),
        env: FnvHashMap::new(),
        bindings: FnvHashMap::new(),
    });
    static ref MODULE_CACHE: RwLock<FnvHashMap<String, SharedModule>> =
        RwLock::new(FnvHashMap::new());
}

pub type ModuleFuncPtr = usize;
pub type FuncPtr = (SharedModule, ModuleFuncPtr);
pub type SharedModule = Arc<Module>;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Module {
    name: String,
    func_table: FnvHashMap<String, ModuleFuncPtr>,
    pub image: Vec<ContextMemory>,
    actions: FnvHashMap<usize, ModuleFuncPtr>,
    env: FnvHashMap<String, Vec<u8>>,
    bindings: FnvHashMap<usize, ModuleFuncPtr>,
}

impl Module {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn to_shared(self) -> SharedModule {
        Arc::new(self)
    }

    pub fn function(&self, name: &str) -> Option<ModuleFuncPtr> {
        Some(*self.func_table.get(name)?)
    }
}

#[derive(Deserialize, Debug)]
struct SymbolTableEntry<'a> {
    #[serde(rename = "Name")]
    name: &'a str,
    #[serde(rename = "Offset")]
    offset: usize,
    #[serde(rename = "Visibility")]
    visibility: usize,
    #[serde(rename = "ABI")]
    abi: usize,
}

#[derive(Deserialize, Debug)]
struct Manifest<'a> {
    #[serde(rename = "ModuleName")]
    module_name: &'a str,
    #[serde(rename = "SymbolTable")]
    symbol_table: Vec<SymbolTableEntry<'a>>,
}

fn load_manifest<'i>(elf: &'i elf::Elf) -> Result<'static, Manifest<'i>> {
    let section = elf
        .section_data_by_name()
        .ok_or("Manifest section not found")?;

    Ok(from_slice(section).map_err(|_| "Failed to decode manifest")?)
}

pub fn load(data: &[u8]) -> Result<'static, Module> {
    let elf = elf::Elf::from(&data).map_err(|_| "Failed to parse elf")?;

    // We check the validity of all loadable sections here
    for segment in elf.segments() {
        if segment.p_type == program_header::PT_LOAD {
            let voff = segment.p_vaddr % 4096;
            let vaddr = segment.p_vaddr - voff;

            if vaddr >= ::USER_ARG_OFFSET as u64 {
                println!("exec: invalid section address {:X}", segment.p_vaddr);
                return Err("Binary requested to load code above permitted vaddr");
            }
        }
    }

    let manifest = load_manifest(&elf)?;

    let mut func_table = FnvHashMap::new();

    for func in manifest.symbol_table {
        func_table.insert(String::from(func.name), func.offset);
    }

    let mut image = Vec::new();

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

        let mut section = ContextMemory::new(num_pages, VirtualAddress::new(vaddr as usize), flags)
            .expect("Failed to allocte context memory");

        section.map_to_kernel(EntryFlags::WRITABLE | EntryFlags::NO_EXECUTE);

        {
            let mut pages_mem = section.as_slice_mut();
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

        image.push(section);
    }

    Ok(Module {
        name: String::from(manifest.module_name),
        func_table: func_table,
        image: image,
        actions: FnvHashMap::new(),
        env: FnvHashMap::new(),
        bindings: FnvHashMap::new(),
    })
}

pub fn load_and_cache(data: &[u8]) -> Result<'static, SharedModule> {
    let module = load(data)?.to_shared();
    MODULE_CACHE
        .write()
        .insert(module.name.clone(), module.clone());
    Ok(module)
}

pub fn cached_module(name: &str) -> Option<SharedModule> {
    MODULE_CACHE.read().get(name).map(|v| v.clone())
}
