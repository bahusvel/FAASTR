#[cfg(feature = "acpi")]
use acpi;
use syscall::io::{Io, Pio};

#[no_mangle]
pub unsafe extern "C" fn kreset() -> ! {
    println!("kreset");

    // 8042 reset
    {
        println!("Reset with 8042");
        let mut port = Pio::<u8>::new(0x64);
        while port.readf(2) {}
        port.write(0xFE);
    }

    // Use triple fault to guarantee reset
    asm!("cli" : : : : "intel", "volatile");
    asm!("lidt cs:0" : : : : "intel", "volatile");
    asm!("int $$3" : : : : "intel", "volatile");

    unreachable!();
}

#[no_mangle]
pub unsafe extern "C" fn kstop() -> ! {
    println!("kstop");

    #[cfg(feature = "acpi")] acpi::set_global_s_state(5);

    // Magic shutdown code for bochs and qemu (older versions).
    for c in "Shutdown".bytes() {
        let port = 0x8900;
        println!("Shutdown with outb(0x{:X}, '{}')", port, c as char);
        Pio::<u8>::new(port).write(c);
    }

    // Magic code for VMWare. Also a hard lock.
    println!("Shutdown with cli hlt");
    asm!("cli; hlt" : : : : "intel", "volatile");

    unreachable!();
}
