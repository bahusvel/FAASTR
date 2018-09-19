#![no_std]
#![feature(try_from)]
#![allow(dead_code)]
extern crate byteorder;

use self::byteorder::{ByteOrder, NativeEndian};
use core::convert::TryInto;
use core::ops::Deref;
use core::str::from_utf8;

const NULL: [u8; 1] = [0];
const WRONG_TYPE: &str = "Received value is of incorrect type";

#[derive(Debug, PartialEq, Clone)]
pub struct Function<'a> {
    pub module: &'a str,
    pub name: &'a str,
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
    EmbeddedOut(SOSIter<'a>),
    EmbeddedIn(&'a [Value<'a>]),
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
            &Value::EmbeddedIn(ref i) => encoded_len(i),
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
    (SOSIter<'a>, Value::EmbeddedOut),
    (&'a [Value<'a>], Value::EmbeddedIn)
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
    (Value::EmbeddedOut, SOSIter<'a>),
    (Value::EmbeddedIn, &'a [Value<'a>])
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
        [
            $(
                $e.into()
            )*
        ]
    };
}

#[allow(unused_must_use)]
pub fn encode_sos(buf: &mut [u8], values: &[Value]) -> usize {
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
                length = encode_sos(&mut buf[coffset..], i) as u32;
            }
            &Value::EmbeddedOut(ref f) => {
                val_type = CType::Embedded;
                length = f.buff.len() as u32;
                (&mut buf[coffset..coffset + length as usize]).copy_from_slice(&f.buff);
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
pub struct SOSIter<'a> {
    count: usize,
    buff: &'a [u8],
}

impl<'a> SOSIter<'a> {
    fn count(&self) -> usize {
        self.count
    }
}

pub fn decode_sos(buff: &[u8]) -> SOSIter {
    let count = NativeEndian::read_u32(&buff[..4]) as usize;
    SOSIter {
        count: count,
        buff: &buff[8..],
    }
}

impl<'a> Iterator for SOSIter<'a> {
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
            CType::Embedded => Value::EmbeddedOut(SOSIter {
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
