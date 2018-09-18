use context;

use alloc::vec::Vec;
use sos;

pub fn sys_fuse(args: sos::SOSIter) {
    println!(
        "Doing a fuse call ({:?})",
        args.collect::<Vec<sos::Value>>()
    );
    let module = context::initfs_module("exit").expect("Failed to load module");
    println!("Loaded");
    context::fuse_name(module, "hellohelloexit").expect("Failed to call");
    println!("Exited to caller");
}

pub fn sys_cast(args: sos::SOSIter) {
    println!(
        "Doing a cast call ({:?})",
        args.collect::<Vec<sos::Value>>()
    );
    let module = context::initfs_module("exit").expect("Failed to load module");
    println!("Loaded");
    context::cast_name(module.clone(), "hellohelloexit").expect("Failed to call");
}
