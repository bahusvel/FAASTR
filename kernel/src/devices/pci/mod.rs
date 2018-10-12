pub use self::bar::PciBar;
pub use self::bus::{PciBus, PciBusIter};
pub use self::class::PciClass;
pub use self::debug::print_pci_device;
pub use self::dev::{PciDev, PciDevIter};
pub use self::func::PciFunc;
pub use self::header::{PciHeader, PciHeaderError, PciHeaderType};
pub use self::intx::pci_intx;
use alloc::vec::Vec;

mod bar;
mod bus;
mod class;
mod debug;
mod dev;
mod func;
pub mod header;
mod intx;
pub mod msix;

pub struct Pci;

impl Pci {
    pub fn new() -> Self {
        Pci
    }

    pub fn buses<'pci>(&'pci self) -> PciIter<'pci> {
        PciIter::new(self)
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub unsafe fn read(&self, bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
        let address = 0x80000000
            | ((bus as u32) << 16)
            | ((dev as u32) << 11)
            | ((func as u32) << 8)
            | ((offset as u32) & 0xFC);
        let value: u32;
        asm!("mov dx, 0xCF8
              out dx, eax
              mov dx, 0xCFC
              in eax, dx"
             : "={eax}"(value) : "{eax}"(address) : "dx" : "intel", "volatile");
        value
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub unsafe fn write(&self, bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
        let address = 0x80000000
            | ((bus as u32) << 16)
            | ((dev as u32) << 11)
            | ((func as u32) << 8)
            | ((offset as u32) & 0xFC);
        asm!("mov dx, 0xCF8
              out dx, eax"
             : : "{eax}"(address) : "dx" : "intel", "volatile");
        asm!("mov dx, 0xCFC
              out dx, eax"
             : : "{eax}"(value) : "dx" : "intel", "volatile");
    }
}

pub struct PciDevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub header: PciHeader,
    pub pci: Pci,
}

impl PciDevice {
    pub fn find_by_id(vendor_id: u16, device_id: u16) -> Vec<Self> {
        let pci = Pci::new();
        let mut vec = Vec::new();
        for bus in pci.buses() {
            for dev in bus.devs() {
                for func in dev.funcs() {
                    let header = PciHeader::from_reader(func.clone());
                    if header.is_ok() && {
                        let header = header.unwrap();
                        header.vendor_id() == vendor_id && header.device_id() == device_id
                    } {
                        vec.push(PciDevice {
                            bus: func.dev.bus.num,
                            dev: func.dev.num,
                            func: func.num,
                            header: PciHeader::from_reader(func.clone()).unwrap(),
                            pci: Pci::new(),
                        })
                    }
                }
            }
        }
        vec
    }
    pub unsafe fn read(&self, offset: u8) -> u32 {
        self.pci.read(self.bus, self.dev, self.func, offset)
    }
    pub unsafe fn write(&self, offset: u8, value: u32) {
        self.pci.write(self.bus, self.dev, self.func, offset, value)
    }
}

pub struct PciIter<'pci> {
    pci: &'pci Pci,
    num: u32,
}

impl<'pci> PciIter<'pci> {
    pub fn new(pci: &'pci Pci) -> Self {
        PciIter { pci: pci, num: 0 }
    }
}

impl<'pci> Iterator for PciIter<'pci> {
    type Item = PciBus<'pci>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.num < 255 {
            /* TODO: Do not ignore 0xFF bus */
            let bus = PciBus {
                pci: self.pci,
                num: self.num as u8,
            };
            self.num += 1;
            Some(bus)
        } else {
            None
        }
    }
}
