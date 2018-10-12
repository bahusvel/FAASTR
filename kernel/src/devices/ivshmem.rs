use alloc::vec::Vec;
use byteorder::{ByteOrder, NativeEndian};
use context;
use context::{current_context, SharedContext};
use core::mem::size_of;
use core::ops::Deref;
use core::ptr::read_volatile;
use core::slice;
use core::sync::atomic::{AtomicUsize, Ordering};
use devices::pci::{pci_intx, PciBar, PciDevice};
use hashmap_core::FnvHashMap;
use ringbuf::{Consumer, Producer};
use sos::{EncodedValues, Value, SOS};
use spin::Mutex;
use syscall::flag::MAP_WRITE;
use syscall::{physmap, sys_cast, sys_fuse};

const VID: u16 = 0x1af4;
const DID: u16 = 0x1110;
const BUFFER_SIZE: usize = 4 * 1024 * 1024;
const MMIO_SIZE: usize = 256;

#[repr(packed)]
struct MsgHeader {
    msgtype: u8,
    length: u32,
    callid: u64,
}

impl MsgHeader {
    #[inline]
    fn from_slice<T: Deref<Target = [u8]>>(h: T) -> Self {
        assert!(h.len() == size_of::<MsgHeader>());
        MsgHeader {
            msgtype: h[0],
            length: NativeEndian::read_u32(&h[1..5]),
            callid: NativeEndian::read_u64(&h[5..13]),
        }
    }
}

enum MsgType {
    Cast,
    Fuse,
    Return,
    Error,
}

impl MsgType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MsgType::Cast),
            1 => Some(MsgType::Fuse),
            2 => Some(MsgType::Return),
            3 => Some(MsgType::Error),
            _ => None,
        }
    }
}

const IVSHRPC_HEADER_SIZE: usize = size_of::<ProtoMsgLen>() + 1;
type ProtoMsgLen = u32;
type CallId = u64;

static CALL_ID: AtomicUsize = AtomicUsize::new(0);

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
    static ref CALL_QUEUE: Mutex<FnvHashMap<CallId, SharedContext>> =
        Mutex::new(FnvHashMap::default());
}

pub fn isr() {
    // Clears interrupt status register.
    unsafe { read_volatile((*MMIO_BAR as *const u32).offset(1)) };
    println!("ivshmem interrupt 2 hit");
}

#[inline]
fn write_msg<T: SOS>(args: T, t: MsgType) {
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
    let current = current_context();
    {
        let callid = CALL_ID.fetch_add(1, Ordering::Relaxed);
        let mut q = CALL_QUEUE.lock();
        q.insert(callid as u64, current.clone());
    }
    write_msg(args, MsgType::Fuse);
    {
        // Atomically checks if return value is already available, if not blocks
        let mut context_lock = current.write();
        if context_lock.result.is_none() {
            context_lock.block();
        }
    }

    // NOTE yes, this will force the switch even if the return value is already available, but it is very unlikely that this is the case.
    unsafe {
        context::switch();
    }

    // Once unblocked we will return here

    // FIXME kinda stupid because all this does is put to the value back in... Can be fixed killing this context from the listener. Or special way to exit.
    return EncodedValues::from(current.write().result.take().unwrap());
}

fn fuse_proxy(callid: u64, args: EncodedValues) {
    let res = sys_fuse(args);
    match res {
        Ok(vals) => write_msg(vals, MsgType::Return),
        Err(vals) => write_msg(vals, MsgType::Error),
    }
}

fn listener() {
    let mut consumer = CONSUMER.lock();
    let header = {
        let mut header = consumer
            .try_read(size_of::<MsgHeader>(), 1000)
            .map(MsgHeader::from_slice);
        if header.is_none() {
            // TODO set not listening
            // Checking one last time to avoid race condition
            header = consumer
                .try_read(size_of::<MsgHeader>(), 1)
                .map(MsgHeader::from_slice);
            if header.is_none() {
                return;
            }
        }
        header.unwrap()
    };

    let buff = consumer.read(header.length as usize);
    let ret = EncodedValues::from(&buff[..]);
    match MsgType::from_u8(header.msgtype) {
        Some(MsgType::Error) | Some(MsgType::Return) => {
            // Deliver result to context
            let q = CALL_QUEUE.lock();
            let context = q.get(&header.callid);
            if context.is_none() {
                panic!("Received result for unknown context id {}", header.callid);
            }
            let context = context.unwrap();
            let mut context_lock = context.write();
            context_lock.result = Some(ret.into_owned());
            context_lock.unblock();
        }
        Some(MsgType::Fuse) => {}
        Some(MsgType::Cast) => {
            let res = sys_cast(ret);
            if res.is_err() {
                write_msg(res.unwrap_err(), MsgType::Error);
            }
        }
        None => panic!("Unexpected response to a fuse call"),
    }
}

fn send_interrupt() {
    // vector 1u16, device 0u16
    unsafe { *(*MMIO_BAR as *mut [u8; 4]).offset(3) = [0, 0, 0, 0] };
}

pub fn ivshrpc_cast<T: SOS>(args: T) {
    write_msg(args, MsgType::Cast);
    //send_interrupt();
}
