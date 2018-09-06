#![no_std]
#![feature(alloc)]
#![allow(dead_code)]
extern crate byteorder;

extern crate alloc;

use self::byteorder::{ByteOrder, NativeEndian};
use alloc::vec::Vec;
use core::str::from_utf8;

const NULL: [u8; 1] = [0];

#[derive(Debug, PartialEq, Clone)]
pub struct Function<'a> {
    pub module: &'a str,
    pub name: &'a str,
}

#[derive(Debug, PartialEq, Clone)]
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
    Embedded(Vec<Value<'a>>),
}

#[derive(Debug, PartialEq)]
enum CType {
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
            0 => Some(CType::Int32),
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

#[allow(non_snake_case)]
#[allow(unused_must_use)]
pub fn EncodeSOS(buf: &mut [u8], values: &[Value]) -> usize {
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
                (&mut buf[coffset..i.len()]).copy_from_slice(&i);
            }
            &Value::Embedded(ref i) => {
                val_type = CType::Embedded;
                length = EncodeSOS(&mut buf[coffset..], i) as u32;
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

#[allow(non_snake_case)]
pub fn DecodeSOS(buff: &[u8]) -> Option<Vec<Value>> {
    let count = NativeEndian::read_u32(&buff[..4]) as usize;
    let mut vals: Vec<Value> = Vec::with_capacity(count as usize);
    let mut coffset = 0;

    for _ in 0..count {
        let val_type = NativeEndian::read_u32(&buff[8 + coffset..8 + coffset + 4]);
        let val_length = NativeEndian::read_u32(&buff[8 + coffset + 4..8 + coffset + 8]) as usize;
        let val_data = &buff[8 + coffset + 8..8 + coffset + 8 + val_length];
        match CType::from_u32(val_type)? {
            CType::Int32 => vals.push(Value::Int32(NativeEndian::read_i32(&val_data))),
            CType::UInt32 => vals.push(Value::UInt32(NativeEndian::read_u32(&val_data))),
            CType::Int64 => vals.push(Value::Int64(NativeEndian::read_i64(&val_data))),
            CType::UInt64 => vals.push(Value::UInt64(NativeEndian::read_u64(&val_data))),
            CType::Float => vals.push(Value::Float(NativeEndian::read_f32(&val_data))),
            CType::Double => vals.push(Value::Double(NativeEndian::read_f64(&val_data))),
            CType::String => {
                vals.push(Value::String(
                    from_utf8(&val_data[..val_length - 1]).unwrap(),
                ));
            }
            CType::Error => {
                vals.push(Value::Error(
                    from_utf8(&val_data[..val_length - 1]).unwrap(),
                ));
            }
            CType::Opaque => {
                let vec = &val_data[..val_length];
                vals.push(Value::Opaque(vec));
            }
            CType::Function => {
                let vec = &val_data[..val_length - 1];
                let pos = vec.iter().position(|&x| x == '\0' as u8).unwrap();
                let module = from_utf8(&vec[..pos]).unwrap();
                let name = from_utf8(&vec[pos + 1..]).unwrap();
                vals.push(Value::Function(Function { module, name }));
            }
            CType::Embedded => {
                vals.push(Value::Embedded(DecodeSOS(&val_data)?));
            }
        }
        coffset += val_length + 8;
    }

    return Some(vals);
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
