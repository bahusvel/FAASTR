//! ELF executables

use alloc::str::from_utf8;
use alloc::string::String;

use goblin::elf::section_header::{SHT_STRTAB, SHT_SYMTAB};

#[cfg(target_arch = "x86")]
pub use goblin::elf32::{header, program_header, section_header, sym};

#[cfg(target_arch = "x86_64")]
pub use goblin::elf64::{header, program_header, section_header, sym};

/// An ELF executable
pub struct Elf<'a> {
    pub data: &'a [u8],
    strtab: Option<&'a [u8]>,
    shstrtab: Option<&'a [u8]>,
    header: &'a header::Header,
}

impl<'a> Elf<'a> {
    /// Create a ELF executable from data
    pub fn from(data: &'a [u8]) -> Result<Elf<'a>, String> {
        if data.len() < header::SIZEOF_EHDR {
            Err(format!(
                "Elf: Not enough data: {} < {}",
                data.len(),
                header::SIZEOF_EHDR
            ))
        } else if &data[..header::SELFMAG] != header::ELFMAG {
            Err(format!(
                "Elf: Invalid magic: {:?} != {:?}",
                &data[..header::SELFMAG],
                header::ELFMAG
            ))
        } else if data.get(header::EI_CLASS) != Some(&header::ELFCLASS) {
            Err(format!(
                "Elf: Invalid architecture: {:?} != {:?}",
                data.get(header::EI_CLASS),
                header::ELFCLASS
            ))
        } else {
            let mut ef = Elf {
                data: data,
                strtab: None,
                shstrtab: None,
                header: unsafe { &*(data.as_ptr() as usize as *const header::Header) },
            };

            ef.strtab = ef
                .sections()
                .find(|h| h.sh_type == SHT_STRTAB)
                .map(|h| &data[h.sh_offset as usize..(h.sh_offset + h.sh_size) as usize]);

            ef.shstrtab = ef
                .sections()
                .nth(ef.header.e_shstrndx as usize)
                .map(|h| &data[h.sh_offset as usize..(h.sh_offset + h.sh_size) as usize]);

            Ok(ef)
        }
    }

    pub fn sections(&'a self) -> ElfSections<'a> {
        ElfSections {
            data: self.data,
            header: self.header,
            i: 0,
        }
    }

    pub fn lookup_section_name(&'a self, index: u32) -> Option<&'a str> {
        let mut end = index as usize;
        if self.shstrtab.is_none() {
            return None;
        }
        while end < self.shstrtab.unwrap().len() {
            let b = self.shstrtab.unwrap()[end];
            end += 1;
            if b == 0 {
                break;
            }
        }
        if end > index as usize {
            let sym_name = &self.shstrtab.unwrap()[index as usize..end - 1];
            from_utf8(sym_name).ok()
        } else {
            None
        }
    }

    pub fn section_data_by_name(&'a self) -> Option<&'a [u8]> {
        for section in self.sections() {
            let name = self.lookup_section_name(section.sh_name)?;
            if name == ".manifest" {
                return Some(
                    &self.data[section.sh_offset as usize
                                   ..(section.sh_offset + section.sh_size) as usize],
                );
            }
        }
        None
    }

    pub fn segments(&'a self) -> ElfSegments<'a> {
        ElfSegments {
            data: self.data,
            header: self.header,
            i: 0,
        }
    }

    pub fn symbols(&'a self) -> Option<ElfSymbols<'a>> {
        let mut symtab_opt = None;
        for section in self.sections() {
            if section.sh_type == SHT_SYMTAB {
                symtab_opt = Some(section);
                break;
            }
        }

        if let Some(symtab) = symtab_opt {
            Some(ElfSymbols {
                data: self.data,
                header: self.header,
                symtab: symtab,
                i: 0,
            })
        } else {
            None
        }
    }

    /// Get the entry field of the header
    pub fn entry(&self) -> usize {
        self.header.e_entry as usize
    }
}

pub struct ElfSections<'a> {
    data: &'a [u8],
    header: &'a header::Header,
    i: usize,
}

impl<'a> Iterator for ElfSections<'a> {
    type Item = &'a section_header::SectionHeader;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i < self.header.e_shnum as usize {
            let item = unsafe {
                &*((self.data.as_ptr() as usize
                    + self.header.e_shoff as usize
                    + self.i * self.header.e_shentsize as usize)
                    as *const section_header::SectionHeader)
            };
            self.i += 1;
            Some(item)
        } else {
            None
        }
    }
}

pub struct ElfSegments<'a> {
    data: &'a [u8],
    header: &'a header::Header,
    i: usize,
}

impl<'a> Iterator for ElfSegments<'a> {
    type Item = &'a program_header::ProgramHeader;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i < self.header.e_phnum as usize {
            let item = unsafe {
                &*((self.data.as_ptr() as usize
                    + self.header.e_phoff as usize
                    + self.i * self.header.e_phentsize as usize)
                    as *const program_header::ProgramHeader)
            };
            self.i += 1;
            Some(item)
        } else {
            None
        }
    }
}

pub struct ElfSymbols<'a> {
    data: &'a [u8],
    header: &'a header::Header,
    symtab: &'a section_header::SectionHeader,
    i: usize,
}

impl<'a> Iterator for ElfSymbols<'a> {
    type Item = &'a sym::Sym;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i < (self.symtab.sh_size as usize) / sym::SIZEOF_SYM {
            let item = unsafe {
                &*((self.data.as_ptr() as usize
                    + self.symtab.sh_offset as usize
                    + self.i * sym::SIZEOF_SYM) as *const sym::Sym)
            };
            self.i += 1;
            Some(item)
        } else {
            None
        }
    }
}
