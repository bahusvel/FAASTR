use context;

use alloc::vec::Vec;
use core::convert::TryInto;
use sos::{EncodedValues, Function, JustError, Value};
use syscall::exit;

pub fn sys_fuse(args: EncodedValues) -> Result<EncodedValues, JustError<'static>> {
    let mut iter = args.decode();
    let function: Function = iter
        .next()
        .ok_or(JustError::new("Not enough arguments"))?
        .try_into()
        .map_err(|e| JustError::new(e))?;

    // DEBUG, inefficient, forces decode of args
    let fargs: Vec<Value> = iter.clone().collect();
    println!("Doing a fuse call {:?}({:?})", function, fargs);

    let module = context::initfs_module(function.module).map_err(|e| JustError::new(e))?;
    println!("Loaded");

    let ret = context::fuse_name(module, function.name, &iter).map_err(|e| JustError::new(e))?;
    println!("Exited to caller");

    Ok(ret)
}

pub fn sys_cast(args: EncodedValues) -> Result<(), JustError<'static>> {
    let mut iter = args.decode();

    let function: Function = iter
        .next()
        .ok_or(JustError::new("Not enough arguments"))?
        .try_into()
        .map_err(|e| JustError::new(e))?;

    // TODO don't do that, I don't need to decode these, just pass them directly through
    let fargs: Vec<Value> = iter.clone().collect();

    println!("Doing a cast call {:?}({:?})", function, fargs);
    let module = context::initfs_module(function.module).map_err(|e| JustError::new(e))?;
    println!("Loaded");

    context::cast_name(module, function.name, &iter).map_err(|e| JustError::new(e))?;

    Ok(())
}

pub fn sys_return(values: EncodedValues) -> ! {
    {
        let current_context = context::contexts_mut()
            .current()
            .expect("No current context")
            .clone();
        println!(
            "Function {} exited with {:?}",
            current_context.read().name(),
            values.decode().collect::<Vec<Value>>()
        )
    }
    exit(0);
}
