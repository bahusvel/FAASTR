use byteorder::{ByteOrder, NativeEndian};
use core::mem;
use core::slice;
use devices::pci::{Pci, PciBar, PciHeader, PciHeaderError};
use ringbuf::{Consumer, Producer};
use sos::{EncodedValues, JustError, SOS};
use spin::Mutex;
use syscall::flag::MAP_WRITE;
use syscall::physmap;

const VID: u16 = 0x1af4;
const DID: u16 = 0x1110;
const BUFFER_SIZE: usize = 1024 * 1024;

type ProtoMsgLen = u32;

lazy_static! {
    static ref BUFFER_PTR: usize = {
        let device_header = get_pci_header().expect("Could not find a compatible ivshmem device!");
        if let PciBar::Memory(shared_bar) = device_header.get_bar(2) {
            let mapping = physmap(shared_bar as usize, BUFFER_SIZE, MAP_WRITE)
                .expect("Failed to map physical ");

            println!("ivshrpc found and initialised");
            mapping
        } else {
            panic!("2nd bar of ivshmem is not memory mapped");
        }
    };
    static ref CONSUMER: Consumer<'static> = unsafe {
        let buffer = slice::from_raw_parts_mut(*BUFFER_PTR as *mut u8, BUFFER_SIZE / 2);
        Consumer::from_slice(buffer)
    };
    static ref PRODUCER: Mutex<Producer<'static>> = unsafe {
        let buffer = slice::from_raw_parts_mut(
            (*BUFFER_PTR as *mut u8).offset((BUFFER_SIZE / 2) as isize),
            BUFFER_SIZE / 2,
        );
        Mutex::new(Producer::from_slice(buffer))
    };
}

fn get_pci_header() -> Option<PciHeader> {
    let pci = Pci::new();
    for bus in pci.buses() {
        for dev in bus.devs() {
            for func in dev.funcs() {
                match PciHeader::from_reader(func) {
                    Ok(header) => {
                        if header.vendor_id() == VID
                            && header.device_id() == DID
                            && header.revision() == 1
                        {
                            return Some(header);
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

pub fn ivsrpc_fuse<'a, T: SOS>(_args: T) -> Result<EncodedValues<'a>, JustError<'static>> {
    // NOTE careful here, it may not be ok to borrow from ring buffer for a long time
    Err(JustError::new("Not implemented"))
}

pub fn ivshrpc_cast<T: SOS>(args: T) {
    let len = args.encoded_len();
    let mut lock = PRODUCER.lock();
    let mut buffer = lock.write(mem::size_of::<ProtoMsgLen>() + len);
    NativeEndian::write_u32(
        &mut buffer[..mem::size_of::<ProtoMsgLen>()],
        len as ProtoMsgLen,
    );
    args.encode(&mut buffer[mem::size_of::<ProtoMsgLen>()..]);
}

pub fn send_call() {
    let hello = "Hello from FaaSTR-MicroKernel";
    let mut lock = PRODUCER.lock();
    lock.write(hello.len()).copy_from_slice(hello.as_bytes());
}
