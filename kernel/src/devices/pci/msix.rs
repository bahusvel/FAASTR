use super::{Pci, PciBar, PciDevice};
use arch;
use core::ptr::write_volatile;
use syscall::flag::MAP_WRITE;
use syscall::physmap;

const PCI_MSIX_ENTRY_CTRL_MASKBIT: u32 = 1;
const PCI_MSIX_ENTRY_SIZE: u32 = 16;
const MSI_ADDRESS_BASE: u64 = 0xfee00000;
const MSI_DESTINATION_ID_SHIFT: u64 = 12;
const MSI_NO_REDIRECTION: u64 = 0x00000000;
const MSI_DESTINATION_MODE_PHYSICAL: u64 = 0x00000000;
const MSI_TRIGGER_MODE_EDGE: u32 = 0x00000000;
const MSI_DELIVERY_MODE_FIXED: u32 = 0x00000000;
const ARCH_INTERRUPT_BASE: u32 = 0x20;
const MSI_DELIVERY_MODE_NMI: u32 = 0x00000400;
const PCI_CAP_ID_MSIX: u32 = 0x11;
const PCI_MSIX_FLAGS: u32 = 0x2;
const PCI_MSIX_FLAGS_MASKALL: u32 = 0x4000;
const PCI_MSIX_FLAGS_ENABLE: u32 = 0x8000;
const PCI_MSIX_FLAGS_QSIZE: u32 = 0x07FF;
const PCI_MSIX_TABLE: u32 = 4;
const PCI_MSIX_TABLE_BIR: u32 = 0x00000007;
const PCI_MSIX_TABLE_OFFSET: u32 = 0xfffffff8;

pub unsafe fn pci_init_msix(dev: &PciDevice) {
    let pci = Pci::new();

    let msix_cap = pci_find_capability(dev, PCI_CAP_ID_MSIX);
    assert!(msix_cap != 0);
    println!("Cap {}", msix_cap);
    let mut control = dev.read((msix_cap + PCI_MSIX_FLAGS) as u8);

    control |= PCI_MSIX_FLAGS_MASKALL | PCI_MSIX_FLAGS_ENABLE;

    dev.write((msix_cap + PCI_MSIX_FLAGS) as u8, control);

    let msix_size = (control & PCI_MSIX_FLAGS_QSIZE) + 1;
    println!("msix_size {}", msix_size);
    let base = pci_msix_map_region(dev, msix_size, msix_cap).expect("Failed to map msi-x bar");

    //pci_irq_vector(dev);
    // Best I can trace it, on x86 msi_domain_alloc_irqs does this job.

    pci_msix_program_entry(base, 0);
    pci_msix_program_entry(base, 1);

    control &= !PCI_MSIX_FLAGS_MASKALL;
    control |= PCI_MSIX_FLAGS_ENABLE;

    pci.write(
        dev.bus,
        dev.dev,
        dev.func,
        (msix_cap + PCI_MSIX_FLAGS) as u8,
        control,
    );
}

pub unsafe fn pci_msix_map_region(
    dev: &PciDevice,
    num_entries: u32,
    msix_cap: u32,
) -> Result<usize, &'static str> {
    let pci = Pci::new();

    let mut table_offset = pci.read(
        dev.bus,
        dev.dev,
        dev.func,
        (msix_cap + PCI_MSIX_TABLE) as u8,
    );

    let bir = (table_offset & PCI_MSIX_TABLE_BIR) as u8;
    table_offset &= PCI_MSIX_TABLE_OFFSET;

    println!("bir {}", bir);

    if let PciBar::Memory(shared_bar) = dev.header.get_bar(bir as usize) {
        println!("phys_addr {}", shared_bar + table_offset);
        let mapping = physmap(
            (shared_bar + table_offset) as usize,
            (num_entries * PCI_MSIX_ENTRY_SIZE) as usize,
            MAP_WRITE,
        ).expect("Failed to map physical ");
        Ok(mapping)
    } else {
        Err("Device doesnt have MSI-X table BAR")
    }
}

unsafe fn pci_msix_program_entry(base: usize, nr: u32) {
    let apic = &arch::device::local_apic::LOCAL_APIC;

    let cpu_apic_id = apic.id() as u64;
    println!("Apic id: {}", cpu_apic_id);

    let vector = 9 + nr;

    let address = MSI_ADDRESS_BASE
        | (cpu_apic_id << MSI_DESTINATION_ID_SHIFT)
        | MSI_NO_REDIRECTION
        | MSI_DESTINATION_MODE_PHYSICAL;

    let data = MSI_TRIGGER_MODE_EDGE | MSI_DELIVERY_MODE_FIXED | (vector + ARCH_INTERRUPT_BASE);

    let entry = (base + (nr * PCI_MSIX_ENTRY_SIZE) as usize) as *mut u32;
    write_volatile(
        entry.offset(3),
        *entry.offset(3) | PCI_MSIX_ENTRY_CTRL_MASKBIT,
    );
    write_volatile(entry, address as u32);
    write_volatile(entry.offset(1), (address >> 32) as u32);
    write_volatile(entry.offset(2), data);

    write_volatile(
        entry.offset(3),
        *entry.offset(3) & !PCI_MSIX_ENTRY_CTRL_MASKBIT,
    );
}

fn pci_find_capability(dev: &PciDevice, cap: u32) -> u32 {
    const PCI_CAPABILITY_LIST: u32 = 0x34;
    let pci = Pci::new();

    let mut ttl = 48;
    let mut pos = unsafe { dev.read(PCI_CAPABILITY_LIST as u8) };
    while ttl > 0 {
        if pos < 0x40 {
            break;
        }

        pos &= !3;

        let ent = unsafe { dev.read(pos as u8) };
        print!("{} ", ent);
        let id = ent & 0xff;
        if id == 0xff {
            break;
        }
        if id == cap {
            return pos;
        }
        pos = ent >> 8;

        ttl -= 1;
    }
    println!();
    0
}
