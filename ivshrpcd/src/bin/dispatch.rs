use super::{ivshrpc_cast, ivshrpc_fuse};
use either::Either;
use fnv::FnvHashMap;
use spin::RwLock;
use std::convert::TryInto;

use sos::{
    DecodeIter, EncodedValues, Function, JustError, OwnedEncodedValues, OwnedFunction, Value,
};

type FuseFunc = fn(args: DecodeIter) -> OwnedEncodedValues;
type CastFunc = fn(args: DecodeIter);

lazy_static! {
    static ref FUNC_TABLE: RwLock<FnvHashMap<OwnedFunction, Either<CastFunc, FuseFunc>>> = {
        let mut map = FnvHashMap::default();
        map.insert(
            OwnedFunction::new("host", "hello"),
            Either::Left(hello as CastFunc),
        );
        map.insert(
            OwnedFunction::new("host", "hello_fuse"),
            Either::Right(hello_fuse as FuseFunc),
        );
        map.insert(
            OwnedFunction::new("host", "cast_test"),
            Either::Left(cast_test as CastFunc),
        );
        RwLock::new(map)
    };
}

fn hello(args: DecodeIter) {
    println!("Hello from host {:?}", args.collect::<Vec<Value>>())
}

fn hello_fuse(args: DecodeIter) -> OwnedEncodedValues {
    let msg = format!("Hello from host {:?}", args.collect::<Vec<Value>>());
    EncodedValues::from(sos![msg.as_str()]).into_owned()
}

fn cast_test(mut args: DecodeIter) {
    let ret = ivshrpc_fuse(sos![("call", "print"), args.next().unwrap()]);
    println!(
        "Kernel returned {:?}",
        EncodedValues::from(ret.unwrap())
            .decode()
            .collect::<Vec<_>>()
    );
}

pub fn dispatch<'a, 'b>(
    args: OwnedEncodedValues,
    fuse: bool,
) -> Result<OwnedEncodedValues, JustError<'static>> {
    let args = EncodedValues::from(args);
    let mut iter = args.decode();
    let function: Function = iter
        .next()
        .ok_or(JustError::new("Not enough arguments"))?
        .try_into()
        .map_err(|e| JustError::new(e))?;

    let lock = FUNC_TABLE.read();

    let func = lock
        .get(&OwnedFunction::from(function)) // TODO avoid this stupid copying operation
        .ok_or(JustError::new("No such function"))?;

    if fuse {
        Ok(func.right().ok_or(JustError::new(
            "Attempt to fuse to a cast only function",
        ))?(iter))
    } else {
        func.left()
            .ok_or(JustError::new("Attempt to cast to a fuse only function"))?(iter);
        Ok(EncodedValues::from(sos!()).into_owned())
    }
}
