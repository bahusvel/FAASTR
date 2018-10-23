#![feature(try_from)]
#[macro_use]
extern crate lazy_static;
extern crate byteorder;
extern crate either;
extern crate fnv;
extern crate memmap;
extern crate ringbuf;
#[macro_use]
extern crate sos;
extern crate ivshrpc;
extern crate nix;
extern crate spin;
extern crate spmc;
extern crate threadpool;

mod dispatch;

use byteorder::{ByteOrder, NativeEndian};
use dispatch::dispatch;

use fnv::FnvHashMap;
use ivshrpc::*;
use memmap::MmapMut;
use nix::fcntl;
use nix::sys::socket::{recvmsg, CmsgSpace, ControlMessage, MsgFlags, RecvMsg};
use nix::sys::uio::IoVec;
use nix::unistd;
use ringbuf::{Consumer, Header, Producer};
use sos::{EncodedValues, OwnedEncodedValues, SOS};

use std::fs::{remove_file, File};
use std::io::Read;
use std::ops::Deref;
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::{str, thread, time};
use threadpool::ThreadPool;

const IVSH_PATH: &str = "/dev/shm/ivshmem";
const IVSH_SERVER: &str = "ivshmem-server";
const IVSH_SERVER_SOCKET: &str = "/tmp/ivshmem_socket";
const NUM_WORKERS: usize = 8;

lazy_static! {
    static ref NOTIFY_FD: spin::Mutex<RawFd> = spin::Mutex::new(-1);
    static ref PRODUCER: spin::Mutex<Option<Producer<'static>>> = spin::Mutex::new(None);
    static ref CALL_QUEUE: spin::Mutex<FnvHashMap<CallId, CallResult>> =
        spin::Mutex::new(FnvHashMap::default());
    static ref THREAD_POOL: spin::Mutex<ThreadPool> =
        spin::Mutex::new(ThreadPool::new(NUM_WORKERS));
}

type CallResult = Arc<(
    Mutex<Option<Result<OwnedEncodedValues, OwnedEncodedValues>>>,
    Condvar,
)>;

static CALL_ID: AtomicUsize = AtomicUsize::new(0);
static mut CONSUMER: Option<Consumer> = None;

fn get_fd(msg: &RecvMsg) -> RawFd {
    for cmsg in msg.cmsgs() {
        if let ControlMessage::ScmRights(fd) = cmsg {
            assert_eq!(fd.len(), 1);
            return fd[0];
        } else {
            panic!("unexpected cmsg");
        }
    }
    return -1;
}

fn send_interrupt(fd: RawFd) {
    let buf: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
    let res = unistd::write(fd, &buf[..]);
    if res.is_err() {
        panic!("{:?}", res);
    }
}

#[inline]
fn write_msg<T: SOS>(args: T, mut header: MsgHeader) {
    header.length = args.encoded_len() as u32;
    let mut lock = PRODUCER.lock();
    let mut buffer = lock
        .as_mut()
        .unwrap()
        .write(IVSHRPC_HEADER_SIZE + header.length as usize);
    buffer[..IVSHRPC_HEADER_SIZE].copy_from_slice(header.to_slice());
    args.encode(&mut buffer[IVSHRPC_HEADER_SIZE..]);

    // TODO, check if listening
    let fd = NOTIFY_FD.lock();
    assert!(*fd != -1);
    send_interrupt(*fd);
}

fn listen_for_clients(fd: RawFd, myid: u16) -> Result<(), nix::Error> {
    let mut buf: [u8; 8] = [0; 8];
    let iov = [IoVec::from_mut_slice(&mut buf[..])];
    let mut cmsg: CmsgSpace<RawFd> = CmsgSpace::new();

    loop {
        let msg = recvmsg(fd, &iov, Some(&mut cmsg), MsgFlags::empty())?;
        let rcvid = NativeEndian::read_i64(iov[0].as_slice()) as u16;
        assert!(rcvid != myid); // This means that the server was configured for more vectors
        let fd = get_fd(&msg);

        // When fd is not present it means that a client was disconnected, otherwise connected
        if fd == -1 {
            println!("Client id {} disconnected", rcvid)
        } else {
            println!("Client id {} connected", rcvid);
            *NOTIFY_FD.lock() = fd;
        }
    }
}

fn dispatch_thread(myfd: RawFd) {
    let flags = fcntl::fcntl(myfd, fcntl::FcntlArg::F_GETFL).unwrap();
    let mut oflags = fcntl::OFlag::from_bits(flags).unwrap();
    oflags.remove(fcntl::OFlag::O_NONBLOCK);
    let mut stream = unsafe { File::from_raw_fd(myfd) };

    loop {
        fcntl::fcntl(myfd, fcntl::FcntlArg::F_SETFL(oflags)).expect("Failed to make myfd blocking");
        let mut buf: [u8; 8] = [0; 8];
        stream
            .read_exact(&mut buf[..])
            .expect("Failed to read on my own fd");
        println!("Received an interrupt!");
        listener()
    }
}

pub fn ivshrpc_cast<T: SOS>(args: T) {
    let callid = CALL_ID.fetch_add(1, Ordering::Relaxed);
    write_msg(args, MsgHeader::new(MsgType::Cast, callid as u64));
}

pub fn ivshrpc_fuse<'a, T: SOS>(args: T) -> Result<OwnedEncodedValues, OwnedEncodedValues> {
    //-> EncodedValues<'a> {}
    let callid = CALL_ID.fetch_add(1, Ordering::Relaxed);
    write_msg(args, MsgHeader::new(MsgType::Fuse, callid as u64));

    let entry = CALL_QUEUE
        .lock()
        .entry(callid as u64)
        .or_insert(Arc::new((Mutex::new(None), Condvar::new())))
        .clone();

    let (lock, var) = entry.deref();
    let mut res = lock.lock().unwrap();
    while res.is_none() {
        res = var.wait(res).unwrap();
    }

    return res.take().unwrap();
}

fn listener() {
    let consumer = unsafe { CONSUMER.as_mut().unwrap() };
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

        unsafe {
            println!("Len: {}, {:?}", header.length, header.to_slice());
        }

        let buff = consumer.read(header.length as usize);
        //println!("Bytes: {:?}", &buff[..]);
        let values = EncodedValues::from(&buff[..]);
        let callid = header.callid;
        let msgtype = MsgType::from_u8(header.msgtype).expect("Unexpected msgtype");

        match msgtype {
            MsgType::Fuse | MsgType::Cast => {
                let pool = THREAD_POOL.lock();
                let owned_values = values.into_owned();
                pool.execute(move || {
                    let result = dispatch(owned_values, msgtype == MsgType::Fuse);
                    match result {
                        Ok(val) => if msgtype == MsgType::Fuse {
                            write_msg(
                                EncodedValues::from(val),
                                MsgHeader::new(MsgType::Error, callid),
                            );
                        },
                        Err(err) => write_msg(err, MsgHeader::new(MsgType::Error, callid)),
                    }
                });
            }
            MsgType::Error | MsgType::Return => {
                let entry = CALL_QUEUE
                    .lock()
                    .remove(&callid)
                    .expect("Received return for unqueued call");
                let (lock, var) = entry.deref();
                *lock.lock().unwrap() = Some(if msgtype == MsgType::Error {
                    Err(values.into_owned())
                } else {
                    Ok(values.into_owned())
                });
                var.notify_all();
            }
        };
    }
}

fn ivsh_server_init(fd: RawFd) -> Result<(u16, RawFd, RawFd), nix::Error> {
    let mut buf: [u8; 8] = [0; 8];
    let iov = [IoVec::from_mut_slice(&mut buf[..])];
    let mut cmsg: CmsgSpace<RawFd> = CmsgSpace::new();

    // Protocol version
    recvmsg::<()>(fd, &iov, None, MsgFlags::empty())?;
    if NativeEndian::read_i64(iov[0].as_slice()) != 0 {
        panic!("ivsh-server reported protocol version != 0");
    }
    // My id
    recvmsg::<()>(fd, &iov, None, MsgFlags::empty())?;
    let id: u16 = NativeEndian::read_i64(iov[0].as_slice()) as u16;

    // Fd that points to memory
    let memfd = {
        let msg = recvmsg(fd, &iov, Some(&mut cmsg), MsgFlags::empty())?;
        if NativeEndian::read_i64(iov[0].as_slice()) != -1 {
            panic!("ivsh-server did not send -1");
        }
        let fd = get_fd(&msg);
        assert!(fd != -1);
        fd
    };

    loop {
        let msg = recvmsg(fd, &iov, Some(&mut cmsg), MsgFlags::empty())?;
        let rcvid = NativeEndian::read_i64(iov[0].as_slice()) as u16;
        // This is connection setup
        let fd = get_fd(&msg);
        assert!(fd != -1);
        if rcvid == id {
            return Ok((id, memfd, fd));
        }
        *NOTIFY_FD.lock() = fd;
    }
}

fn main() {
    let _ = remove_file(IVSH_SERVER_SOCKET);

    let _cmd = Command::new(IVSH_SERVER)
        .args(&[
            "-F",
            "-m",
            "/dev/shm",
            "-M",
            "ivshmem",
            "-l",
            &BUFFER_SIZE.to_string(),
            "-n",
            "1",
            "-S",
            IVSH_SERVER_SOCKET,
        ]).spawn()
        .expect("Failed to start ivshmem-server");

    let timeout = time::SystemTime::now() + time::Duration::from_secs(10);

    let mut connfd = 0;

    while time::SystemTime::now() < timeout {
        let conn = UnixStream::connect(IVSH_SERVER_SOCKET);
        if conn.is_err() {
            continue;
        }
        connfd = conn.unwrap().into_raw_fd();
        break;
    }
    assert!(connfd != 0);

    let (myid, memfd, myfd) =
        ivsh_server_init(connfd).expect("Failed to connect to ivshmem-server");

    let file = unsafe { File::from_raw_fd(memfd) };
    let mut mapping = unsafe { MmapMut::map_mut(&file).expect("Failed to map ivshmem") };
    let (viho, vohi) = mapping.split_at_mut(BUFFER_SIZE / 2);

    // It is host's responsibility to initliase the headers, anything that was there previously will be wiped.
    unsafe {
        Header::new_inline_at(viho);
        Header::new_inline_at(vohi);
        // This is used to escape mapping lifetime.
        *PRODUCER.lock() = Some(Producer::from_slice(&mut *(viho as *mut [u8])));
        CONSUMER = Some(Consumer::from_slice(&mut *(vohi as *mut [u8])));
    }

    thread::spawn(move || listen_for_clients(connfd, myid));

    dispatch_thread(myfd);
}
