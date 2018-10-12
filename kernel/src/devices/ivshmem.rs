use alloc::vec::Vec;
use byteorder::{ByteOrder, NativeEndian};
use core::mem;
use core::ptr::read_volatile;
use core::slice;
use devices::pci::{pci_intx, PciBar, PciDevice};
use ringbuf::{Consumer, Producer};
use sos::{EncodedValues, Value, SOS};
use spin::Mutex;
use syscall::flag::MAP_WRITE;
use syscall::physmap;

const VID: u16 = 0x1af4;
const DID: u16 = 0x1110;
const BUFFER_SIZE: usize = 4 * 1024 * 1024;
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
    static ref DEVICE: PciDevice = PciDevice::find_by_id(VID, DID)
        .pop()
        .expect("Could not find a compatible ivshmem device!");
    static ref BUFFER_PTR: usize = {
        if let PciBar::Memory(shared_bar) = DEVICE.header.get_bar(2) {
            let mapping = physmap(shared_bar as usize, BUFFER_SIZE, MAP_WRITE)
                .expect("Failed to map physical ");

            println!("ivshrpc found and initialised");
            mapping
        } else {
            panic!("2nd bar of ivshmem is not memory mapped");
        }
    };
    static ref MMIO_BAR: usize = {
        if let PciBar::Memory(shared_bar) = DEVICE.header.get_bar(0) {
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

pub fn isr() {
    // Clears interrupt status register.
    unsafe { read_volatile((*MMIO_BAR as *const u32).offset(1)) };
    println!("ivshmem interrupt 2 hit");
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
    unsafe {
        while *(*MMIO_BAR as *const i32).offset(2) < 0 {}
        println!("My id {}", *(*MMIO_BAR as *const i32).offset(2));
        pci_intx(&DEVICE, true);
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
