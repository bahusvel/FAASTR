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

mod dispatch;

use byteorder::{ByteOrder, NativeEndian};
use dispatch::dispatch;

use ivshrpc::*;
use memmap::MmapMut;
use nix::fcntl;
use nix::sys::socket::{recvmsg, CmsgSpace, ControlMessage, MsgFlags, RecvMsg};
use nix::sys::uio::IoVec;
use nix::unistd;
use ringbuf::{Consumer, Header, Producer};
use sos::{EncodedValues, JustError, SOS};

use std::fs::{remove_file, File};
use std::io::Read;
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time;

const IVSH_PATH: &str = "/dev/shm/ivshmem";
const IVSH_SERVER: &str = "ivshmem-server";
const IVSH_SERVER_SOCKET: &str = "/tmp/ivshmem_socket";

lazy_static! {
    static ref NOTIFY_FD: Mutex<RawFd> = Mutex::new(-1);
    static ref PRODUCER: Mutex<Option<Producer<'static>>> = Mutex::new(None);
}

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

fn handle_fuse(values: EncodedValues, callid: CallId) -> Result<(), JustError<'static>> {
    let result = dispatch(values, true)?;

    write_msg(
        EncodedValues::from(result),
        MsgHeader::new(MsgType::Return, callid),
    );

    Ok(())
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
    let mut lock = PRODUCER.lock().unwrap();
    let mut buffer = lock
        .as_mut()
        .unwrap()
        .write(IVSHRPC_HEADER_SIZE + header.length as usize);
    buffer[..IVSHRPC_HEADER_SIZE].copy_from_slice(header.to_slice());
    args.encode(&mut buffer[IVSHRPC_HEADER_SIZE..]);

    // TODO, check if listening
    let fd = NOTIFY_FD.lock().unwrap();
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
            *NOTIFY_FD.lock().unwrap() = fd;
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

pub fn ivshrpc_fuse<'a, T: SOS>(args: T) {
    //-> EncodedValues<'a> {}
    let callid = CALL_ID.fetch_add(1, Ordering::Relaxed);
    write_msg(args, MsgHeader::new(MsgType::Fuse, callid as u64));
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

        let error = match MsgType::from_u8(header.msgtype).expect("Unexpected msgtype") {
            MsgType::Fuse => handle_fuse(values, header.callid),
            MsgType::Cast => dispatch(values, false).map(|_| ()),
            _ => panic!("Not Implemented"),
        };

        if error.is_err() {
            write_msg(
                error.unwrap_err(),
                MsgHeader::new(MsgType::Error, header.callid),
            );
        }
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
        *NOTIFY_FD.lock().unwrap() = fd;
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
        *PRODUCER.lock().unwrap() = Some(Producer::from_slice(&mut *(viho as *mut [u8])));
        CONSUMER = Some(Consumer::from_slice(&mut *(vohi as *mut [u8])));
    }

    thread::spawn(move || listen_for_clients(connfd, myid));

    dispatch_thread(myfd);
}
