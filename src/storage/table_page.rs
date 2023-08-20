use crate::common::{config::BUSTUBX_PAGE_SIZE, rid::Rid};

use super::{
    page::PageId,
    tuple::{Tuple, TupleMeta},
};

pub const TABLE_PAGE_HEADER_SIZE: usize = 4 + 2 + 2;
pub const TABLE_PAGE_TUPLE_INFO_SIZE: usize = 2 + 2 + (4 + 4 + 4);

/**
 * Slotted page format:
 *  ---------------------------------------------------------
 *  | HEADER | ... FREE SPACE ... | ... INSERTED TUPLES ... |
 *  ---------------------------------------------------------
 *                                ^
 *                                free space pointer
 *
 *  Header format (size in bytes):
 *  ----------------------------------------------------------------------------
 *  | NextPageId (4)| NumTuples(2) | NumDeletedTuples(2) |
 *  ----------------------------------------------------------------------------
 *  ----------------------------------------------------------------
 *  | Tuple_1 offset+size (4) + TupleMeta(12) | Tuple_2 offset+size (4) + TupleMeta(12)  | ... |
 *  ----------------------------------------------------------------
 *
 */
pub struct TablePage {
    pub next_page_id: PageId,
    pub num_tuples: u16,
    pub num_deleted_tuples: u16,
    // (offset, size, meta)
    pub tuple_info: Vec<(u16, u16, TupleMeta)>,
    // 整个页原始数据
    // TODO 可以通过memmove、memcpy优化，参考bustub
    pub data: [u8; BUSTUBX_PAGE_SIZE],
}

impl TablePage {
    pub fn new(next_page_id: PageId) -> Self {
        Self {
            next_page_id,
            num_tuples: 0,
            num_deleted_tuples: 0,
            tuple_info: Vec::with_capacity(BUSTUBX_PAGE_SIZE / TABLE_PAGE_TUPLE_INFO_SIZE),
            data: [0; BUSTUBX_PAGE_SIZE],
        }
    }

    // Get the offset for the next tuple insertion.
    pub fn get_next_tuple_offset(&self, meta: &TupleMeta, tuple: &Tuple) -> Option<u16> {
        // Get the ending offset of the current slot. If there are inserted tuples,
        // get the offset of the previous inserted tuple; otherwise, set it to the size of the page.
        let slot_end_offset = if self.num_tuples > 0 {
            self.tuple_info[self.num_tuples as usize - 1].0
        } else {
            BUSTUBX_PAGE_SIZE as u16
        };

        // Check if the current slot has enough space for the new tuple. Return None if not.
        if slot_end_offset < tuple.data.len() as u16 {
            return None;
        }

        // Calculate the insertion offset for the new tuple by subtracting its data length
        // from the ending offset of the current slot.
        let tuple_offset = slot_end_offset - tuple.data.len() as u16;

        // Calculate the minimum valid tuple insertion offset, including the table page header size,
        // the total size of each tuple info (existing tuple infos and newly added tuple info).
        let min_tuple_offset = TABLE_PAGE_HEADER_SIZE as u16
            + (self.num_tuples as u16 + 1) * TABLE_PAGE_TUPLE_INFO_SIZE as u16;
        if tuple_offset < min_tuple_offset {
            return None;
        }

        // Return the calculated insertion offset for the new tuple.
        return Some(tuple_offset);
    }

    pub fn insert_tuple(&mut self, meta: &TupleMeta, tuple: &Tuple) -> Option<u16> {
        // Get the offset for the next tuple insertion.
        let tuple_offset = self.get_next_tuple_offset(meta, tuple)?;
        let tuple_id = self.num_tuples;

        // Store tuple information including offset, length, and metadata.
        self.tuple_info
            .push((tuple_offset, tuple.data.len() as u16, meta.clone()));

        // only check
        assert_eq!(tuple_id, self.tuple_info.len() as u16 - 1);

        self.num_tuples += 1;
        if meta.is_deleted {
            self.num_deleted_tuples += 1;
        }

        // Copy the tuple's data into the appropriate position within the page's data buffer.
        self.data[tuple_offset as usize..(tuple_offset + tuple.data.len() as u16) as usize]
            .copy_from_slice(&tuple.data);
        return Some(tuple_id);
    }

    pub fn update_tuple_meta(&mut self, meta: &TupleMeta, rid: &Rid) {
        let tuple_id = rid.slot_num;
        if tuple_id >= self.num_tuples as u32 {
            panic!("tuple_id {} out of range", tuple_id);
        }
        if meta.is_deleted && !self.tuple_info[tuple_id as usize].2.is_deleted {
            self.num_deleted_tuples += 1;
        }

        self.tuple_info[tuple_id as usize].2 = meta.clone();
    }

    pub fn get_tuple(&self, rid: &Rid) -> (TupleMeta, Tuple) {
        let tuple_id = rid.slot_num;
        if tuple_id >= self.num_tuples as u32 {
            panic!("tuple_id {} out of range", tuple_id);
        }

        let (offset, size, meta) = self.tuple_info[tuple_id as usize];
        let tuple = Tuple::new_with_rid(
            *rid,
            self.data[offset as usize..(offset + size) as usize].to_vec(),
        );

        return (meta, tuple);
    }

    pub fn get_tuple_meta(&self, rid: &Rid) -> TupleMeta {
        let tuple_id = rid.slot_num;
        if tuple_id >= self.num_tuples as u32 {
            panic!("tuple_id {} out of range", tuple_id);
        }

        return self.tuple_info[tuple_id as usize].2.clone();
    }

    pub fn get_next_rid(&self, rid: &Rid) -> Option<Rid> {
        // TODO 忽略删除的tuple
        let tuple_id = rid.slot_num;
        if tuple_id + 1 >= self.num_tuples as u32 {
            return None;
        }

        return Some(Rid::new(rid.page_id, tuple_id + 1));
    }

    // Parse real data from disk pages into memory pages.
    pub fn from_bytes(data: &[u8]) -> Self {
        let next_page_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let mut table_page = Self::new(next_page_id);
        table_page.num_tuples = u16::from_be_bytes([data[4], data[5]]);
        table_page.num_deleted_tuples = u16::from_be_bytes([data[6], data[7]]);

        for i in 0..table_page.num_tuples as usize {
            let offset = 8 + i * TABLE_PAGE_TUPLE_INFO_SIZE;
            let tuple_offset = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let tuple_size = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            let insert_txn_id = u32::from_be_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            let delete_txn_id = u32::from_be_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);
            let is_deleted = u32::from_be_bytes([
                data[offset + 12],
                data[offset + 13],
                data[offset + 14],
                data[offset + 15],
            ]) == 1;
            table_page.tuple_info.push((
                tuple_offset,
                tuple_size,
                TupleMeta {
                    insert_txn_id,
                    delete_txn_id,
                    is_deleted,
                },
            ));
        }

        table_page.data.copy_from_slice(data);

        return table_page;
    }

    pub fn to_bytes(&self) -> [u8; BUSTUBX_PAGE_SIZE] {
        let mut bytes = [0; BUSTUBX_PAGE_SIZE];
        bytes[0..4].copy_from_slice(&self.next_page_id.to_be_bytes());
        bytes[4..6].copy_from_slice(&self.num_tuples.to_be_bytes());
        bytes[6..8].copy_from_slice(&self.num_deleted_tuples.to_be_bytes());
        for i in 0..self.num_tuples as usize {
            let offset = 8 + i * TABLE_PAGE_TUPLE_INFO_SIZE;
            let (tuple_offset, tuple_size, meta) = self.tuple_info[i];
            bytes[offset..offset + 2].copy_from_slice(&tuple_offset.to_be_bytes());
            bytes[offset + 2..offset + 4].copy_from_slice(&tuple_size.to_be_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&meta.insert_txn_id.to_be_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&meta.delete_txn_id.to_be_bytes());
            let is_deleted = if meta.is_deleted { 1u32 } else { 0u32 };
            bytes[offset + 12..offset + 16].copy_from_slice(&is_deleted.to_be_bytes());
        }
        bytes[TABLE_PAGE_HEADER_SIZE + self.num_tuples as usize * TABLE_PAGE_TUPLE_INFO_SIZE..]
            .copy_from_slice(
                &self.data[TABLE_PAGE_HEADER_SIZE
                    + self.num_tuples as usize * TABLE_PAGE_TUPLE_INFO_SIZE..],
            );
        bytes
    }
}

mod tests {
    use crate::{
        common::{config::BUSTUBX_PAGE_SIZE, rid::Rid},
        storage::tuple::Tuple,
    };

    #[test]
    pub fn test_table_page_insert() {
        let mut table_page = super::TablePage::new(0);
        let meta = super::TupleMeta {
            insert_txn_id: 0,
            delete_txn_id: 0,
            is_deleted: false,
        };
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![1, 1, 1]));
        assert_eq!(tuple_id, Some(0));
        assert_eq!(table_page.num_tuples, 1);
        assert_eq!(table_page.num_deleted_tuples, 0);
        assert_eq!(table_page.tuple_info.len(), 1);
        assert_eq!(
            table_page.tuple_info[tuple_id.unwrap() as usize].0,
            BUSTUBX_PAGE_SIZE as u16 - 3
        );
        assert_eq!(table_page.tuple_info[tuple_id.unwrap() as usize].1, 3);
        assert_eq!(table_page.tuple_info[tuple_id.unwrap() as usize].2, meta);

        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![1, 1, 1]));
        assert_eq!(tuple_id, Some(1));
        assert_eq!(table_page.num_tuples, 2);
        assert_eq!(table_page.num_deleted_tuples, 0);
        assert_eq!(table_page.tuple_info.len(), 2);
        assert_eq!(
            table_page.tuple_info[tuple_id.unwrap() as usize].0,
            BUSTUBX_PAGE_SIZE as u16 - 3 - 3
        );
        assert_eq!(table_page.tuple_info[tuple_id.unwrap() as usize].1, 3);
        assert_eq!(table_page.tuple_info[tuple_id.unwrap() as usize].2, meta);
    }

    #[test]
    pub fn test_table_page_get_tuple() {
        let mut table_page = super::TablePage::new(0);
        let meta = super::TupleMeta {
            insert_txn_id: 0,
            delete_txn_id: 0,
            is_deleted: false,
        };
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![1, 1, 1]));
        assert_eq!(tuple_id, Some(0));
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![2, 2, 2]));
        assert_eq!(tuple_id, Some(1));
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![3, 3, 3]));
        assert_eq!(tuple_id, Some(2));

        let (tuple_meta, tuple) = table_page.get_tuple(&super::Rid::new(0, 0));
        assert_eq!(tuple_meta, meta);
        assert_eq!(tuple.data, vec![1, 1, 1]);
        let (tuple_meta, tuple) = table_page.get_tuple(&super::Rid::new(0, 1));
        assert_eq!(tuple.data, vec![2, 2, 2]);
        let (tuple_meta, tuple) = table_page.get_tuple(&super::Rid::new(0, 2));
        assert_eq!(tuple.data, vec![3, 3, 3]);
    }

    #[test]
    pub fn test_table_page_update_tuple_meta() {
        let mut table_page = super::TablePage::new(0);
        let meta = super::TupleMeta {
            insert_txn_id: 0,
            delete_txn_id: 0,
            is_deleted: false,
        };
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![1, 1, 1]));
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![2, 2, 2]));
        let tuple_id = table_page.insert_tuple(&meta, &Tuple::new(vec![3, 3, 3]));

        let mut tuple_meta = table_page.get_tuple_meta(&super::Rid::new(0, 0));
        tuple_meta.is_deleted = true;
        tuple_meta.delete_txn_id = 1;
        tuple_meta.insert_txn_id = 2;

        table_page.update_tuple_meta(&tuple_meta, &super::Rid::new(0, 0));
        let tuple_meta = table_page.get_tuple_meta(&super::Rid::new(0, 0));
        assert_eq!(tuple_meta.is_deleted, true);
        assert_eq!(tuple_meta.delete_txn_id, 1);
        assert_eq!(tuple_meta.insert_txn_id, 2);
    }

    #[test]
    pub fn test_table_page_from_to_bytes() {
        let mut table_page = super::TablePage::new(1);
        let meta = super::TupleMeta {
            insert_txn_id: 0,
            delete_txn_id: 0,
            is_deleted: false,
        };
        let tuple_id1 = table_page.insert_tuple(&meta, &Tuple::new(vec![1, 1, 1]));
        let tuple_id2 = table_page.insert_tuple(&meta, &Tuple::new(vec![2, 2, 2]));
        let tuple_id3 = table_page.insert_tuple(&meta, &Tuple::new(vec![3, 3, 3]));

        let bytes = table_page.to_bytes();
        let table_page2 = super::TablePage::from_bytes(&bytes);
        assert_eq!(table_page2.next_page_id, 1);
        assert_eq!(table_page2.num_tuples, 3);
        assert_eq!(table_page2.num_deleted_tuples, 0);
        assert_eq!(table_page2.tuple_info.len(), 3);
        assert_eq!(table_page2.tuple_info[0].0, BUSTUBX_PAGE_SIZE as u16 - 3);
        assert_eq!(table_page2.tuple_info[0].1, 3);
        assert_eq!(table_page2.tuple_info[0].2, meta);
        assert_eq!(
            table_page2.tuple_info[1].0,
            BUSTUBX_PAGE_SIZE as u16 - 3 - 3
        );
        assert_eq!(table_page2.tuple_info[1].1, 3);
        assert_eq!(table_page2.tuple_info[1].2, meta);
        assert_eq!(
            table_page2.tuple_info[2].0,
            BUSTUBX_PAGE_SIZE as u16 - 3 - 3 - 3
        );
        assert_eq!(table_page2.tuple_info[2].1, 3);
        assert_eq!(table_page2.tuple_info[2].2, meta);

        let (tuple_meta, tuple) = table_page2.get_tuple(&Rid::new(0, tuple_id2.unwrap() as u32));
        assert_eq!(tuple_meta, meta);
        assert_eq!(tuple.data, vec![2, 2, 2]);
    }
}
