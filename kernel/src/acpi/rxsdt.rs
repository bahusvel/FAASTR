use alloc::boxed::Box;

use paging::ActivePageTable;

use super::get_sdt;
use super::sdt::Sdt;

pub trait Rxsdt {
    fn iter(&self) -> Box<Iterator<Item = usize>>;

    fn map_all(&self, active_table: &mut ActivePageTable) {
        for sdt in self.iter() {
            get_sdt(sdt, active_table);
        }
    }

    fn find(
        &self,
        signature: [u8; 4],
        oem_id: [u8; 6],
        oem_table_id: [u8; 8],
    ) -> Option<&'static Sdt> {
        for sdt in self.iter() {
            let sdt = unsafe { &*(sdt as *const Sdt) };

            if sdt.match_pattern(signature, oem_id, oem_table_id) {
                return Some(sdt);
            }
        }

        None
    }
}
