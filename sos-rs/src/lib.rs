#![no_std]
#![feature(try_from)]
#![feature(alloc)]
#![allow(dead_code)]
extern crate alloc;
extern crate byteorder;

use self::byteorder::{ByteOrder, NativeEndian};
#[cfg(feature = "alloc")]
use alloc::borrow::Cow;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::convert::TryInto;
use core::fmt::Debug;
use core::ops::Deref;
use core::str::from_utf8;

const NULL: [u8; 1] = [0];
const WRONG_TYPE: &str = "Received value is of incorrect type";

#[derive(PartialEq, Clone)]
pub struct Function<'a> {
    pub module: &'a str,
    pub name: &'a str,
}

impl<'a> Debug for Function<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
}

pub trait SOS {
    fn encode(&self, &mut [u8]) -> usize;
    fn encoded_len(&self) -> usize;
}

#[derive(Debug, Clone)]
pub enum Value<'a> {
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Error(&'a str),
    String(&'a str),
    Opaque(&'a [u8]),
    Function(Function<'a>),
    EmbeddedOut(DecodeIter<'a>),
    EmbeddedIn(ReferencedValues<'a>),
}

#[derive(Debug)]
pub struct JustError<'a>([Value<'a>; 1]);

impl<'a> Deref for JustError<'a> {
    type Target = [Value<'a>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> JustError<'a> {
    pub fn new(error: &'a str) -> Self {
        JustError([Value::Error(error); 1])
    }
}

impl<'a> SOS for JustError<'a> {
    fn encoded_len(&self) -> usize {
        ReferencedValues(&self.0).encoded_len()
    }

    fn encode(&self, buf: &mut [u8]) -> usize {
        ReferencedValues(&self.0).encode(buf)
    }
}

#[derive(Debug, Clone)]
pub struct ReferencedValues<'a>(pub &'a [Value<'a>]);

impl<'a> SOS for ReferencedValues<'a> {
    fn encode(&self, buf: &mut [u8]) -> usize {
        encode_sos(buf, self.0)
    }

    fn encoded_len(&self) -> usize {
        encoded_len(self.0)
    }
}

#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct EncodedValues<'a>(Cow<'a, [u8]>);

impl<'a> Deref for EncodedValues<'a> {
    type Target = Cow<'a, [u8]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "alloc")]
impl<'a> EncodedValues<'a> {
    pub fn decode(&self) -> DecodeIter {
        decode_sos(&self)
    }
}

#[cfg(feature = "alloc")]
impl<'a> From<&'a [u8]> for EncodedValues<'a> {
    fn from(buf: &'a [u8]) -> Self {
        EncodedValues(Cow::Borrowed(buf))
    }
}

#[cfg(feature = "alloc")]
impl<'a> From<Vec<u8>> for EncodedValues<'a> {
    fn from(buf: Vec<u8>) -> Self {
        EncodedValues(Cow::Owned(buf))
    }
}

#[cfg(feature = "alloc")]
impl<'a, 'b> From<ReferencedValues<'b>> for EncodedValues<'a> {
    fn from(vals: ReferencedValues<'b>) -> Self {
        let want = vals.encoded_len();
        let mut vec = Vec::with_capacity(want);
        unsafe { vec.set_len(want) };
        vals.encode(&mut vec);
        EncodedValues(Cow::Owned(vec))
    }
}

#[cfg(feature = "alloc")]
impl<'a> SOS for EncodedValues<'a> {
    fn encode(&self, buf: &mut [u8]) -> usize {
        buf.copy_from_slice(&self);
        self.len()
    }

    fn encoded_len(&self) -> usize {
        self.len()
    }
}

impl<'a> Value<'a> {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Int32(_) => stringify!(Value::Int32),
            Value::UInt32(_) => stringify!(Value::UInt32),
            Value::Int64(_) => stringify!(Value::Int64),
            Value::UInt64(_) => stringify!(Value::UInt64),
            Value::Float(_) => stringify!(Value::Float),
            Value::Double(_) => stringify!(Value::Double),
            Value::String(_) => stringify!(Value::String),
            Value::Error(_) => stringify!(Value::Error),
            Value::Opaque(_) => stringify!(Value::Opaque),
            Value::Function(_) => stringify!(Value::Function),
            Value::EmbeddedOut(_) => stringify!(Value::EmbeddedOut),
            Value::EmbeddedIn(_) => stringify!(Value::EmbeddedIn),
        }
    }
    fn encoded_size(&self) -> usize {
        match self {
            &Value::Int32(_) | &Value::UInt32(_) | &Value::Float(_) => 4,
            &Value::Int64(_) | &Value::UInt64(_) | &Value::Double(_) => 8,
            &Value::String(ref i) => i.len() + 1,
            &Value::Error(ref i) => i.len() + 1,
            &Value::Opaque(ref i) => i.len(),
            &Value::EmbeddedIn(ref i) => i.encoded_len(),
            &Value::EmbeddedOut(ref f) => f.buff.len(),
            &Value::Function(ref f) => f.module.len() + f.name.len() + 2,
        }
    }
}

macro_rules! do_list {
    ($do:ident[$($arg:tt),*]) => {
        $($do!$arg;)*
    };
}

macro_rules! impl_from {
    ($src:ty, $dst:path) => {
        impl<'a> From<$src> for Value<'a> {
            fn from(i: $src) -> Self {
                $dst(i)
            }
        }
    };
}

macro_rules! impl_try_into {
    ($src:path, $dst:ty) => {
        impl<'a> TryInto<$dst> for Value<'a> {
            type Error = &'static str;
            fn try_into(self) -> Result<$dst, Self::Error> {
                match self {
                    $src(i) => Ok(i),
                    _ => Err(concat!("Incorrect type, wanted: ", stringify!($dst))),
                }
            }
        }
    };
}

do_list!(impl_from[
    (i32, Value::Int32),
    (u32, Value::UInt32),
    (i64, Value::Int64),
    (u64, Value::UInt64),
    (f32, Value::Float),
    (f64, Value::Double),
    (&'a str, Value::String),
    (&'a [u8], Value::Opaque),
    (Function<'a>, Value::Function),
    (DecodeIter<'a>, Value::EmbeddedOut),
    (ReferencedValues<'a>, Value::EmbeddedIn)
]);

do_list!(impl_try_into[
    (Value::Int32, i32),
    (Value::UInt32, u32),
    (Value::Int64, i64),
    (Value::UInt64, u64),
    (Value::Float, f32),
    (Value::Double, f64),
    (Value::Opaque, &'a [u8]),
    (Value::Function, Function<'a>),
    (Value::EmbeddedOut, DecodeIter<'a>),
    (Value::EmbeddedIn, ReferencedValues<'a>)
]);

impl<'a> TryInto<&'a str> for Value<'a> {
    type Error = &'static str;
    fn try_into(self) -> Result<&'a str, Self::Error> {
        match self {
            Value::Error(i) | Value::String(i) => Ok(i),
            _ => Err(concat!("Incorrect type, wanted: ", "&'a str")),
        }
    }
}

#[derive(Debug, PartialEq)]
enum CType {
    Invalid,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float,
    Double,
    Error,
    String,
    Opaque,
    Function,
    Embedded,
}

impl CType {
    fn from_u32(i: u32) -> Option<Self> {
        match i {
            0 => Some(CType::Invalid),
            1 => Some(CType::Int32),
            2 => Some(CType::UInt32),
            3 => Some(CType::Int64),
            4 => Some(CType::UInt64),
            5 => Some(CType::Float),
            6 => Some(CType::Double),
            7 => Some(CType::Error),
            8 => Some(CType::String),
            9 => Some(CType::Opaque),
            10 => Some(CType::Function),
            11 => Some(CType::Embedded),
            _ => None,
        }
    }
}

pub fn encoded_len(values: &[Value]) -> usize {
    let mut len = values.len() * 8 + 8;
    for value in values {
        len += value.encoded_size();
    }
    len
}

#[macro_export]
macro_rules! sos {
    ( $($e:expr) , * ) => {
        {
        use sos::ReferencedValues;
        ReferencedValues(&[
            $(
                $e.into()
            )*
        ])
        }
    };
}

#[allow(unused_must_use)]
fn encode_sos(buf: &mut [u8], values: &[Value]) -> usize {
    let mut coffset = 16;

    for value in values {
        let length: u32;
        let val_type;

        match value {
            &Value::Int32(i) => {
                length = 4;
                val_type = CType::Int32;
                NativeEndian::write_i32(&mut buf[coffset..], i)
            }
            &Value::UInt32(i) => {
                length = 4;
                val_type = CType::UInt32;
                NativeEndian::write_u32(&mut buf[coffset..], i)
            }
            &Value::Int64(i) => {
                length = 8;
                val_type = CType::Int64;
                NativeEndian::write_i64(&mut buf[coffset..], i)
            }
            &Value::UInt64(i) => {
                length = 8;
                val_type = CType::UInt64;
                NativeEndian::write_u64(&mut buf[coffset..], i)
            }
            &Value::Float(i) => {
                length = 4;
                val_type = CType::Float;
                NativeEndian::write_f32(&mut buf[coffset..], i)
            }
            &Value::Double(i) => {
                length = 8;
                val_type = CType::Double;
                NativeEndian::write_f64(&mut buf[coffset..], i)
            }
            &Value::String(ref i) => {
                length = i.len() as u32 + 1;
                val_type = CType::String;
                (&mut buf[coffset..coffset + i.len()]).copy_from_slice(i.as_bytes());
                buf[coffset + i.len()] = 0;
            }
            &Value::Error(ref i) => {
                length = i.len() as u32 + 1;
                val_type = CType::Error;
                (&mut buf[coffset..coffset + i.len()]).copy_from_slice(i.as_bytes());
                buf[coffset + i.len()] = 0;
            }
            &Value::Opaque(ref i) => {
                length = i.len() as u32;
                val_type = CType::Opaque;
                (&mut buf[coffset..coffset + i.len()]).copy_from_slice(&i);
            }
            &Value::EmbeddedIn(ref i) => {
                val_type = CType::Embedded;
                length = i.encode(&mut buf[coffset..]) as u32;
            }
            &Value::EmbeddedOut(ref f) => {
                val_type = CType::Embedded;
                length = f.encode(&mut buf[coffset..]) as u32;
            }
            &Value::Function(ref f) => {
                length = (f.module.len() + f.name.len() + 2) as u32;
                val_type = CType::Function;
                (&mut buf[coffset..coffset + f.module.len()]).copy_from_slice(f.module.as_bytes());
                buf[coffset + f.module.len()] = 0;
                (&mut buf
                    [coffset + 1 + f.module.len()..coffset + 1 + f.module.len() + f.name.len()])
                    .copy_from_slice(f.name.as_bytes());
                buf[coffset + 1 + length as usize - 1] = 0;
            }
        }
        NativeEndian::write_u32(&mut buf[coffset - 8..coffset - 4], val_type as u32);
        NativeEndian::write_u32(&mut buf[coffset - 4..coffset], length);
        coffset += length as usize + 8
    }
    NativeEndian::write_u32(&mut buf[..4], values.len() as u32);
    NativeEndian::write_u32(&mut buf[4..8], 0);

    return coffset;
}

#[derive(Debug, Clone)]
pub struct DecodeIter<'a> {
    count: usize,
    buff: &'a [u8],
}

impl<'a> SOS for DecodeIter<'a> {
    fn encoded_len(&self) -> usize {
        self.buff.len() + 8
    }
    fn encode(&self, buf: &mut [u8]) -> usize {
        NativeEndian::write_u32(&mut buf[..4], self.count as u32);
        NativeEndian::write_u32(&mut buf[4..8], 0);
        buf[8..].copy_from_slice(&self.buff);
        self.encoded_len()
    }
}

impl<'a> DecodeIter<'a> {
    fn count(&self) -> usize {
        self.count
    }
}

fn decode_sos(buff: &[u8]) -> DecodeIter {
    let count = NativeEndian::read_u32(&buff[..4]) as usize;
    DecodeIter {
        count: count,
        buff: &buff[8..],
    }
}

impl<'a> Iterator for DecodeIter<'a> {
    type Item = Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buff.len() <= 8 || self.count == 0 {
            return None;
        }
        let val_type = NativeEndian::read_u32(&self.buff[..4]);
        let val_length = NativeEndian::read_u32(&self.buff[4..8]) as usize;
        assert!(val_length != 0);
        assert!(val_length + 8 < self.buff.len());
        let val_data = &self.buff[8..8 + val_length];
        let val = match CType::from_u32(val_type)? {
            CType::Invalid => return None,
            CType::Int32 => Value::Int32(NativeEndian::read_i32(&val_data)),
            CType::UInt32 => Value::UInt32(NativeEndian::read_u32(&val_data)),
            CType::Int64 => Value::Int64(NativeEndian::read_i64(&val_data)),
            CType::UInt64 => Value::UInt64(NativeEndian::read_u64(&val_data)),
            CType::Float => Value::Float(NativeEndian::read_f32(&val_data)),
            CType::Double => Value::Double(NativeEndian::read_f64(&val_data)),
            CType::String => Value::String(from_utf8(&val_data[..val_length - 1]).unwrap()),
            CType::Error => Value::Error(from_utf8(&val_data[..val_length - 1]).unwrap()),
            CType::Opaque => Value::Opaque(&val_data[..val_length]),
            CType::Function => {
                let vec = &val_data[..val_length - 1];
                let pos = vec.iter().position(|&x| x == '\0' as u8).unwrap();
                let module = from_utf8(&vec[..pos]).unwrap();
                let name = from_utf8(&vec[pos + 1..]).unwrap();
                Value::Function(Function { module, name })
            }
            CType::Embedded => Value::EmbeddedOut(DecodeIter {
                count: NativeEndian::read_u32(&self.buff[..4]) as usize,
                buff: &val_data[8..],
            }),
        };
        self.buff = &self.buff[val_length + 8..];
        self.count -= 1;
        Some(val)
    }
}

#[test]
fn encode_decode() {
    let mut buf = [0; 100];
    let vals = &[
        Value::Int64(3),
        Value::Double(2.8),
        Value::Error("Hello".to_string()),
        Value::Opaque([1, 2, 3].to_vec()),
        Value::String("world".to_string()),
    ];
    let len = EncodeSOS(&mut buf, vals);
    let decoded = DecodeSOS(&buf[..len]);
    assert_eq!(vals.to_vec(), decoded)
}
