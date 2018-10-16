#![no_std]
extern crate byteorder;

use byteorder::{ByteOrder, NativeEndian};
use core::mem::size_of;
use core::ops::Deref;
use core::slice;

pub const BUFFER_SIZE: usize = 4 * 1024 * 1024;
pub type CallId = u64;

pub const IVSHRPC_HEADER_SIZE: usize = size_of::<MsgHeader>();

#[repr(packed)]
pub struct MsgHeader {
    pub msgtype: u8,
    pub length: u32,
    pub callid: CallId,
}

impl MsgHeader {
    pub fn new(msgtype: MsgType, callid: CallId) -> Self {
        MsgHeader {
            msgtype: msgtype as u8,
            length: 0,
            callid,
        }
    }
    #[inline]
    pub fn from_slice<T: Deref<Target = [u8]>>(h: T) -> Self {
        assert!(h.len() == size_of::<MsgHeader>());
        MsgHeader {
            msgtype: h[0],
            length: NativeEndian::read_u32(&h[1..5]),
            callid: NativeEndian::read_u64(&h[5..13]),
        }
    }
    pub fn to_slice(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(
                self as *const MsgHeader as *const u8,
                size_of::<MsgHeader>(),
            )
        }
    }
}

pub enum MsgType {
    Cast,
    Fuse,
    Return,
    Error,
}

impl MsgType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MsgType::Cast),
            1 => Some(MsgType::Fuse),
            2 => Some(MsgType::Return),
            3 => Some(MsgType::Error),
            _ => None,
        }
    }
}
