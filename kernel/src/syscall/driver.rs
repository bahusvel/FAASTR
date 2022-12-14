use context;
use context::memory::Grant;
use interrupt::syscall::SyscallStack;
use memory::{allocate_frames, deallocate_frames, Frame};
use paging::entry::EntryFlags;
use paging::{ActivePageTable, PhysicalAddress, VirtualAddress};
use syscall::error::{Error, Result, EFAULT, EINVAL, ENOMEM, ESRCH};
use syscall::flag::{MAP_WRITE, MAP_WRITE_COMBINE};

fn enforce_root() -> Result<()> {
    // Do some kind of system permision checking
    Ok(())
}

pub fn iopl(level: usize, stack: &mut SyscallStack) -> Result<usize> {
    enforce_root()?;

    if level > 3 {
        return Err(Error::new(EINVAL));
    }

    stack.rflags = (stack.rflags & !(3 << 12)) | ((level & 3) << 12);

    Ok(0)
}

pub fn physalloc(size: usize) -> Result<usize> {
    enforce_root()?;

    allocate_frames((size + 4095) / 4096)
        .ok_or(Error::new(ENOMEM))
        .map(|frame| frame.start_address().get())
}

pub fn physfree(physical_address: usize, size: usize) -> Result<usize> {
    enforce_root()?;

    deallocate_frames(
        Frame::containing_address(PhysicalAddress::new(physical_address)),
        (size + 4095) / 4096,
    );
    //TODO: Check that no double free occured
    Ok(0)
}

//TODO: verify exlusive access to physical memory
pub fn physmap(physical_address: usize, size: usize, flags: usize) -> Result<usize> {
    enforce_root()?;

    if size == 0 {
        Ok(0)
    } else {
        let contexts = context::contexts();
        let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
        let mut context = context_lock.write();

        let grants = &mut context.grants;

        let from_address = (physical_address / 4096) * 4096;
        let offset = physical_address - from_address;
        let full_size = ((offset + size + 4095) / 4096) * 4096;
        let mut to_address = ::USER_GRANT_OFFSET;

        let mut entry_flags =
            EntryFlags::PRESENT | EntryFlags::NO_EXECUTE | EntryFlags::USER_ACCESSIBLE;
        if flags & MAP_WRITE == MAP_WRITE {
            entry_flags |= EntryFlags::WRITABLE;
        }
        if flags & MAP_WRITE_COMBINE == MAP_WRITE_COMBINE {
            entry_flags |= EntryFlags::HUGE_PAGE;
        }

        for i in 0..grants.len() {
            let start = grants[i].start_address().get();
            if to_address + full_size < start {
                grants.insert(
                    i,
                    Grant::physmap(
                        PhysicalAddress::new(from_address),
                        VirtualAddress::new(to_address),
                        full_size,
                        entry_flags,
                    ),
                );

                return Ok(to_address + offset);
            } else {
                let pages = (grants[i].size() + 4095) / 4096;
                let end = start + pages * 4096;
                to_address = end;
            }
        }

        grants.push(Grant::physmap(
            PhysicalAddress::new(from_address),
            VirtualAddress::new(to_address),
            full_size,
            entry_flags,
        ));

        Ok(to_address + offset)
    }
}

pub fn physunmap(virtual_address: usize) -> Result<usize> {
    enforce_root()?;

    if virtual_address == 0 {
        Ok(0)
    } else {
        let contexts = context::contexts();
        let context_lock = contexts.current().ok_or(Error::new(ESRCH))?;
        let mut context = context_lock.write();

        let grants = &mut context.grants;

        for i in 0..grants.len() {
            let start = grants[i].start_address().get();
            let end = start + grants[i].size();
            if virtual_address >= start && virtual_address < end {
                grants.remove(i).unmap();

                return Ok(0);
            }
        }

        Err(Error::new(EFAULT))
    }
}

pub fn virttophys(virtual_address: usize) -> Result<usize> {
    enforce_root()?;

    let active_table = unsafe { ActivePageTable::new() };
    match active_table.translate(VirtualAddress::new(virtual_address)) {
        Some(physical_address) => Ok(physical_address.get()),
        None => Err(Error::new(EFAULT)),
    }
}
