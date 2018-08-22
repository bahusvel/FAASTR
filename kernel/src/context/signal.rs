use syscall;

pub extern "C" fn signal_handler(sig: usize) {
    // TODO at the moment we will just exit for all signals. In future we will issue a cast before doing so.
    syscall::exit(sig)
}
