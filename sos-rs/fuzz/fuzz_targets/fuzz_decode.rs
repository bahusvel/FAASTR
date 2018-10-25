#![no_main]
#[macro_use]
extern crate libfuzzer_sys;
extern crate sos;
use sos::decode_sos;

fuzz_target!(|data: &[u8]| {
    let _ = decode_sos(data, false).map(|i| i.collect::<Vec<_>>());
});
