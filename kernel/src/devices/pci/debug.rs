use super::{PciClass, PciDevice};

pub fn print_pci_device(dev: &PciDevice) {
    let raw_class: u8 = dev.header.class().into();
    let mut string = format!(
        "PCI {:>02X}/{:>02X}/{:>02X} {:>04X}:{:>04X} {:>02X}.{:>02X}.{:>02X}.{:>02X} {:?}",
        dev.bus,
        dev.dev,
        dev.func,
        dev.header.vendor_id(),
        dev.header.device_id(),
        raw_class,
        dev.header.subclass(),
        dev.header.interface(),
        dev.header.revision(),
        dev.header.class()
    );

    match dev.header.class() {
        PciClass::Storage => match dev.header.subclass() {
            0x01 => {
                string.push_str(" IDE");
            }
            0x06 => {
                string.push_str(" SATA");
            }
            _ => (),
        },
        PciClass::SerialBus => match dev.header.subclass() {
            0x03 => match dev.header.interface() {
                0x00 => {
                    string.push_str(" UHCI");
                }
                0x10 => {
                    string.push_str(" OHCI");
                }
                0x20 => {
                    string.push_str(" EHCI");
                }
                0x30 => {
                    string.push_str(" XHCI");
                }
                _ => (),
            },
            _ => (),
        },
        _ => (),
    }

    for (i, bar) in dev.header.bars().iter().enumerate() {
        if !bar.is_none() {
            string.push_str(&format!(" {}={}", i, bar));
        }
    }

    string.push('\n');

    print!("{}", string);
}
