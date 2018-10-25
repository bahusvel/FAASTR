#![feature(plugin)]
#![plugin(quickcheck_macros)]
#![feature(custom_attribute)]
//#[macro_use]
extern crate quickcheck;
extern crate rand;
extern crate rand_xorshift;
extern crate sos;
use quickcheck::{Arbitrary, Gen};
use rand::distributions::{Alphanumeric, Standard};
use rand::prelude::*;
use rand_xorshift::XorShiftRng;
use sos::*;
use std::fmt::{Debug, Formatter, Result};

const MAX_LENGTH: usize = 1000;
const MAX_EMBEDDED_SIZE: usize = 5;

#[derive(Clone, Copy)]
enum RngType {
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
static TYPES: [RngType; 11] = [
    RngType::Int32,
    RngType::UInt32,
    RngType::Int64,
    RngType::UInt64,
    RngType::Float,
    RngType::Double,
    RngType::Error,
    RngType::String,
    RngType::Opaque,
    RngType::Function,
    RngType::Embedded,
];

#[derive(Clone)]
struct RngValue(OwnedValue);

impl Debug for RngValue {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.0.fmt(f)
    }
}

impl Arbitrary for RngValue {
    fn arbitrary<G: Gen>(rng: &mut G) -> Self {
        RngValue(gen_rand_type(*rng.choose(&TYPES[..]).unwrap(), rng))
    }
}

fn gen_rand_string<R: Rng>(rng: &mut R) -> String {
    let length = rng.gen::<usize>() % MAX_LENGTH;
    rng.sample_iter(&Alphanumeric).take(length).collect()
}

fn gen_rand_type<R: Rng>(t: RngType, rng: &mut R) -> OwnedValue {
    match t {
        RngType::Int32 => OwnedValue::Int32(rng.gen()),
        RngType::UInt32 => OwnedValue::UInt32(rng.gen()),
        RngType::Int64 => OwnedValue::Int64(rng.gen()),
        RngType::UInt64 => OwnedValue::UInt64(rng.gen()),
        RngType::Float => OwnedValue::Float(rng.gen()),
        RngType::Double => OwnedValue::Double(rng.gen()),
        RngType::String => OwnedValue::String(gen_rand_string(rng)),
        RngType::Error => OwnedValue::Error(gen_rand_string(rng)),
        RngType::Opaque => {
            let length = rng.gen::<usize>() % MAX_LENGTH;
            OwnedValue::Opaque(rng.sample_iter(&Standard).take(length).collect())
        }
        RngType::Function => OwnedValue::Function(OwnedFunction {
            module: gen_rand_string(rng),
            name: gen_rand_string(rng),
        }),
        RngType::Embedded => OwnedValue::Embedded(gen_rand_sos(MAX_EMBEDDED_SIZE, rng)),
    }
}

fn gen_rand_sos<R: Rng>(num_values: usize, rng: &mut R) -> Vec<OwnedValue> {
    let mut vals = Vec::new();
    for _ in 0..num_values {
        vals.push(gen_rand_type(*rng.choose(&TYPES[..]).unwrap(), rng));
    }
    vals
}

#[test]
fn encode_decode() {
    let mut buf = [0; 100];
    let vals = [
        3.into(),
        2.8.into(),
        Value::Error("Hello"),
        Value::Opaque(&[1, 2, 3]),
        Value::String("world"),
    ];
    let rvals = ReferencedValues(&vals);
    let len = rvals.encode(&mut buf[..]);
    println!("Encoded {:?}", &buf[..len]);
    let decoded = decode_sos(&buf[..len], false).collect::<Vec<_>>();
    println!("{:?}", decoded);
    assert_eq!(rvals, ReferencedValues(&decoded[..]))
}

#[test]
fn rand_encode_decode() {
    let mut rng = XorShiftRng::seed_from_u64(10);
    let vals = gen_rand_sos(MAX_EMBEDDED_SIZE, &mut rng);
    println!("Encoding {:?}", vals);
    let rvals = vals.iter().map(|v| v.borrow()).collect::<Vec<Value>>();
    let refvals = ReferencedValues(&rvals[..]);
    let length = refvals.encoded_len();
    let mut buf = Vec::with_capacity(length);
    unsafe {
        buf.set_len(length);
    }
    refvals.encode(&mut buf[..]);
    let decoded = decode_sos(&buf, false).collect::<Vec<_>>();
    println!("Decoded {:?}", decoded);
    assert_eq!(&rvals[..], &decoded[..]);
}

#[quickcheck]
fn encode_decode_identity(vals: Vec<RngValue>) -> bool {
    //println!("Encoding {:?}", vals);
    let rvals = vals.iter().map(|v| v.0.borrow()).collect::<Vec<Value>>();
    let refvals = ReferencedValues(&rvals[..]);
    let length = refvals.encoded_len();
    let mut buf = Vec::with_capacity(length);
    unsafe {
        buf.set_len(length);
    }
    refvals.encode(&mut buf[..]);
    let decoded = decode_sos(&buf, false).collect::<Vec<_>>();
    if &rvals[..] != &decoded[..] {
        println!("Decoded {:?}\nArgs {:?}", decoded, rvals);
    }
    &rvals[..] == &decoded[..]
}
