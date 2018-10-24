extern crate rand;
extern crate rand_xorshift;
extern crate sos;
use rand::distributions::Alphanumeric;
use rand::prelude::*;
use rand_xorshift::XorShiftRng;
use sos::*;

const MAX_LENGTH: usize = 1000;

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
static TYPES: [RngType; 10] = [
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
];

fn gen_rand_string<R: Rng>(rng: &mut R) -> String {
    let length = rng.gen::<usize>() % MAX_LENGTH;
    rng.sample_iter(&Alphanumeric).take(length).collect()
}

fn gen_rand_type<R: Rng>(t: RngType, rng: &mut R) -> sos::Value<'static> {
    match t {
        RngType::Int32 => sos::Value::Int32(rng.gen()),
        RngType::UInt32 => sos::Value::UInt32(rng.gen()),
        RngType::Int64 => sos::Value::Int64(rng.gen()),
        RngType::UInt64 => sos::Value::UInt64(rng.gen()),
        RngType::Float => sos::Value::Float(rng.gen()),
        RngType::Double => sos::Value::Double(rng.gen()),
        RngType::String => sos::Value::OwnedString(gen_rand_string(rng)),
        RngType::Error => sos::Value::OwnedError(gen_rand_string(rng)),
        _ => panic!("Not implemented"), /*
    RngType::Opaque =>,
    RngType::Function =>,
    RngType::Embedded =>,
    */
    }
}

fn gen_rand_sos<R: Rng>(num_values: usize, rng: &mut R) -> Vec<sos::Value<'static>> {
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
    let decoded = decode_sos(&buf[..len]).collect::<Vec<_>>();
    println!("{:?}", decoded);
    assert_eq!(rvals, ReferencedValues(&decoded[..]))
}
