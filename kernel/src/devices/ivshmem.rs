use context;
use context::{contexts_mut, current_context, SharedContext, Status};
use core::ptr::read_volatile;
use core::slice;
use core::sync::atomic::{AtomicUsize, Ordering};
use devices::pci::{pci_intx, PciBar, PciDevice};
use hashmap_core::FnvHashMap;
use interrupt;
use ivshrpc::*;
use ringbuf::{Consumer, Producer};
use sos::{EncodedValues, EncodedValuesPtr, SOS};
use spin::Mutex;
use syscall::flag::MAP_WRITE;
use syscall::{exit, physmap, sys_cast, sys_fuse};

const VID: u16 = 0x1af4;
const DID: u16 = 0x1110;

const MMIO_SIZE: usize = 256;

static CALL_ID: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    static ref DEVICE: PciDevice = PciDevice::find_by_id(VID, DID)
        .pop()
        .expect("Could not find a compatible ivshmem device!");
    static ref BUFFER_PTR: usize = {
        if let PciBar::Memory(shared_bar) = DEVICE.header.get_bar(2) {
            let mapping = physmap(shared_bar as usize, BUFFER_SIZE, MAP_WRITE)
                .expect("Failed to map physical ");
            //println!("ivshrpc found and initialised");
            mapping
        } else {
            panic!("2nd bar of ivshmem is not memory mapped");
        }
    };
    static ref MMIO_BAR: usize = {
        if let PciBar::Memory(shared_bar) = DEVICE.header.get_bar(0) {
            let mapping = physmap(shared_bar as usize, MMIO_SIZE, MAP_WRITE)
                .expect("Failed to map physical ");
            //println!("ivshrpc-mmio found and initialised");
            // What does this do?
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

#[inline]
fn write_msg<T: SOS>(args: T, mut header: MsgHeader) {
    header.length = args.encoded_len() as u32;
    let mut lock = PRODUCER.lock();
    let mut buffer = lock.write(IVSHRPC_HEADER_SIZE + header.length as usize);
    buffer[..IVSHRPC_HEADER_SIZE].copy_from_slice(header.to_slice());
    args.encode(&mut buffer[IVSHRPC_HEADER_SIZE..]);

    // TODO, check if listening
    send_interrupt();
}

pub fn init() {
    unsafe {
        // Poll until interrupts are available
        while *(*MMIO_BAR as *const i32).offset(2) < 0 {}
        println!("IVSHRPC_ID {}", *(*MMIO_BAR as *const i32).offset(2));
        pci_intx(&DEVICE, true);
    }
}

pub extern "C" fn fuse_proxy(values: EncodedValuesPtr) {
    println!("Fuse OK 0x{:x}", values as usize);

    let values = unsafe { EncodedValues::from(slice::from_raw_parts(values, 4096)) };

    let res = sys_fuse(values);
    match res {
        // TODO the callid is not zero, I need to pass it through values above.
        Ok(vals) => write_msg(vals, MsgHeader::new(MsgType::Return, 0)),
        Err(vals) => write_msg(vals, MsgHeader::new(MsgType::Error, 0)),
    }

    exit(0);
}

pub fn isr() {
    unsafe { read_volatile((*MMIO_BAR as *const u32).offset(1)) };
    println!("ivshmem interrupt 2 hit");

    let mut consumer = CONSUMER.lock();
    loop {
        let header = {
            let mut header = consumer
                .try_read(IVSHRPC_HEADER_SIZE, 1000)
                .map(MsgHeader::from_slice);
            if header.is_none() {
                // TODO set not listening
                // Checking one last time to avoid race condition
                header = consumer
                    .try_read(IVSHRPC_HEADER_SIZE, 1)
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
                let context = CALL_QUEUE.lock().remove(&header.callid);
                if context.is_none() {
                    panic!("Received result for unknown context id {}", header.callid);
                }
                let context = context.unwrap();
                let mut context_lock = context.write();
                context_lock.result = Some(ret.into_owned());
                context_lock.unblock();
            }
            Some(MsgType::Fuse) => {
                println!("Proxy ptr: {:x}", fuse_proxy as usize);
                context::cast_ptr((context::KERNEL_MODULE.clone(), fuse_proxy as usize), &ret)
                    .expect("Failed to cast proxy");
            }
            Some(MsgType::Cast) => {
                let res = sys_cast(ret);
                if res.is_err() {
                    write_msg(
                        res.unwrap_err(),
                        MsgHeader::new(MsgType::Error, header.callid),
                    );
                }
            }
            None => panic!("Unexpected response to a fuse call"),
        }
    }
}

fn send_interrupt() {
    // vector 1u16, device 0u16
    unsafe { *(*MMIO_BAR as *mut [u8; 4]).offset(3) = [0, 0, 0, 0] };
}

pub fn ivshrpc_cast<T: SOS>(args: T) {
    let callid = CALL_ID.fetch_add(1, Ordering::Relaxed);
    write_msg(args, MsgHeader::new(MsgType::Cast, callid as u64));
}

pub fn ivshrpc_fuse<'a, T: SOS>(args: T) -> EncodedValues<'a> {
    let callid = CALL_ID.fetch_add(1, Ordering::Relaxed);
    let current = current_context();
    {
        let mut q = CALL_QUEUE.lock();
        q.insert(callid as u64, current.clone());
    }
    write_msg(args, MsgHeader::new(MsgType::Fuse, callid as u64));
    {
        // Atomically checks if return value is already available, if not blocks
        let mut context_lock = current.write();
        if context_lock.result.is_none() {
            context_lock.status = Status::Blocked;
        }
    }

    // NOTE yes, this will force the switch even if the return value is already available, but it is very unlikely that this is the case.

    while current.read().status == Status::Blocked {
        unsafe {
            interrupt::disable();
            if context::switch() {
                interrupt::enable_and_nop();
            } else {
                // No other task to switch to, halt and wait for interrupts.
                interrupt::enable_and_halt();
            }
        }
    }

    // FIXME kinda stupid because all this does is put to the value back in... Can be fixed killing this context from the listener. Or special way to exit.
    return EncodedValues::from(
        current
            .write()
            .result
            .take()
            .expect("This shouldn't be empty."),
    );
}
