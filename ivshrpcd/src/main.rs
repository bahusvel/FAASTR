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
extern crate nix;

use byteorder::{ByteOrder, NativeEndian};
use either::Either;
use fnv::FnvHashMap;
use memmap::MmapMut;
use nix::fcntl;
use nix::sys::socket::{recvmsg, CmsgSpace, ControlMessage, MsgFlags, RecvMsg};
use nix::sys::uio::IoVec;
use nix::unistd;
use ringbuf::{Consumer, Header, Producer};
use sos::{
    DecodeIter, EncodedValues, Function, JustError, OwnedEncodedValues, OwnedFunction, Value, SOS,
};
use std::convert::TryInto;
use std::fs::{remove_file, File};
use std::io::Read;
use std::mem;
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::str;
use std::sync::RwLock;
use std::thread;
use std::time;

const IVSH_PATH: &str = "/dev/shm/ivshmem";
const IVSH_SERVER: &str = "ivshmem-server";
const IVSH_SERVER_SOCKET: &str = "/tmp/ivshmem_socket";
const IVSH_SIZE: usize = 1024 * 1024;

type ProtoMsgLen = u32;

const IVSHRPC_HEADER_SIZE: usize = mem::size_of::<ProtoMsgLen>() + 1;

type FuseFunc = fn(args: DecodeIter) -> OwnedEncodedValues;
type CastFunc = fn(args: DecodeIter);

#[derive(PartialEq)]
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

lazy_static! {
    static ref FUNC_TABLE: RwLock<FnvHashMap<OwnedFunction, Either<CastFunc, FuseFunc>>> = {
        let mut map = FnvHashMap::default();
        map.insert(
            OwnedFunction::new("host", "hello"),
            Either::Left(hello as CastFunc),
        );
        map.insert(
            OwnedFunction::new("host", "hello_fuse"),
            Either::Right(hello_fuse as FuseFunc),
        );
        RwLock::new(map)
    };
    static ref NOTIFY_FDS: RwLock<Vec<RawFd>> = RwLock::new(Vec::new());
}

fn hello(args: DecodeIter) {
    println!("Hello from host {:?}", args.collect::<Vec<Value>>())
}

fn hello_fuse(args: DecodeIter) -> OwnedEncodedValues {
    let msg = format!("Hello from host {:?}", args.collect::<Vec<Value>>());
    EncodedValues::from(sos![msg.as_str()]).into_owned()
}

fn dispatch<'a, 'b>(
    args: EncodedValues<'a>,
    fuse: bool,
) -> Result<OwnedEncodedValues, JustError<'static>> {
    let mut iter = args.decode();
    let function: Function = iter
        .next()
        .ok_or(JustError::new("Not enough arguments"))?
        .try_into()
        .map_err(|e| JustError::new(e))?;

    let lock = FUNC_TABLE.read().expect("Poisoned lock");

    let func = lock
        .get(&OwnedFunction::from(function)) // TODO avoid this stupid copying operation
        .ok_or(JustError::new("No such function"))?;

    if fuse {
        Ok(func.right().ok_or(JustError::new(
            "Attempt to fuse to a cast only function",
        ))?(iter))
    } else {
        func.left()
            .ok_or(JustError::new("Attempt to cast to a fuse only function"))?(iter);
        Ok(EncodedValues::from(sos!()).into_owned())
    }
}

fn handle_fuse(buff: &[u8], producer: &mut Producer) -> Result<(), JustError<'static>> {
    let result = dispatch(EncodedValues::from(&buff[..]), true)?;

    encode_msg(producer, result.len() as u32, MsgType::Return, |buff| {
        buff.copy_from_slice(&result);
    });

    Ok(())
}

fn handle_cast(buff: &[u8]) -> Result<(), JustError<'static>> {
    dispatch(EncodedValues::from(&buff[..]), false)?;

    Ok(())
}

fn encode_msg<F>(producer: &mut Producer, len: u32, t: MsgType, f: F)
where
    F: Fn(&mut [u8]),
{
    let mut buff = producer.write(5 + len as usize);
    buff[0] = t as u8;
    NativeEndian::write_u32(&mut buff[1..5], len);
    f(&mut buff[5..])
}

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

fn ivsh_server_init(fd: RawFd) -> Result<(u16, RawFd, Vec<RawFd>, RawFd), nix::Error> {
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

    // Existing clients and my single vector
    let (existing, myfd) = {
        let mut fds: Vec<RawFd> = Vec::new();
        let mut myfd;
        loop {
            let msg = recvmsg(fd, &iov, Some(&mut cmsg), MsgFlags::empty())?;
            let rcvid = NativeEndian::read_i64(iov[0].as_slice()) as u16;
            // This is connection setup
            let fd = get_fd(&msg);
            assert!(fd != -1);
            if rcvid == id {
                myfd = fd;
                break;
            }
            fds.push(fd)
        }
        assert!(myfd != -1);
        (fds, myfd)
    };

    Ok((id, memfd, existing, myfd))
}

fn interrupt_client(fd: RawFd) {
    let buf: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
    loop {
        thread::sleep(time::Duration::from_secs(5));
        unistd::write(fd, &buf[..]).expect("Failed to notify client");
        println!("Interrupt sent");
    }
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
            thread::spawn(move || {
                interrupt_client(fd);
            });
            NOTIFY_FDS.write().unwrap().push(fd);
        }
    }
}

fn dispatch_thread(myfd: RawFd) {
    let flags = fcntl::fcntl(myfd, fcntl::FcntlArg::F_GETFL).unwrap();
    let mut oflags = fcntl::OFlag::from_bits(flags).unwrap();
    oflags.remove(fcntl::OFlag::O_NONBLOCK);
    fcntl::fcntl(myfd, fcntl::FcntlArg::F_SETFL(oflags)).expect("Failed to make myfd blocking");

    let mut stream = unsafe { File::from_raw_fd(myfd) };
    loop {
        let mut buf: [u8; 8] = [0; 8];
        stream
            .read_exact(&mut buf[..])
            .expect("Failed to read on my own fd");
        println!("Received an interrupt!");
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
            &IVSH_SIZE.to_string(),
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

    let (myid, memfd, existing, myfd) =
        ivsh_server_init(connfd).expect("Failed to connect to ivshmem-server");

    /*
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(IVSH_PATH)
        .expect("Failed to open ivshmem");

    file.set_len(IVSH_SIZE as u64)
        .expect("Failed to set file size");

    */

    let file = unsafe { File::from_raw_fd(memfd) };
    let mut mapping = unsafe { MmapMut::map_mut(&file).expect("Failed to map ivshmem") };
    let (viho, vohi) = mapping.split_at_mut(IVSH_SIZE / 2);

    // It is host's responsibility to initliase the headers, anything that was there previously will be wiped.
    unsafe {
        Header::new_inline_at(viho);
        Header::new_inline_at(vohi);
    }

    // These are safe-ish, because we have initialised them just above.
    let mut producer = unsafe { Producer::from_slice(viho) };
    let mut consumer = unsafe { Consumer::from_slice(vohi) };

    NOTIFY_FDS.write().unwrap().extend_from_slice(&existing);

    thread::spawn(move || listen_for_clients(connfd, myid));

    thread::spawn(move || dispatch_thread(myfd));

    loop {
        let (msgtype, len) = {
            let header = consumer.read(5);
            (
                MsgType::from(header[0]),
                NativeEndian::read_u32(&header[1..5]),
            )
        };

        println!("Len: {}", len);
        let buff = consumer.read(len as usize);
        println!("Bytes: {:?}", &buff[..]);

        let error = match msgtype {
            MsgType::Fuse => handle_fuse(&buff, &mut producer),
            MsgType::Cast => handle_cast(&buff),
            _ => panic!("Not Implemented"),
        };

        if error.is_err() {
            let err = error.unwrap_err();
            encode_msg(
                &mut producer,
                err.encoded_len() as u32,
                MsgType::Error,
                |buff| {
                    err.encode(buff);
                },
            );
        }
    }
}
