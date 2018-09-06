#![feature(test)]
extern crate test;

extern crate sos;

use sos::*;

use test::Bencher;

#[bench]
fn encode_small(b: &mut Bencher) {
    let mut buf = [0; 100];
    let vals = &[Value::Int64(3)];
    b.iter(|| EncodeSOS(&mut buf, vals));
}

#[bench]
fn encode_medium(b: &mut Bencher) {
    let mut buf = [0; 100];
    let vals = &[
        Value::Int64(3),
        Value::Double(2.8),
        Value::Error("Hello".to_string()),
        Value::Opaque([1, 2, 3].to_vec()),
        Value::String("world".to_string()),
    ];
    b.iter(|| EncodeSOS(&mut buf, vals));
}

#[bench]
fn encode_large(b: &mut Bencher) {
    let mut buf = [0; 100];
    let vals = &[
        Value::Int64(3),
        Value::Double(2.8),
        Value::Error("Hello".to_string()),
        Value::Opaque([1, 2, 3].to_vec()),
        Value::String("world".to_string()),
    ];
    b.iter(|| EncodeSOS(&mut buf, vals));
}

#[bench]
fn decode_small(b: &mut Bencher) {
    let mut buf = [0; 100];
    let vals = &[Value::Int64(3)];
    let len = EncodeSOS(&mut buf, vals);
    b.iter(|| DecodeSOS(&buf[..len]));
}

#[bench]
fn decode_medium(b: &mut Bencher) {
    let mut buf = [0; 100];
    let vals = &[
        Value::Int64(3),
        Value::Double(2.8),
        Value::Error("Hello".to_string()),
        Value::Opaque([1, 2, 3].to_vec()),
        Value::String("world".to_string()),
    ];
    let len = EncodeSOS(&mut buf, vals);
    b.iter(|| DecodeSOS(&buf[..len]));
}

#[bench]
fn decode_large(b: &mut Bencher) {
    let mut buf = [0; 100];
    let vals = &[
        Value::Int64(3),
        Value::Double(2.8),
        Value::Error("Hello".to_string()),
        Value::Opaque([1, 2, 3].to_vec()),
        Value::String("world".to_string()),
    ];
    let len = EncodeSOS(&mut buf, vals);
    b.iter(|| DecodeSOS(&buf[..len]));
}
