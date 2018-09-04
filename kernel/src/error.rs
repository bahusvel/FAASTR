use core::result;

pub type Result<'a, T> = result::Result<T, &'a str>;
