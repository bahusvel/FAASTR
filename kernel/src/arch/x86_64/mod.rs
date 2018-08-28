#[macro_use]
pub mod macros;

/// Debugging support
pub mod debug;

/// Devices
pub mod device;

/// Global descriptor table
pub mod gdt;
//pub mod ngdt;

/// Graphical debug
#[cfg(feature = "graphical_debug")]
mod graphical_debug;

/// Interrupt descriptor table
pub mod idt;

/// Interrupt instructions
pub mod interrupt;

/// Paging
pub mod paging;

/// Page table isolation
pub mod pti;

/// Initialization and start function
pub mod start;

/// Stop function
pub mod stop;
