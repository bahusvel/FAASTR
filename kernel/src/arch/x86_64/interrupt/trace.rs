use core::mem;
use goblin::elf::sym;

use paging::{ActivePageTable, VirtualAddress};

/// Get a stack trace
//TODO: Check for stack being mapped before dereferencing
#[inline(never)]
pub unsafe fn stack_trace() {
    let mut rbp: usize;
    asm!("" : "={rbp}"(rbp) : : : "intel", "volatile");

    println!("TRACE: {:>016X}", rbp);
    //Maximum 64 frames
    let active_table = ActivePageTable::new();
    for _frame in 0..64 {
        if let Some(rip_rbp) = rbp.checked_add(mem::size_of::<usize>()) {
            if active_table.translate(VirtualAddress::new(rbp)).is_some() && active_table
                .translate(VirtualAddress::new(rip_rbp))
                .is_some()
            {
                let rip = *(rip_rbp as *const usize);
                if rip == 0 {
                    println!(" {:>016X}: EMPTY RETURN", rbp);
                    break;
                }
                println!("  {:>016X}: {:>016X}", rbp, rip);
                rbp = *(rbp as *const usize);
                symbol_trace(rip);
            } else {
                println!("  {:>016X}: GUARD PAGE", rbp);
                break;
            }
        } else {
            println!("  {:>016X}: RBP OVERFLOW", rbp);
        }
    }
}

/// Get a symbol
//TODO: Do not create Elf object for every symbol lookup
#[inline(never)]
pub unsafe fn symbol_trace(addr: usize) {
    use core::slice;
    use core::sync::atomic::Ordering;

    use elf::Elf;
    use start::{KERNEL_BASE, KERNEL_SIZE};

    let kernel_ptr = (KERNEL_BASE.load(Ordering::SeqCst) + ::KERNEL_OFFSET) as *const u8;
    let kernel_slice = slice::from_raw_parts(kernel_ptr, KERNEL_SIZE.load(Ordering::SeqCst));

    let elf = Elf::from(kernel_slice);
    if elf.is_err() {
        return;
    }
    let elf = elf.unwrap();
    let symbols = elf.symbols();
    if symbols.is_none() {
        return;
    }
    let symbols = symbols.unwrap();

    for sym in symbols {
        if sym::st_type(sym.st_info) == sym::STT_FUNC
            && addr >= sym.st_value as usize
            && addr < (sym.st_value + sym.st_size) as usize
        {
            println!(
                "    {:>016X}+{:>04X}",
                sym.st_value,
                addr - sym.st_value as usize
            );

            let sym_name = elf.lookup_symbol_name(sym.st_name);
            if sym_name.is_none() {
                continue;
            }

            let sym_name = sym_name.unwrap();

            print!("    ");

            if sym_name.starts_with(b"_ZN") {
                // Skip _ZN
                let mut i = 3;
                let mut first = true;
                while i < sym_name.len() {
                    // E is the end character
                    if sym_name[i] == b'E' {
                        break;
                    }

                    // Parse length string
                    let mut len = 0;
                    while i < sym_name.len() {
                        let b = sym_name[i];
                        if b >= b'0' && b <= b'9' {
                            i += 1;
                            len *= 10;
                            len += (b - b'0') as usize;
                        } else {
                            break;
                        }
                    }

                    // Print namespace seperator, if required
                    if first {
                        first = false;
                    } else {
                        print!("::");
                    }

                    macro_rules! match_symbol {
                        ($symbol:expr) => {
                            sym_name[i..].len() > $symbol.len() && if &sym_name
                                [i..i + $symbol.len()]
                                == $symbol
                            {
                                i += $symbol.len();
                                true
                            } else {
                                false
                            }
                        };
                    }

                    // Print name string
                    let end = i + len;
                    while i < sym_name.len() && i < end {
                        if match_symbol!(b"$LT$") {
                            print!("<");
                        } else if match_symbol!(b"$GT$") {
                            print!(">");
                        } else if match_symbol!(b"$u27$") {
                            print!("'");
                        } else if match_symbol!(b"$u20$") {
                            print!(" ");
                        } else if match_symbol!(b"..") {
                            print!("::");
                        } else {
                            print!("{}", sym_name[i] as char);
                            i += 1;
                            continue;
                        }
                    }
                }
            } else {
                for &b in sym_name.iter() {
                    print!("{}", b as char);
                }
            }

            println!("");
        }
    }
}
