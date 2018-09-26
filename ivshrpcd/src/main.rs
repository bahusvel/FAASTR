extern crate memmap;
extern crate ringbuf;

use memmap::MmapMut;
use ringbuf::{Consumer, Header, Producer};
use std::fs::OpenOptions;
use std::str;

const IVSH_PATH: &str = "/dev/shm/ivshmem";
const IVSH_SIZE: usize = 1024 * 1024;

fn main() {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(IVSH_PATH)
        .expect("Failed to open ivshmem");

    file.set_len(IVSH_SIZE as u64)
        .expect("Failed to set file size");

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

    loop {
        let read = consumer.read(5);
        let s = str::from_utf8(&read).unwrap();
        println!("Got this string from kernel: {}", s);
    }
}
