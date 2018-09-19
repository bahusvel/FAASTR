use context;

use alloc::vec::Vec;
use sos;
use syscall::exit;

pub fn sys_fuse(args: sos::SOSIter) {
    println!(
        "Doing a fuse call ({:?})",
        args.collect::<Vec<sos::Value>>()
    );
    let module = context::initfs_module("exit").expect("Failed to load module");
    println!("Loaded");
    context::fuse_name(module, "hellohelloexit", &sos!()).expect("Failed to call");
    println!("Exited to caller");
}

pub fn sys_cast(args: sos::SOSIter) {
    println!(
        "Doing a cast call ({:?})",
        args.collect::<Vec<sos::Value>>()
    );
    let module = context::initfs_module("exit").expect("Failed to load module");
    println!("Loaded");
    context::cast_name(module.clone(), "hellohelloexit", &sos!()).expect("Failed to call");
}

pub fn sys_return(values: sos::SOSIter) {
    {
        let current_context = context::contexts_mut()
            .current()
            .expect("No current context")
            .clone();
        println!(
            "Function {} exited with {:?}",
            current_context.read().name(),
            values.collect::<Vec<sos::Value>>()
        )
    }
    exit(0);
}
