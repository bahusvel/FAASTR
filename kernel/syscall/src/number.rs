pub const SYS_CLASS: usize = 0xF000_0000;
pub const SYS_CLASS_PATH: usize = 0x1000_0000;
pub const SYS_CLASS_FILE: usize = 0x2000_0000;

pub const SYS_ARG: usize = 0x0F00_0000;
pub const SYS_ARG_SLICE: usize = 0x0100_0000;
pub const SYS_ARG_MSLICE: usize = 0x0200_0000;
pub const SYS_ARG_PATH: usize = 0x0300_0000;

pub const SYS_RET: usize = 0x00F0_0000;
pub const SYS_RET_FILE: usize = 0x0010_0000;

pub const SYS_BRK: usize = 45;
pub const SYS_CLOCK_GETTIME: usize = 265;
pub const SYS_CLONE: usize = 120;
pub const SYS_EXECVE: usize = 11;
pub const SYS_EXIT: usize = 1;
pub const SYS_FUTEX: usize = 240;
pub const SYS_GETPID: usize = 20;
pub const SYS_IOPL: usize = 110;
pub const SYS_KILL: usize = 37;
pub const SYS_NANOSLEEP: usize = 162;
pub const SYS_PHYSALLOC: usize = 945;
pub const SYS_PHYSFREE: usize = 946;
pub const SYS_PHYSMAP: usize = 947;
pub const SYS_PHYSUNMAP: usize = 948;
pub const SYS_VIRTTOPHYS: usize = 949;
pub const SYS_SIGACTION: usize = 67;
pub const SYS_SIGRETURN: usize = 119;
pub const SYS_WAITPID: usize = 7;
pub const SYS_YIELD: usize = 158;
