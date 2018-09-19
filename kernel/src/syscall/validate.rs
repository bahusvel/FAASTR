use core::{mem, slice};

use paging::entry::EntryFlags;
use paging::{ActivePageTable, Page, VirtualAddress};
use sos::JustError;

fn validate(address: usize, size: usize, flags: EntryFlags) -> Result<(), JustError<'static>> {
    let end_offset = size
        .checked_sub(1)
        .ok_or(JustError::new("Invalid memory address"))?;
    let end_address = address
        .checked_add(end_offset)
        .ok_or(JustError::new("Invalid memory address"))?;

    let active_table = unsafe { ActivePageTable::new() };

    let start_page = Page::containing_address(VirtualAddress::new(address));
    let end_page = Page::containing_address(VirtualAddress::new(end_address));
    for page in Page::range_inclusive(start_page, end_page) {
        if let Some(page_flags) = active_table.translate_page_flags(page) {
            if !page_flags.contains(flags) {
                //println!("{:X}: Not {:?}", page.start_address().get(), flags);
                return Err(JustError::new("Invalid memory address"));
            }
        } else {
            //println!("{:X}: Not found", page.start_address().get());
            return Err(JustError::new("Invalid memory address"));
        }
    }

    Ok(())
}

/// Convert a pointer and length to slice, if valid
pub fn validate_slice<T>(ptr: *const T, len: usize) -> Result<&'static [T], JustError<'static>> {
    if len == 0 {
        Ok(&[])
    } else {
        validate(
            ptr as usize,
            len * mem::size_of::<T>(),
            EntryFlags::PRESENT | EntryFlags::USER_ACCESSIBLE,
        )?;
        Ok(unsafe { slice::from_raw_parts(ptr, len) })
    }
}

/// Convert a pointer and length to slice, if valid
pub fn validate_slice_mut<T>(
    ptr: *mut T,
    len: usize,
) -> Result<&'static mut [T], JustError<'static>> {
    if len == 0 {
        Ok(&mut [])
    } else {
        validate(
            ptr as usize,
            len * mem::size_of::<T>(),
            EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::USER_ACCESSIBLE,
        )?;
        Ok(unsafe { slice::from_raw_parts_mut(ptr, len) })
    }
}
