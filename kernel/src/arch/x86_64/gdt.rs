use core::ptr;
use x86_64::instructions::segmentation::*;
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::DescriptorFlags as F;
use x86_64::structures::gdt::*;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::{PrivilegeLevel, VirtAddr};

static mut INIT_GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();
pub const GDT_KERNEL_CODE: SegmentSelector = SegmentSelector::new(1, PrivilegeLevel::Ring0);
pub const GDT_KERNEL_DATA: SegmentSelector = SegmentSelector::new(2, PrivilegeLevel::Ring0);
pub const GDT_USER_CODE: usize = 4;
pub const GDT_USER_DATA: usize = 5;
pub const GDT_USER_TLS: usize = 6;

#[thread_local]
pub static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();

#[thread_local]
pub static mut TSS: TaskStateSegment = TaskStateSegment::new();

#[cfg(feature = "pti")]
pub unsafe fn set_tss_stack(stack: usize) {
    use arch::x86_64::pti::{PTI_CONTEXT_STACK, PTI_CPU_STACK};
    TSS.rsp[0] = (PTI_CPU_STACK.as_ptr() as usize + PTI_CPU_STACK.len()) as u64;
    PTI_CONTEXT_STACK = stack;
}

#[cfg(not(feature = "pti"))]
pub unsafe fn set_tss_stack(stack: usize) {
    TSS.privilege_stack_table[0] = VirtAddr::new(stack as u64);
}

macro_rules! descriptor {
    ($flags:expr) => {
        (Descriptor::UserSegment($flags.bits()))
    };
}

pub unsafe fn init() {
    INIT_GDT = GlobalDescriptorTable::new();
    let gdt = &mut INIT_GDT;
    let cs = gdt.add_entry(Descriptor::kernel_code_segment());
    let ds = gdt.add_entry(Descriptor::kernel_data_segment());
    // Load the initial GDT, before we have access to thread locals
    gdt.load();

    // Load the segment descriptors
    set_cs(cs);
    load_ds(ds);
    load_es(ds);
    load_fs(ds);
    load_gs(ds);
    load_ss(ds);
}

pub unsafe fn init_paging(tcb_offset: usize, stack_offset: usize) {
    println!("Setting up TLS");
    let gdt = &mut INIT_GDT;
    let fs = gdt.add_entry(Descriptor::tcb_segment(tcb_offset as u64));
    // Load the initial GDT, before we have access to thread locals
    gdt.load();
    load_fs(fs);

    println!("Enabled Kernel TLS without explosion");

    GDT = gdt.clone();

    let gdt = &mut GDT;

    gdt.add_entry(descriptor!(
        F::PRESENT | F::USER_SEGMENT | F::EXECUTABLE | F::LONG_MODE | F::RW | F::Ring3
    ));
    gdt.add_entry(descriptor!(
        F::PRESENT | F::USER_SEGMENT | F::LONG_MODE | F::RW | F::Ring3
    ));
    gdt.add_entry(descriptor!(
        F::PRESENT | F::USER_SEGMENT | F::LONG_MODE | F::RW | F::Ring3
    ));
    let tss = gdt.add_entry(Descriptor::tss_segment(&TSS));

    set_tss_stack(stack_offset);

    GDT.load();

    set_cs(GDT_KERNEL_CODE);
    load_ds(GDT_KERNEL_DATA);
    load_es(GDT_KERNEL_DATA);
    load_fs(fs);
    load_gs(GDT_KERNEL_DATA);
    load_ss(GDT_KERNEL_DATA);

    load_tss(tss);
}
