///       VM Layout
/// +-------------------+
/// | Recursive Mapping | PML4=511
/// +-------------------+
/// |    Kernel Image   | PML4=510
/// +-------------------+
/// |    Kernel Heap    | PML4=509
/// +-------------------+
/// |   Kernel Valloc   | PML4=508
/// +-------------------+
/// |     Kernel TLS    | PML4=507
/// +-------------------+
///  ===================
///          Gap
///  ===================
///  ===================
///      Temporaries
///  ===================
/// +-------------------+
/// |     User Stack    | PML4=3 0x0000_0180_0000_0000 - 0x0000_0200_0000_0000
/// +-------------------+
/// |    User Grants    | PML4=2 0x0000_0100_0000_0000 - 0x0000_0180_0000_0000
/// +-------------------+
/// |     User Heap     | PML4=1 0x0000_0080_0000_0000 - 0x0000_0100_0000_0000
/// +-------------------+
/// |     User Image    | PML4=0 0x0000_0000_0000_0000 - 0x0000_0080_0000_0000
/// +-------------------+

// Because the memory map is so important to not be aliased, it is defined here, in one place
// Each PML4 entry references up to 512 GB of memory
// The top (511) PML4 is reserved for recursive mapping
// The second from the top (510) PML4 is reserved for the kernel
/// The size of a single PML4
pub const PML4_SIZE: usize = 0x0000_0080_0000_0000;
pub const PML4_MASK: usize = 0x0000_ff80_0000_0000;

/// Offset of recursive paging
pub const RECURSIVE_PAGE_OFFSET: usize = (-(PML4_SIZE as isize)) as usize;
pub const RECURSIVE_PAGE_PML4: usize = (RECURSIVE_PAGE_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset of kernel
pub const KERNEL_OFFSET: usize = RECURSIVE_PAGE_OFFSET - PML4_SIZE;
pub const KERNEL_PML4: usize = (KERNEL_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to kernel heap
pub const KERNEL_HEAP_OFFSET: usize = KERNEL_OFFSET - PML4_SIZE;
pub const KERNEL_HEAP_PML4: usize = (KERNEL_HEAP_OFFSET & PML4_MASK) / PML4_SIZE;
/// Size of kernel heap
pub const KERNEL_HEAP_SIZE: usize = 1 * 1024 * 1024; // 1 MB

pub const KERNEL_VALLOC_OFFSET: usize = KERNEL_HEAP_OFFSET - PML4_SIZE;
pub const KERNEL_VALLOC_PML4: usize = (KERNEL_VALLOC_OFFSET & PML4_MASK) / PML4_SIZE;
pub const KERNEL_VALLOC_SIZE: usize = PML4_SIZE;

/// Offset to kernel percpu variables
//TODO: Use 64-bit fs offset to enable this
pub const KERNEL_PERCPU_OFFSET: usize = KERNEL_VALLOC_OFFSET - PML4_SIZE;
pub const KERNEL_PERCPU_PML4: usize = (KERNEL_PERCPU_OFFSET & PML4_MASK) / PML4_SIZE;
//pub const KERNEL_PERCPU_OFFSET: usize = 0xC000_0000;
/// Size of kernel percpu variables
pub const KERNEL_PERCPU_SIZE: usize = 64 * 1024; // 64 KB

/// Offset to user image
pub const USER_OFFSET: usize = 0;
pub const USER_PML4: usize = (USER_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to user arguments
pub const USER_ARG_OFFSET: usize = USER_OFFSET + PML4_SIZE / 2;

/// Offset to user heap
pub const USER_HEAP_OFFSET: usize = USER_OFFSET + PML4_SIZE;
pub const USER_HEAP_PML4: usize = (USER_HEAP_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to user grants
pub const USER_GRANT_OFFSET: usize = USER_HEAP_OFFSET + PML4_SIZE;
pub const USER_GRANT_PML4: usize = (USER_GRANT_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to user stack
pub const USER_STACK_OFFSET: usize = USER_GRANT_OFFSET + PML4_SIZE;
pub const USER_STACK_PML4: usize = (USER_STACK_OFFSET & PML4_MASK) / PML4_SIZE;
/// Size of user stack
pub const USER_STACK_SIZE: usize = 1024 * 1024; // 1 MB

/// Offset to user temporary image (used when cloning)
pub const USER_TMP_OFFSET: usize = USER_STACK_OFFSET + PML4_SIZE;
pub const USER_TMP_PML4: usize = (USER_TMP_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to user temporary heap (used when cloning)
pub const USER_TMP_HEAP_OFFSET: usize = USER_TMP_OFFSET + PML4_SIZE;
pub const USER_TMP_HEAP_PML4: usize = (USER_TMP_HEAP_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to user temporary page for grants
pub const USER_TMP_GRANT_OFFSET: usize = USER_TMP_HEAP_OFFSET + PML4_SIZE;
pub const USER_TMP_GRANT_PML4: usize = (USER_TMP_GRANT_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset to user temporary stack (used when cloning)
pub const USER_TMP_STACK_OFFSET: usize = USER_TMP_GRANT_OFFSET + PML4_SIZE;
pub const USER_TMP_STACK_PML4: usize = (USER_TMP_STACK_OFFSET & PML4_MASK) / PML4_SIZE;

/// Offset for usage in other temporary pages
pub const USER_TMP_MISC_OFFSET: usize = USER_TMP_STACK_OFFSET + PML4_SIZE;
pub const USER_TMP_MISC_PML4: usize = (USER_TMP_MISC_OFFSET & PML4_MASK) / PML4_SIZE;
