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
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::convert::TryInto;
use core::fmt::Debug;
use core::ops::Deref;
use core::slice;
use core::str::from_utf8;

const NULL: [u8; 1] = [0];
const WRONG_TYPE: &str = "Received value is of incorrect type";

pub type EncodedValuesPtr = *const u8;

type SyntacticFunc<'a> = (&'a str, &'a str);

#[derive(PartialEq, Eq, Clone, Hash)]
pub struct Function<'a> {
    pub module: &'a str,
    pub name: &'a str,
}

#[cfg(feature = "alloc")]
#[derive(PartialEq, Eq, Clone, Hash)]
pub struct OwnedFunction {
    pub module: String,
    pub name: String,
}

#[cfg(feature = "alloc")]
impl OwnedFunction {
    pub fn new(module: &str, name: &str) -> Self {
        OwnedFunction {
            module: String::from(module),
            name: String::from(name),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'a> From<Function<'a>> for OwnedFunction {
    fn from(f: Function<'a>) -> Self {
        OwnedFunction {
            module: String::from(f.module),
            name: String::from(f.name),
        }
    }
}

#[cfg(feature = "alloc")]
impl Debug for OwnedFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
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

#[derive(Debug, Clone, PartialEq)]
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
    #[cfg(feature = "alloc")]
    EmbeddedVec(Vec<Value<'a>>),
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq)]
pub enum OwnedValue {
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Error(String),
    String(String),
    Opaque(Vec<u8>),
    Function(OwnedFunction),
    Embedded(Vec<OwnedValue>),
}

#[cfg(feature = "alloc")]
impl OwnedValue {
    pub fn borrow(&self) -> Value {
        match self {
            OwnedValue::Int32(i) => Value::Int32(*i),
            OwnedValue::UInt32(i) => Value::UInt32(*i),
            OwnedValue::Int64(i) => Value::Int64(*i),
            OwnedValue::UInt64(i) => Value::UInt64(*i),
            OwnedValue::Float(i) => Value::Float(*i),
            OwnedValue::Double(i) => Value::Double(*i),
            OwnedValue::Error(i) => Value::Error(i),
            OwnedValue::String(i) => Value::String(i),
            OwnedValue::Opaque(i) => Value::Opaque(i),
            OwnedValue::Function(i) => Value::Function(Function {
                module: &i.module,
                name: &i.name,
            }),
            OwnedValue::Embedded(i) => Value::EmbeddedVec(i.iter().map(|v| v.borrow()).collect()),
        }
    }
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

#[derive(Debug, Clone, PartialEq)]
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
pub type OwnedEncodedValues = Vec<u8>;

#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct EncodedValues<'a>(Cow<'a, [u8]>);

#[cfg(feature = "alloc")]
impl<'a> Deref for EncodedValues<'a> {
    type Target = Cow<'a, [u8]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "alloc")]
impl<'a> EncodedValues<'a> {
    pub fn decode(&self) -> Option<DecodeIter> {
        decode_sos(&self, true)
    }
    pub fn into_owned(self) -> OwnedEncodedValues {
        self.0.into_owned()
    }
    // NO, length is not currently sent, I need to send it.
    pub unsafe fn from_ptr(ptr: EncodedValuesPtr) -> Self {
        let length = *(ptr as *const u32).offset(1);
        EncodedValues(Cow::Borrowed(slice::from_raw_parts(ptr, length as usize)))
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
    pub fn type_name(&self) -> &'static str {
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
            #[cfg(feature = "alloc")]
            Value::EmbeddedVec(_) => stringify!(Value::EmbeddedVec),
        }
    }
    #[inline(always)]
    fn ctype(&self) -> CType {
        match self {
            Value::Int32(_) => CType::Int32,
            Value::UInt32(_) => CType::UInt32,
            Value::Int64(_) => CType::Int64,
            Value::UInt64(_) => CType::UInt64,
            Value::Float(_) => CType::Float,
            Value::Double(_) => CType::Double,
            Value::String(_) => CType::String,
            Value::Error(_) => CType::Error,
            Value::Opaque(_) => CType::Opaque,
            Value::Function(_) => CType::Function,
            Value::EmbeddedOut(_) | Value::EmbeddedIn(_) => CType::Embedded,
            #[cfg(feature = "alloc")]
            Value::EmbeddedVec(_) => CType::Embedded,
        }
    }
    #[inline(always)]
    fn encoded_size(&self) -> usize {
        match self {
            &Value::Int32(_) | &Value::UInt32(_) | &Value::Float(_) => 4,
            &Value::Int64(_) | &Value::UInt64(_) | &Value::Double(_) => 8,
            &Value::String(ref i) => i.len() + 1 + 4,
            &Value::Error(ref i) => i.len() + 1 + 4,
            &Value::Opaque(ref i) => i.len() + 4,
            &Value::EmbeddedIn(ref i) => i.encoded_len() + 4,
            &Value::EmbeddedOut(ref f) => f.buff.len() + 4,
            #[cfg(feature = "alloc")]
            &Value::EmbeddedVec(ref i) => ReferencedValues(&i[..]).encoded_len() + 4,
            &Value::Function(ref f) => f.module.len() + f.name.len() + 2 + 4,
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

impl<'a> From<(&'a str, &'a str)> for Value<'a> {
    fn from(i: (&'a str, &'a str)) -> Self {
        Value::Function(Function {
            module: i.0,
            name: i.1,
        })
    }
}

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

#[derive(Debug, PartialEq, Clone, Copy)]
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
    fn from_u8(i: u8) -> Option<Self> {
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
    let mut len = values.len() + 8;
    for value in values {
        len += value.encoded_size();
    }
    len
}

#[macro_export]
macro_rules! sos {
    ( $( $e:expr ),* ) => {
        {
        use sos::ReferencedValues;
        ReferencedValues(&[
            $(
                $e.into(),
            )*
        ])
        }
    };
}

#[allow(unused_must_use)]
fn encode_sos(buf: &mut [u8], values: &[Value]) -> usize {
    let len = ReferencedValues(values).encoded_len();
    assert!(buf.len() >= len);
    let buf = &mut buf[..len];
    NativeEndian::write_u32(&mut buf[..4], values.len() as u32);
    NativeEndian::write_u32(&mut buf[4..8], len as u32);
    let mut coffset = 8;
    for value in values {
        let val_type = value.ctype();
        let mut length = value.encoded_size();
        buf[coffset] = val_type as u8;
        coffset += match val_type {
            CType::Int32
            | CType::UInt32
            | CType::Float
            | CType::Int64
            | CType::UInt64
            | CType::Double => 1,
            _ => {
                length -= 4;
                NativeEndian::write_u32(&mut buf[coffset + 1..coffset + 4 + 1], length as u32);
                5
            }
        };
        let wbuf = &mut buf[coffset..coffset + length];
        match value {
            &Value::Int32(i) => NativeEndian::write_i32(wbuf, i),
            &Value::UInt32(i) => NativeEndian::write_u32(wbuf, i),
            &Value::Int64(i) => NativeEndian::write_i64(wbuf, i),
            &Value::UInt64(i) => NativeEndian::write_u64(wbuf, i),
            &Value::Float(i) => NativeEndian::write_f32(wbuf, i),
            &Value::Double(i) => NativeEndian::write_f64(wbuf, i),
            &Value::String(i) => {
                wbuf[..length - 1].copy_from_slice(i.as_bytes());
                wbuf[length - 1] = 0;
            }
            &Value::Error(i) => {
                wbuf[..length - 1].copy_from_slice(i.as_bytes());
                wbuf[length - 1] = 0;
            }
            &Value::Opaque(i) => {
                wbuf.copy_from_slice(&i);
            }
            &Value::EmbeddedIn(ref i) => {
                i.encode(wbuf);
            }
            &Value::EmbeddedOut(ref f) => {
                f.encode(wbuf);
            }
            &Value::EmbeddedVec(ref i) => {
                ReferencedValues(&i[..]).encode(wbuf);
            }
            &Value::Function(ref f) => {
                let modlen = f.module.len();
                wbuf[..modlen].copy_from_slice(f.module.as_bytes());
                wbuf[modlen] = 0;
                wbuf[1 + modlen..length as usize - 1].copy_from_slice(f.name.as_bytes());
                wbuf[length as usize - 1] = 0;
            }
        }
        coffset += length as usize;
    }
    return coffset;
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeIter<'a> {
    count: usize,
    buff: &'a [u8],
    lazy: bool,
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

pub fn decode_sos(buff: &[u8], lazy: bool) -> Option<DecodeIter> {
    if buff.len() <= 8 {
        return None;
    }
    let count = NativeEndian::read_u32(&buff[..4]) as usize;
    let size = NativeEndian::read_u32(&buff[4..8]) as usize;
    if buff.len() < size || size < 8 {
        return None;
    }
    Some(DecodeIter {
        count: count,
        buff: &buff[8..size],
        lazy: lazy,
    })
}

impl<'a> Iterator for DecodeIter<'a> {
    type Item = Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buff.len() < 5 || self.count == 0 {
            return None;
        }
        let mut offset = 1;
        let val_type = CType::from_u8(self.buff[0])?;
        let val_length = match val_type {
            CType::Int32 | CType::UInt32 | CType::Float => 4,
            CType::Int64 | CType::UInt64 | CType::Double => 8,
            _ => {
                offset += 4;
                NativeEndian::read_u32(&self.buff[1..5]) as usize
            }
        };
        if self.buff.len() < val_length + offset {
            return None;
        }
        let val_data = &self.buff[offset..offset + val_length];
        let val = match val_type {
            CType::Invalid => return None,
            CType::Int32 => Value::Int32(NativeEndian::read_i32(&val_data)),
            CType::UInt32 => Value::UInt32(NativeEndian::read_u32(&val_data)),
            CType::Int64 => Value::Int64(NativeEndian::read_i64(&val_data)),
            CType::UInt64 => Value::UInt64(NativeEndian::read_u64(&val_data)),
            CType::Float => Value::Float(NativeEndian::read_f32(&val_data)),
            CType::Double => Value::Double(NativeEndian::read_f64(&val_data)),
            CType::String => Value::String(
                from_utf8(&val_data[..if val_length == 0 { 0 } else { val_length - 1 }]).ok()?,
            ),
            CType::Error => Value::Error(
                from_utf8(&val_data[..if val_length == 0 { 0 } else { val_length - 1 }]).ok()?,
            ),
            CType::Opaque => Value::Opaque(&val_data[..val_length]),
            CType::Function => {
                let vec = &val_data[..if val_length == 0 { 0 } else { val_length - 1 }];
                let pos = vec.iter().position(|&x| x == '\0' as u8)?;
                let module = from_utf8(&vec[..pos]).ok()?;
                let name = from_utf8(&vec[pos + 1..]).ok()?;
                Value::Function(Function { module, name })
            }
            CType::Embedded => {
                let iter = decode_sos(val_data, self.lazy)?;
                if self.lazy {
                    Value::EmbeddedOut(iter)
                } else {
                    Value::EmbeddedVec(iter.collect::<Vec<_>>())
                }
            }
        };
        self.buff = &self.buff[val_length + offset..];
        self.count -= 1;
        Some(val)
    }
}
