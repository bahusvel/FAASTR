use super::PciDevice;

const PCI_COMMAND: u16 = 0x04;
const PCI_COMMAND_INTX_DISABLE: u32 = 0x400;
const PCI_INTERRUPT_LINE: u8 = 0x3c;

pub unsafe fn pci_intx(dev: &PciDevice, enable: bool) {
    // TODO remove the magic numbers make it constants
    dev.write(0x04, dev.read(0x04) | 7);

    let command = dev.read(PCI_COMMAND as u8);
    let new = if enable {
        command & !PCI_COMMAND_INTX_DISABLE
    } else {
        command | PCI_COMMAND_INTX_DISABLE
    };

    if new != command {
        dev.write(PCI_COMMAND as u8, new);
    }
}

pub unsafe fn pci_irq_vector(dev: &PciDevice) -> u8 {
    println!("Interrupt pin {}", dev.read(PCI_INTERRUPT_LINE + 1));
    let data = dev.read(PCI_INTERRUPT_LINE);
    let irq = (data & 0xFF) as u8;
    println!("Original IRQ {}", irq);
    if irq == 0xff {
        dev.write(PCI_INTERRUPT_LINE, (data & 0xFFFFFF00) | 9);
        return 9;
    }
    irq
}
