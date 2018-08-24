use alloc::Vec;
use arch::paging::{Page, PageIter, VirtualAddress, PAGE_SIZE};

/// Allocator that doesnt actually allocate memory but rather virtual memory pages
pub struct Valloc {
    free: Vec<(usize, usize)>,
}

impl Valloc {
    pub fn new(start: usize, size: usize) -> Self {
        Valloc {
            free: vec![(start, size / PAGE_SIZE)],
        }
    }

    fn free_count(&self) -> usize {
        let mut count = 0;
        for free in self.free.iter() {
            count += free.1;
        }
        count
    }

    fn merge(&mut self, address: usize, count: usize) -> bool {
        for i in 0..self.free.len() {
            let changed = {
                let free = &mut self.free[i];
                if address + count * PAGE_SIZE == free.0 {
                    free.0 = address;
                    free.1 += count;
                    true
                } else if free.0 + free.1 * PAGE_SIZE == address {
                    free.1 += count;
                    true
                } else {
                    false
                }
            };

            if changed {
                //TODO: Do not use recursion
                let (address, count) = self.free[i];
                if self.merge(address, count) {
                    self.free.remove(i);
                }
                return true;
            }
        }

        false
    }

    pub fn allocate_pages(&mut self, count: usize) -> Option<Page> {
        let mut best_fit = None;
        let mut best_fit_index = 0;

        for i in 0..self.free.len() {
            let block = self.free[i];
            if block.1 < count {
                continue;
            }
            if best_fit.is_none() {
                best_fit = Some(block);
                best_fit_index = i;
            }
            if block.1 < best_fit.unwrap().1 {
                best_fit = Some(block);
                best_fit_index = i;
            }
            if best_fit.unwrap().1 == count {
                break;
            }
        }

        if let Some(best_fit) = best_fit {
            if best_fit.1 > count {
                let new_block = (best_fit.0, best_fit.1 - count);
                self.free.push(new_block);
            }
            self.free.swap_remove(best_fit_index);

            let start = best_fit.0 + (best_fit.1 - count) * PAGE_SIZE;
            let end = start + count * PAGE_SIZE;

            println!("Valloced 0x{:X}", start);

            Some(Page::containing_address(VirtualAddress::new(start)))
        } else {
            None
        }
    }

    pub fn deallocate_pages(&mut self, page: Page, count: usize) {
        println!("Unvalloced 0x{:X}", page.start_address().get());
        let address = page.start_address().get();
        if !self.merge(address, count) {
            self.free.push((address, count));
        }
    }
}
