use context;

pub fn sys_fuse() {
    let module = context::initfs_module("exit").expect("Failed to load module");
    println!("Loaded");
    context::fuse_name(module, "hellohelloexit").expect("Failed to call");
    println!("Exited to caller");
}

pub fn sys_cast() {
    let module = context::initfs_module("exit").expect("Failed to load module");
    println!("Loaded");
    context::cast_name(module.clone(), "hellohelloexit").expect("Failed to call");
}
