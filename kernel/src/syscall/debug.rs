use alloc::string::String;
use core::ops::Range;

use super::data::TimeSpec;
use super::number::*;
use super::validate::*;

// Copied from std
pub struct EscapeDefault {
    range: Range<usize>,
    data: [u8; 4],
}

pub fn escape_default(c: u8) -> EscapeDefault {
    let (data, len) = match c {
        b'\t' => ([b'\\', b't', 0, 0], 2),
        b'\r' => ([b'\\', b'r', 0, 0], 2),
        b'\n' => ([b'\\', b'n', 0, 0], 2),
        b'\\' => ([b'\\', b'\\', 0, 0], 2),
        b'\'' => ([b'\\', b'\'', 0, 0], 2),
        b'"' => ([b'\\', b'"', 0, 0], 2),
        b'\x20'...b'\x7e' => ([c, 0, 0, 0], 1),
        _ => ([b'\\', b'x', hexify(c >> 4), hexify(c & 0xf)], 4),
    };

    return EscapeDefault {
        range: (0..len),
        data: data,
    };

    fn hexify(b: u8) -> u8 {
        match b {
            0...9 => b'0' + b,
            _ => b'a' + b - 10,
        }
    }
}

impl Iterator for EscapeDefault {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        self.range.next().map(|i| self.data[i])
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

pub fn format_call(a: usize, b: usize, c: usize, d: usize, e: usize, f: usize) -> String {
    match a {
        SYS_BRK => format!("brk({:#X})", b),
        SYS_CLOCK_GETTIME => format!(
            "clock_gettime({}, {:?})",
            b,
            validate_slice_mut(c as *mut TimeSpec, 1)
        ),
        SYS_EXIT => format!("exit({})", b),
        SYS_FUTEX => format!(
            "futex({:#X} [{:?}], {}, {}, {}, {})",
            b,
            validate_slice_mut(b as *mut i32, 1).map(|uaddr| &mut uaddr[0]),
            c,
            d,
            e,
            f
        ),
        SYS_GETPID => format!("getpid()"),
        SYS_IOPL => format!("iopl({})", b),
        SYS_KILL => format!("kill({}, {})", b, c),
        SYS_NANOSLEEP => format!(
            "nanosleep({:?}, ({}, {}))",
            validate_slice(b as *const TimeSpec, 1),
            c,
            d
        ),
        SYS_PHYSALLOC => format!("physalloc({})", b),
        SYS_PHYSFREE => format!("physfree({:#X}, {})", b, c),
        SYS_PHYSMAP => format!("physmap({:#X}, {}, {:#X})", b, c, d),
        SYS_PHYSUNMAP => format!("physunmap({:#X})", b),
        SYS_VIRTTOPHYS => format!("virttophys({:#X})", b),
        SYS_WAITPID => format!("waitpid({}, {:#X}, {})", b, c, d),
        SYS_YIELD => format!("yield()"),
        _ => format!(
            "UNKNOWN{} {:#X}({:#X}, {:#X}, {:#X}, {:#X}, {:#X})",
            a, a, b, c, d, e, f
        ),
    }
}
