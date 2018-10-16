use core::mem;
use core::sync::atomic::{AtomicBool, ATOMIC_BOOL_INIT};
use gdt;

/// This must be used by the kernel to ensure that context switches are done atomically
/// Compare and exchange this to true when beginning a context switch on any CPU
/// The `Context::switch_to` function will set it back to false, allowing other CPU's to switch
/// This must be done, as no locks can be held on the stack during switch
pub static CONTEXT_SWITCH_LOCK: AtomicBool = ATOMIC_BOOL_INIT;

#[derive(Clone, Debug)]
pub struct Context {
    /// FX valid?
    loadable: bool,
    /// FX location
    fx: usize,
    /// Page table pointer
    cr3: usize,
    /// RFLAGS register
    rflags: usize,
    /// RBX register
    rbx: usize,
    /// R12 register
    r12: usize,
    /// R13 register
    r13: usize,
    /// R14 register
    r14: usize,
    /// R15 register
    r15: usize,
    /// Base pointer
    rbp: usize,
    /// Stack pointer
    rsp: usize,
}

macro_rules! store {
        ($self:expr) => {
            asm!("fxsave [$0]" : : "r"($self.fx) : "memory" : "intel", "volatile");
            $self.loadable = true;
            asm!("mov $0, cr3
            pushfq ; pop $1
            mov $2, rbx
            mov $3, r12
            mov $4, r13
            mov $5, r14
            mov $6, r15
            mov $7, rsp
            mov $8, rbp"
            :"=r"($self.cr3), "=r"($self.rflags), "=r"($self.rbx), "=r"($self.r12), "=r"($self.r13),"=r"($self.r14),"=r"($self.r15), "=r"($self.rsp), "=r"($self.rbp)
            : // No input
            : "memory"
            : "intel", "volatile");
    };
}

macro_rules! load {
    ($next:expr, $cr3_bool:expr) => {
        if $next.loadable {
            asm!("fxrstor [$0]" : : "r"($next.fx) : "memory" : "intel", "volatile");
        } else {
            asm!("fninit" : : : "memory" : "intel", "volatile");
        }
        if $cr3_bool {
            asm!("mov cr3, $0" : : "r"($next.cr3) : "memory" : "intel", "volatile");
        }
        asm!("push $0 ; popfq
            mov rbx, $1
            mov r12, $2
            mov r13, $3
            mov r14, $4
            mov r15, $5
            mov rsp, $6
            mov rbp, $7"
        : // No output
        : "r"($next.rflags), "r"($next.rbx), "r"($next.r12), "r"($next.r13), "r"($next.r14), "r"($next.r15), "r"($next.rsp), "r"($next.rbp)
        : "memory"
        : "intel", "volatile");
    };
}

impl Context {
    pub fn new() -> Context {
        Context {
            loadable: false,
            fx: 0,
            cr3: 0,
            rflags: 0,
            rbx: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rbp: 0,
            rsp: 0,
        }
    }

    pub fn get_page_table(&self) -> usize {
        self.cr3
    }

    pub fn set_fx(&mut self, address: usize) {
        self.fx = address;
    }

    pub fn set_page_table(&mut self, address: usize) {
        self.cr3 = address;
    }

    pub fn set_stack(&mut self, address: usize) {
        self.rsp = address;
    }

    pub fn get_stack(&self) -> usize {
        self.rsp
    }

    pub unsafe fn signal_stack(&mut self, handler: extern "C" fn(usize), sig: u8) {
        self.push_stack(sig as usize);
        self.push_stack(handler as usize);
        self.push_stack(signal_handler_wrapper as usize);
    }

    pub unsafe fn push_stack(&mut self, value: usize) {
        self.rsp -= mem::size_of::<usize>();
        *(self.rsp as *mut usize) = value;
    }

    pub unsafe fn pop_stack(&mut self) -> usize {
        let value = *(self.rsp as *const usize);
        self.rsp += mem::size_of::<usize>();
        value
    }

    /// Switch to the next context by restoring its stack and registers
    #[cold]
    #[inline(never)]
    #[naked]
    pub unsafe fn switch_to(&mut self, next: &mut Context) {
        store!(self);
        load!(next, self.cr3 != next.cr3);
    }

    #[cold]
    #[inline(never)]
    #[naked]
    pub unsafe fn switch_discarding(&self, next: &mut Context) {
        load!(next, next.cr3 != self.cr3);
    }

    #[cold]
    #[inline(never)]
    #[naked]
    pub unsafe fn switch_user(&mut self, next: &mut Context, ip: usize, sp: usize, arg: usize) {
        store!(self);

        if next.cr3 != self.cr3 {
            asm!("mov cr3, $0" : : "r"(next.cr3) : "memory" : "intel", "volatile");
        }

        // Push some config stuff on the stack
        asm!("push r10
              push r11
              push r12
              push r13
              push r14
              push r15"
              : // No output
              :   "{r10}"(gdt::GDT_USER_DATA << 3 | 3), // Data segment
                  "{r11}"(sp), // Stack pointer
                  "{r12}"(1 << 9), // Flags - Set interrupt enable flag
                  "{r13}"(gdt::GDT_USER_CODE << 3 | 3), // Code segment
                  "{r14}"(ip), // IP
                  "{r15}"(arg) // Argument
              : // No clobbers
              : "intel", "volatile");

        asm!("mov ds, r14d
             mov es, r14d
             mov fs, r15d
             mov gs, r14d
             xor rbx, rbx
             xor rcx, rcx
             xor rdx, rdx
             xor rsi, rsi
             xor rdi, rdi
             xor rbp, rbp
             xor r8, r8
             xor r9, r9
             xor r10, r10
             xor r11, r11
             xor r12, r12
             xor r13, r13
             xor r14, r14
             xor r15, r15
             fninit
             pop rdi
             iretq"
             : // No output because it never returns
             :   "{r14}"(gdt::GDT_USER_DATA << 3 | 3), // Data segment
                 "{r15}"(gdt::GDT_NULL << 3 | 3) // TLS segment
             : // No clobbers because it never returns
             : "intel", "volatile");
        unreachable!();
    }
}

#[allow(dead_code)]
#[repr(packed)]
pub struct SignalHandlerStack {
    r11: usize,
    r10: usize,
    r9: usize,
    r8: usize,
    rsi: usize,
    rdi: usize,
    rdx: usize,
    rcx: usize,
    rax: usize,
    handler: extern "C" fn(usize),
    sig: usize,
    rip: usize,
}

#[naked]
unsafe extern "C" fn signal_handler_wrapper() {
    #[inline(never)]
    unsafe fn inner(stack: &SignalHandlerStack) {
        (stack.handler)(stack.sig);
    }

    // Push scratch registers
    asm!("push rax
        push rcx
        push rdx
        push rdi
        push rsi
        push r8
        push r9
        push r10
        push r11"
        : : : : "intel", "volatile");

    // Get reference to stack variables
    let rsp: usize;
    asm!("" : "={rsp}"(rsp) : : : "intel", "volatile");

    // Call inner rust function
    inner(&*(rsp as *const SignalHandlerStack));

    // Pop scratch registers, error code, and return
    asm!("pop r11
        pop r10
        pop r9
        pop r8
        pop rsi
        pop rdi
        pop rdx
        pop rcx
        pop rax
        add rsp, 16"
        : : : : "intel", "volatile");
}
