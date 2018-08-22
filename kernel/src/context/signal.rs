use alloc::arc::Arc;
use core::mem;

use context::{contexts, switch, Status, WaitpidKey};
use start::usermode;
use syscall;
use syscall::flag::{SIGCHLD, SIGCONT, SIGSTOP, SIGTSTP, SIGTTIN, SIGTTOU, SIG_DFL, SIG_IGN};

pub extern "C" fn signal_handler(sig: usize) {
    // TODO at the moment we will just exit for all signals. In future we will issue a cast before doing so.
    syscall::exit(sig)
}
