use alloc::vec::Vec;
use byteorder::{ByteOrder, NativeEndian};
use core::mem;
use core::slice;
use devices::pci::{Pci, PciBar, PciClass, PciHeader, PciHeaderError};
use ringbuf::{Consumer, Producer};
use sos::{EncodedValues, Value, SOS};
use spin::Mutex;
use syscall::flag::MAP_WRITE;
use syscall::physmap;

const VID: u16 = 0x1af4;
const DID: u16 = 0x1110;
const BUFFER_SIZE: usize = 1024 * 1024;
const MMIO_SIZE: usize = 256;

enum MsgType {
    Cast,
    Fuse,
    Return,
    Error,
}

impl From<u8> for MsgType {
    fn from(v: u8) -> Self {
        match v {
            0 => MsgType::Cast,
            1 => MsgType::Fuse,
            2 => MsgType::Return,
            3 => MsgType::Error,
            _ => panic!("Invalid message type"),
        }
    }
}

const IVSHRPC_HEADER_SIZE: usize = mem::size_of::<ProtoMsgLen>() + 1;
type ProtoMsgLen = u32;

lazy_static! {
    static ref HEADER: PciHeader = {
        let header = get_pci_header().expect("Could not find a compatible ivshmem device!");
        header.0
    };
    static ref BUFFER_PTR: usize = {
        if let PciBar::Memory(shared_bar) = HEADER.get_bar(2) {
            let mapping = physmap(shared_bar as usize, BUFFER_SIZE, MAP_WRITE)
                .expect("Failed to map physical ");

            println!("ivshrpc found and initialised");
            mapping
        } else {
            panic!("2nd bar of ivshmem is not memory mapped");
        }
    };
    static ref MMIO_BAR: usize = {
        if let PciBar::Memory(shared_bar) = HEADER.get_bar(0) {
            let mapping = physmap(shared_bar as usize, MMIO_SIZE, MAP_WRITE)
                .expect("Failed to map physical ");
            println!("ivshrpc-mmio found and initialised");

            unsafe { *(mapping as *mut [u8; 4]) = [0xFF, 0xFF, 0xFF, 0xFF] };

            mapping
        } else {
            panic!("0th bar of ivshmem is not memory mapped");
        }
    };
    static ref CONSUMER: Mutex<Consumer<'static>> = unsafe {
        let buffer = slice::from_raw_parts_mut(*BUFFER_PTR as *mut u8, BUFFER_SIZE / 2);
        let _ = *MMIO_BAR;
        Mutex::new(Consumer::from_slice(buffer))
    };
    static ref PRODUCER: Mutex<Producer<'static>> = unsafe {
        let buffer = slice::from_raw_parts_mut(
            (*BUFFER_PTR as *mut u8).offset((BUFFER_SIZE / 2) as isize),
            BUFFER_SIZE / 2,
        );
        Mutex::new(Producer::from_slice(buffer))
    };
}

fn get_pci_header() -> Option<(PciHeader, PciDev)> {
    let pci = Pci::new();
    for bus in pci.buses() {
        for dev in bus.devs() {
            for func in dev.funcs() {
                let pci_dev = PciDev {
                    bus: bus.num,
                    dev: dev.num,
                    func: func.num,
                };
                match PciHeader::from_reader(func) {
                    Ok(header) => {
                        if header.vendor_id() == VID
                            && header.device_id() == DID
                            && header.revision() == 1
                        {
                            return Some((header, pci_dev));
                        }
                    }
                    Err(PciHeaderError::NoDevice) => {}
                    Err(PciHeaderError::UnknownHeaderType(id)) => {
                        println!("pcid: unknown header type: {}", id);
                    }
                }
            }
        }
    }
    None
}

struct PciDev {
    bus: u8,
    dev: u8,
    func: u8,
}

unsafe fn pci_intx(dev: &PciDev, enable: bool) {
    const PCI_COMMAND: u16 = 0x04;
    const PCI_COMMAND_INTX_DISABLE: u32 = 0x400;
    let pci = Pci::new();
    let command = pci.read(dev.bus, dev.dev, dev.func, PCI_COMMAND as u8);

    let new = if enable {
        command & !PCI_COMMAND_INTX_DISABLE
    } else {
        command | PCI_COMMAND_INTX_DISABLE
    };

    if new != command {
        pci.write(dev.bus, dev.dev, dev.func, PCI_COMMAND as u8, new);
    }
}

unsafe fn pci_irq_vector(dev: &PciDev) -> u8 {
    const PCI_INTERRUPT_LINE: u8 = 0x3c;
    let pci = Pci::new();

    let mut data = pci.read(dev.bus, dev.dev, dev.func, PCI_INTERRUPT_LINE);
    println!("Original IRQ {}", (data & 0xFF) as u8);
    data = (data & 0xFFFFFF00) | 9;
    pci.write(dev.bus, dev.dev, dev.func, PCI_INTERRUPT_LINE, data);
    9
}

unsafe fn pci_msix_map_region(dev: &PciDev, num_entries: u32, msix_cap: u32) -> usize {
    let pci = Pci::new();
    const PCI_MSIX_TABLE: u32 = 4;
    const PCI_MSIX_TABLE_BIR: u32 = 0x00000007;
    const PCI_MSIX_TABLE_OFFSET: u32 = 0xfffffff8;
    const PCI_MSIX_ENTRY_SIZE: u32 = 16;

    let mut table_offset = pci.read(
        dev.bus,
        dev.dev,
        dev.func,
        (msix_cap + PCI_MSIX_TABLE) as u8,
    );

    let bir = (table_offset & PCI_MSIX_TABLE_BIR) as u8;
    table_offset &= PCI_MSIX_TABLE_OFFSET;

    println!("bir {}", bir);

    if let PciBar::Memory(shared_bar) = HEADER.get_bar(1) {
        println!("phys_addr {}", shared_bar + table_offset);
        let mapping = physmap(
            (shared_bar + table_offset) as usize,
            (num_entries * PCI_MSIX_ENTRY_SIZE) as usize,
            MAP_WRITE,
        ).expect("Failed to map physical ");
        mapping
    } else {
        panic!("1st bar of ivshmem is not memory mapped");
    }
}

unsafe fn pci_msix_program_entry(base: usize, nr: u32) {
    const PCI_MSIX_ENTRY_CTRL_MASKBIT: u32 = 1;
    const PCI_MSIX_ENTRY_VECTOR_CTRL: u32 = 12;
    const PCI_MSIX_ENTRY_SIZE: u32 = 16;
    let addr = base + (nr * PCI_MSIX_ENTRY_SIZE + PCI_MSIX_ENTRY_VECTOR_CTRL) as usize;
    let mut mask_bits = *(addr as *mut u32);

    mask_bits |= PCI_MSIX_ENTRY_CTRL_MASKBIT;

    *(addr as *mut u32) = mask_bits;
}

unsafe fn pci_init_msix(dev: &PciDev) {
    let pci = Pci::new();
    const PCI_CAP_ID_MSIX: u32 = 0x11;
    const PCI_MSIX_FLAGS: u32 = 0x2;
    const PCI_MSIX_FLAGS_MASKALL: u32 = 0x4000;
    const PCI_MSIX_FLAGS_ENABLE: u32 = 0x8000;
    const PCI_MSIX_FLAGS_QSIZE: u32 = 0x07FF;

    let msix_cap = pci_find_capability(dev, PCI_CAP_ID_MSIX);
    assert!(msix_cap != 0);
    println!("Cap {}", msix_cap);
    let mut control = pci.read(
        dev.bus,
        dev.dev,
        dev.func,
        (msix_cap + PCI_MSIX_FLAGS) as u8,
    );

    let msix_size = (control & PCI_MSIX_FLAGS_QSIZE) + 1;
    println!("msix_size {}", msix_size);
    let base = pci_msix_map_region(dev, msix_size, msix_cap);

    pci_irq_vector(dev);

    //pci_intx(dev, true);

    //Set and clear
    control &= !PCI_MSIX_FLAGS_MASKALL;
    control |= PCI_MSIX_FLAGS_ENABLE;

    pci.write(
        dev.bus,
        dev.dev,
        dev.func,
        (msix_cap + PCI_MSIX_FLAGS) as u8,
        control,
    );

    pci_msix_program_entry(base, 0);
    pci_msix_program_entry(base, 1);
}

fn pci_find_capability(dev: &PciDev, cap: u32) -> u32 {
    const PCI_CAPABILITY_LIST: u32 = 0x34;
    let pci = Pci::new();

    let mut ttl = 48;
    let mut pos = unsafe { pci.read(dev.bus, dev.dev, dev.func, PCI_CAPABILITY_LIST as u8) };
    while ttl > 0 {
        if pos < 0x40 {
            break;
        }

        pos &= !3;

        let ent = unsafe { pci.read(dev.bus, dev.dev, dev.func, pos as u8) };
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

fn print_pci_device(pci: &Pci, bus_num: u8, dev_num: u8, func_num: u8, header: PciHeader) {
    let raw_class: u8 = header.class().into();
    let mut string = format!(
        "PCI {:>02X}/{:>02X}/{:>02X} {:>04X}:{:>04X} {:>02X}.{:>02X}.{:>02X}.{:>02X} {:?}",
        bus_num,
        dev_num,
        func_num,
        header.vendor_id(),
        header.device_id(),
        raw_class,
        header.subclass(),
        header.interface(),
        header.revision(),
        header.class()
    );

    match header.class() {
        PciClass::Storage => match header.subclass() {
            0x01 => {
                string.push_str(" IDE");
            }
            0x06 => {
                string.push_str(" SATA");
            }
            _ => (),
        },
        PciClass::SerialBus => match header.subclass() {
            0x03 => match header.interface() {
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

    for (i, bar) in header.bars().iter().enumerate() {
        if !bar.is_none() {
            string.push_str(&format!(" {}={}", i, bar));
        }
    }

    string.push('\n');

    print!("{}", string);
}

#[inline]
fn init_call<T: SOS>(args: T, t: MsgType) {
    let len = args.encoded_len();
    let mut lock = PRODUCER.lock();
    let mut buffer = lock.write(IVSHRPC_HEADER_SIZE + len);
    buffer[0] = t as u8;
    NativeEndian::write_u32(&mut buffer[1..5], len as ProtoMsgLen);
    args.encode(&mut buffer[IVSHRPC_HEADER_SIZE..]);
}

pub fn init() {
    let pci = Pci::new();
    for bus in pci.buses() {
        for dev in bus.devs() {
            for func in dev.funcs() {
                let func_num = func.num;
                let header = PciHeader::from_reader(func);
                if header.is_ok() {
                    print_pci_device(&pci, bus.num, dev.num, func_num, header.unwrap());
                }
            }
        }
    }

    let dev = get_pci_header().expect("Could not find ivshmem device").1;
    unsafe {
        pci_init_msix(&dev);
    }
}

pub fn ivshrpc_fuse<'a, T: SOS>(args: T) -> EncodedValues<'a> {
    // NOTE careful here, it may not be ok to borrow from ring buffer for a long time
    init_call(args, MsgType::Fuse);

    let mut consumer = CONSUMER.lock();

    let (msgtype, len) = {
        let header = consumer.read(5);
        (
            MsgType::from(header[0]),
            NativeEndian::read_u32(&header[1..5]),
        )
    };

    let buff = consumer.read(len as usize);

    let ret = EncodedValues::from(&buff[..]);

    match msgtype {
        MsgType::Error => {
            println!("Error in call {:?}", ret.decode().collect::<Vec<Value>>());
            EncodedValues::from(ret.into_owned())
        }
        MsgType::Return => EncodedValues::from(ret.into_owned()),
        _ => panic!("Unexpected response to a fuse call"),
    }
}

fn send_interrupt() {
    // vector 1u16, device 0u16
    unsafe { *(*MMIO_BAR as *mut [u8; 4]).offset(3) = [0, 0, 0, 0] };
}

pub fn ivshrpc_cast<T: SOS>(args: T) {
    init_call(args, MsgType::Cast);
    //send_interrupt();
}
