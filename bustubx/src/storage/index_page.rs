use super::Tuple;
use crate::buffer::{PageId, INVALID_PAGE_ID};
use crate::catalog::SchemaRef;
use crate::{catalog::Schema, common::rid::Rid};

pub const INTERNAL_PAGE_HEADER_SIZE: usize = 4 + 4 + 4;
pub const LEAF_PAGE_HEADER_SIZE: usize = 4 + 4 + 4 + 4;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BPlusTreePage {
    // B+树内部节点页
    Internal(BPlusTreeInternalPage),
    // B+树叶子节点页
    Leaf(BPlusTreeLeafPage),
}
impl BPlusTreePage {
    pub fn is_leaf(&self) -> bool {
        match self {
            Self::Internal(_) => false,
            Self::Leaf(_) => true,
        }
    }
    pub fn is_full(&self) -> bool {
        match self {
            Self::Internal(page) => page.is_full(),
            Self::Leaf(page) => page.is_full(),
        }
    }
    pub fn is_underflow(&self, is_root: bool) -> bool {
        if is_root {
            return false;
        }
        match self {
            Self::Internal(page) => page.header.current_size < page.min_size(),
            Self::Leaf(page) => page.header.current_size < page.min_size(),
        }
    }
    pub fn insert_internalkv(&mut self, internalkv: InternalKV, key_schema: &Schema) {
        match self {
            Self::Internal(page) => page.insert(internalkv.0, internalkv.1, key_schema),
            Self::Leaf(_) => panic!("Leaf page cannot insert InternalKV"),
        }
    }
    pub fn can_borrow(&self) -> bool {
        match self {
            Self::Internal(page) => page.header.current_size > page.min_size(),
            Self::Leaf(page) => page.header.current_size > page.min_size(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BPlusTreePageType {
    LeafPage,
    InternalPage,
}

pub type InternalKV = (Tuple, PageId);
pub type LeafKV = (Tuple, Rid);

/**
 * Internal page format (keys are stored in increasing order):
 *  --------------------------------------------------------------------------
 * | HEADER | KEY(1)+PAGE_ID(1) | KEY(2)+PAGE_ID(2) | ... | KEY(n)+PAGE_ID(n) |
 *  --------------------------------------------------------------------------
 *
 * Header format (size in byte, 12 bytes in total):
 * ----------------------------------------------------------------------------
 * | PageType (4) | CurrentSize (4) | MaxSize (4) |
 * ----------------------------------------------------------------------------
 */
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BPlusTreeInternalPage {
    pub schema: SchemaRef,
    pub header: BPlusTreeInternalPageHeader,
    // 第一个key为空，n个key对应n+1个value
    pub array: Vec<InternalKV>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BPlusTreeInternalPageHeader {
    pub page_type: BPlusTreePageType,
    pub current_size: u32,
    // max kv size can be stored
    pub max_size: u32,
}

impl BPlusTreeInternalPage {
    pub fn new(schema: SchemaRef, max_size: u32) -> Self {
        Self {
            schema,
            header: BPlusTreeInternalPageHeader {
                page_type: BPlusTreePageType::InternalPage,
                current_size: 0,
                max_size,
            },
            array: Vec::with_capacity(max_size as usize),
        }
    }
    pub fn min_size(&self) -> u32 {
        self.header.max_size / 2
    }
    pub fn key_at(&self, index: usize) -> &Tuple {
        &self.array[index].0
    }
    pub fn value_at(&self, index: usize) -> PageId {
        self.array[index].1
    }
    pub fn values(&self) -> Vec<PageId> {
        self.array.iter().map(|kv| kv.1).collect()
    }

    pub fn sibling_page_ids(&self, page_id: PageId) -> (Option<PageId>, Option<PageId>) {
        let index = self.array.iter().position(|x| x.1 == page_id);
        if let Some(index) = index {
            return (
                if index == 0 {
                    None
                } else {
                    Some(self.array[index - 1].1)
                },
                if index == self.header.current_size as usize - 1 {
                    None
                } else {
                    Some(self.array[index + 1].1)
                },
            );
        }
        return (None, None);
    }

    // TODO 可以通过二分查找来插入
    pub fn insert(&mut self, key: Tuple, page_id: PageId, key_schema: &Schema) {
        if self.header.current_size == 0 && !key.is_null() {
            panic!("First key must be zero");
        }
        self.array.push((key, page_id));
        self.header.current_size += 1;
        // 跳过第一个空key
        let null_kv = self.array.remove(0);
        self.array.sort_by(|a, b| a.0.compare(&b.0, key_schema));
        self.array.insert(0, null_kv);
    }
    pub fn batch_insert(&mut self, kvs: Vec<InternalKV>, key_schema: &Schema) {
        let kvs_len = kvs.len();
        self.array.extend(kvs);
        self.header.current_size += kvs_len as u32;
        self.array.sort_by(|a, b| a.0.compare(&b.0, key_schema));
    }

    pub fn delete(&mut self, key: &Tuple, key_schema: &Schema) {
        if self.header.current_size == 0 {
            return;
        }
        // 第一个key为空，所以从1开始
        let mut start: i32 = 1;
        let mut end: i32 = self.header.current_size as i32 - 1;
        while start < end {
            let mid = (start + end) / 2;
            let compare_res = key.compare(&self.array[mid as usize].0, key_schema);
            if compare_res == std::cmp::Ordering::Equal {
                self.array.remove(mid as usize);
                self.header.current_size -= 1;
                // 删除后，如果只剩下一个空key，那么删除
                if self.header.current_size == 1 {
                    self.array.remove(0);
                    self.header.current_size -= 1;
                }
                return;
            } else if compare_res == std::cmp::Ordering::Less {
                end = mid - 1;
            } else {
                start = mid + 1;
            }
        }
        if key.compare(&self.array[start as usize].0, key_schema) == std::cmp::Ordering::Equal {
            self.array.remove(start as usize);
            self.header.current_size -= 1;
            // 删除后，如果只剩下一个空key，那么删除
            if self.header.current_size == 1 {
                self.array.remove(0);
                self.header.current_size -= 1;
            }
            return;
        }
        return;
    }

    pub fn delete_page_id(&mut self, page_id: PageId) {
        if self.header.current_size == 0 {
            return;
        }
        for i in 0..self.header.current_size {
            if self.array[i as usize].1 == page_id {
                if i == 0 {
                    self.array.remove(0);
                    self.header.current_size -= 1;
                    // 把第一个key置空
                    self.array[0].0 = Tuple::empty(self.schema.clone());
                } else {
                    self.array.remove(i as usize);
                    self.header.current_size -= 1;
                }
                // 删除后，如果只剩下一个空key，那么删除
                if self.header.current_size == 1 {
                    self.array.remove(0);
                    self.header.current_size -= 1;
                }
                return;
            }
        }
    }

    pub fn is_full(&self) -> bool {
        self.header.current_size > self.header.max_size
    }

    pub fn split_off(&mut self, at: usize) -> Vec<InternalKV> {
        let new_array = self.array.split_off(at);
        self.header.current_size -= new_array.len() as u32;
        return new_array;
    }

    pub fn reverse_split_off(&mut self, at: usize) -> Vec<InternalKV> {
        let mut new_array = Vec::new();
        for _ in 0..=at {
            new_array.push(self.array.remove(0));
        }
        self.header.current_size -= new_array.len() as u32;
        return new_array;
    }

    pub fn replace_key(&mut self, old_key: &Tuple, new_key: Tuple, key_schema: &Schema) {
        let key_index = self.key_index(old_key, key_schema);
        if let Some(index) = key_index {
            self.array[index].0 = new_key;
        }
    }

    pub fn key_index(&self, key: &Tuple, key_schema: &Schema) -> Option<usize> {
        if self.header.current_size == 0 {
            return None;
        }
        // 第一个key为空，所以从1开始
        let mut start: i32 = 1;
        let mut end: i32 = self.header.current_size as i32 - 1;
        while start < end {
            let mid = (start + end) / 2;
            let compare_res = key.compare(&self.array[mid as usize].0, key_schema);
            if compare_res == std::cmp::Ordering::Equal {
                return Some(mid as usize);
            } else if compare_res == std::cmp::Ordering::Less {
                end = mid - 1;
            } else {
                start = mid + 1;
            }
        }
        if key.compare(&self.array[start as usize].0, key_schema) == std::cmp::Ordering::Equal {
            return Some(start as usize);
        }
        return None;
    }

    // 查找key对应的page_id
    pub fn look_up(&self, key: &Tuple, key_schema: &Schema) -> PageId {
        // 第一个key为空，所以从1开始
        let mut start = 1;
        if self.header.current_size == 0 {
            println!("look_up empty page");
        }
        let mut end = self.header.current_size - 1;
        while start < end {
            let mid = (start + end) / 2;
            let compare_res = key.compare(&self.array[mid as usize].0, key_schema);
            if compare_res == std::cmp::Ordering::Equal {
                return self.array[mid as usize].1;
            } else if compare_res == std::cmp::Ordering::Less {
                end = mid - 1;
            } else {
                start = mid + 1;
            }
        }
        let compare_res = key.compare(&self.array[start as usize].0, key_schema);
        if compare_res == std::cmp::Ordering::Less {
            return self.array[start as usize - 1].1;
        } else {
            return self.array[start as usize].1;
        }
    }

    pub fn print_page(&self, page_id: PageId, key_schema: &Schema) {
        println!(
            "{:?}, page_id: {}, size: {}/{}",
            self.header.page_type, page_id, self.header.current_size, self.header.max_size
        );
        print!("array: ");
        for i in 0..self.header.current_size {
            if i == 0 {
                print!("null => {} , ", self.array[i as usize].1);
                continue;
            }
            print!(
                "{:?} => {}{}",
                self.array[i as usize].0.data,
                self.array[i as usize].1,
                if i == self.header.current_size - 1 {
                    ""
                } else {
                    " , "
                }
            );
        }
        println!("");
    }
}

/**
 * Leaf page format (keys are stored in order):
 *  ----------------------------------------------------------------------
 * | HEADER | KEY(1) + RID(1) | KEY(2) + RID(2) | ... | KEY(n) + RID(n)
 *  ----------------------------------------------------------------------
 *
 *  Header format (size in byte, 16 bytes in total):
 *  ---------------------------------------------------------------------
 * | PageType (4) | CurrentSize (4) | MaxSize (4) | NextPageId (4)
 *  ---------------------------------------------------------------------
 */
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BPlusTreeLeafPage {
    pub schema: SchemaRef,
    pub header: BPlusTreeLeafPageHeader,
    pub array: Vec<LeafKV>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BPlusTreeLeafPageHeader {
    pub page_type: BPlusTreePageType,
    pub current_size: u32,
    // max kv size can be stored
    pub max_size: u32,
    pub next_page_id: PageId,
}

impl BPlusTreeLeafPage {
    pub fn new(schema: SchemaRef, max_size: u32) -> Self {
        Self {
            schema,
            header: BPlusTreeLeafPageHeader {
                page_type: BPlusTreePageType::LeafPage,
                current_size: 0,
                max_size,
                next_page_id: INVALID_PAGE_ID,
            },
            array: Vec::with_capacity(max_size as usize),
        }
    }

    pub fn min_size(&self) -> u32 {
        self.header.max_size / 2
    }

    pub fn key_at(&self, index: usize) -> &Tuple {
        &self.array[index].0
    }

    pub fn kv_at(&self, index: usize) -> &LeafKV {
        &self.array[index]
    }

    pub fn is_full(&self) -> bool {
        self.header.current_size > self.header.max_size
    }

    // TODO 可以通过二分查找来插入
    pub fn insert(&mut self, key: Tuple, rid: Rid, key_schema: &Schema) {
        self.array.push((key, rid));
        self.header.current_size += 1;
        self.array.sort_by(|a, b| a.0.compare(&b.0, key_schema));
    }

    pub fn batch_insert(&mut self, kvs: Vec<LeafKV>, key_schema: &Schema) {
        let kvs_len = kvs.len();
        self.array.extend(kvs);
        self.header.current_size += kvs_len as u32;
        self.array.sort_by(|a, b| a.0.compare(&b.0, key_schema));
    }

    pub fn split_off(&mut self, at: usize) -> Vec<LeafKV> {
        let new_array = self.array.split_off(at);
        self.header.current_size -= new_array.len() as u32;
        return new_array;
    }

    pub fn reverse_split_off(&mut self, at: usize) -> Vec<LeafKV> {
        let mut new_array = Vec::new();
        for _ in 0..=at {
            new_array.push(self.array.remove(0));
        }
        self.header.current_size -= new_array.len() as u32;
        return new_array;
    }

    pub fn delete(&mut self, key: &Tuple, key_schema: &Schema) {
        let key_index = self.key_index(key, key_schema);
        if let Some(index) = key_index {
            self.array.remove(index);
            self.header.current_size -= 1;
        }
    }

    // 查找key对应的rid
    pub fn look_up(&self, key: &Tuple, key_schema: &Schema) -> Option<Rid> {
        let key_index = self.key_index(key, key_schema);
        return key_index.map(|index| self.array[index].1);
    }

    fn key_index(&self, key: &Tuple, key_schema: &Schema) -> Option<usize> {
        if self.header.current_size == 0 {
            return None;
        }
        let mut start: i32 = 0;
        let mut end: i32 = self.header.current_size as i32 - 1;
        while start < end {
            let mid = (start + end) / 2;
            let compare_res = key.compare(&self.array[mid as usize].0, key_schema);
            if compare_res == std::cmp::Ordering::Equal {
                return Some(mid as usize);
            } else if compare_res == std::cmp::Ordering::Less {
                end = mid - 1;
            } else {
                start = mid + 1;
            }
        }
        if key.compare(&self.array[start as usize].0, key_schema) == std::cmp::Ordering::Equal {
            return Some(start as usize);
        }
        None
    }

    pub fn print_page(&self, page_id: PageId, key_schema: &Schema) {
        println!(
            "{:?}, page_id: {}, size: {}/{}, , next_page_id: {}",
            self.header.page_type,
            page_id,
            self.header.current_size,
            self.header.max_size,
            self.header.next_page_id
        );
        print!("array: ");
        for i in 0..self.header.current_size {
            print!(
                "{:?} => {}-{}{}",
                self.array[i as usize].0.data,
                self.array[i as usize].1.page_id,
                self.array[i as usize].1.slot_num,
                if i == self.header.current_size - 1 {
                    ""
                } else {
                    " , "
                }
            );
        }
        println!("")
    }
}

#[cfg(test)]
mod tests {
    use crate::common::ScalarValue;
    use crate::{
        catalog::{Column, DataType, Schema},
        common::rid::Rid,
        storage::{
            index_page::{BPlusTreeInternalPage, BPlusTreeLeafPage},
            Tuple,
        },
    };
    use std::sync::Arc;

    #[test]
    pub fn test_internal_page_insert() {
        let key_schema = Arc::new(Schema::new(vec![
            Column::new("a".to_string(), DataType::Int8, false),
            Column::new("b".to_string(), DataType::Int16, false),
        ]));
        let mut internal_page = BPlusTreeInternalPage::new(key_schema.clone(), 3);
        internal_page.insert(Tuple::empty(key_schema.clone()), 0, &key_schema);
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            2,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            1,
            &key_schema,
        );
        assert_eq!(internal_page.header.current_size, 3);
        assert_eq!(
            internal_page.array[0].0.data,
            Tuple::empty(key_schema.clone()).data
        );
        assert_eq!(internal_page.array[0].1, 0);
        assert_eq!(internal_page.array[1].0.data, vec![1i8.into(), 1i16.into()]);
        assert_eq!(internal_page.array[1].1, 1);
        assert_eq!(internal_page.array[2].0.data, vec![2i8.into(), 2i16.into()]);
        assert_eq!(internal_page.array[2].1, 2);
    }

    #[test]
    pub fn test_leaf_page_insert() {
        let key_schema = Arc::new(Schema::new(vec![
            Column::new("a".to_string(), DataType::Int8, false),
            Column::new("b".to_string(), DataType::Int16, false),
        ]));
        let mut leaf_page = BPlusTreeLeafPage::new(key_schema.clone(), 3);
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            Rid::new(2, 2),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            Rid::new(1, 1),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            Rid::new(3, 3),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 3);
        assert_eq!(leaf_page.array[0].0.data, vec![1i8.into(), 1i16.into()]);
        assert_eq!(leaf_page.array[0].1, Rid::new(1, 1));
        assert_eq!(leaf_page.array[1].0.data, vec![2i8.into(), 2i16.into()]);
        assert_eq!(leaf_page.array[1].1, Rid::new(2, 2));
        assert_eq!(leaf_page.array[2].0.data, vec![3i8.into(), 3i16.into()]);
        assert_eq!(leaf_page.array[2].1, Rid::new(3, 3));
    }

    #[test]
    pub fn test_internal_page_look_up() {
        let key_schema = Arc::new(Schema::new(vec![
            Column::new("a".to_string(), DataType::Int8, false),
            Column::new("b".to_string(), DataType::Int16, false),
        ]));
        let mut internal_page = BPlusTreeInternalPage::new(key_schema.clone(), 5);
        internal_page.insert(Tuple::empty(key_schema.clone()), 0, &key_schema);
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            2,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            1,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            3,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            4,
            &key_schema,
        );

        assert_eq!(
            internal_page.look_up(
                &Tuple::new(key_schema.clone(), vec![0i8.into(), 0i16.into()]),
                &key_schema
            ),
            0
        );
        assert_eq!(
            internal_page.look_up(
                &Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
                &key_schema
            ),
            3
        );
        assert_eq!(
            internal_page.look_up(
                &Tuple::new(key_schema.clone(), vec![5i8.into(), 5i16.into()]),
                &key_schema
            ),
            4
        );

        let mut internal_page = BPlusTreeInternalPage::new(key_schema.clone(), 2);
        internal_page.insert(Tuple::empty(key_schema.clone()), 0, &key_schema);
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            1,
            &key_schema,
        );

        assert_eq!(
            internal_page.look_up(
                &Tuple::new(key_schema.clone(), vec![0i8.into(), 0i16.into()]),
                &key_schema
            ),
            0
        );
        assert_eq!(
            internal_page.look_up(
                &Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
                &key_schema
            ),
            1
        );
        assert_eq!(
            internal_page.look_up(
                &Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
                &key_schema
            ),
            1
        );
    }

    #[test]
    pub fn test_leaf_page_look_up() {
        let key_schema = Arc::new(Schema::new(vec![
            Column::new("a".to_string(), DataType::Int8, false),
            Column::new("b".to_string(), DataType::Int16, false),
        ]));
        let mut leaf_page = BPlusTreeLeafPage::new(key_schema.clone(), 5);
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            Rid::new(2, 2),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            Rid::new(1, 1),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            Rid::new(3, 3),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![5i8.into(), 5i16.into()]),
            Rid::new(5, 5),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            Rid::new(4, 4),
            &key_schema,
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![0i8.into(), 0i16.into()]),
                &key_schema
            ),
            None
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
                &key_schema
            ),
            Some(Rid::new(2, 2))
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
                &key_schema
            ),
            Some(Rid::new(3, 3))
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![6i8.into(), 6i16.into()]),
                &key_schema
            ),
            None
        );

        let mut leaf_page = BPlusTreeLeafPage::new(key_schema.clone(), 2);
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            Rid::new(2, 2),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            Rid::new(1, 1),
            &key_schema,
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(
                    key_schema.clone(),
                    vec![ScalarValue::Int8(None), ScalarValue::Int16(None)]
                ),
                &key_schema
            ),
            None
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
                &key_schema
            ),
            Some(Rid::new(1, 1))
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
                &key_schema
            ),
            Some(Rid::new(2, 2))
        );
        assert_eq!(
            leaf_page.look_up(
                &Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
                &key_schema
            ),
            None
        );
    }

    #[test]
    pub fn test_internal_page_delete() {
        let key_schema = Arc::new(Schema::new(vec![
            Column::new("a".to_string(), DataType::Int8, false),
            Column::new("b".to_string(), DataType::Int16, false),
        ]));
        let mut internal_page = BPlusTreeInternalPage::new(key_schema.clone(), 5);
        internal_page.insert(Tuple::empty(key_schema.clone()), 0, &key_schema);
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            2,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            1,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            3,
            &key_schema,
        );
        internal_page.insert(
            Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            4,
            &key_schema,
        );

        internal_page.delete(
            &Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            &key_schema,
        );
        assert_eq!(internal_page.header.current_size, 4);
        assert_eq!(
            internal_page.array[0].0.data,
            vec![ScalarValue::Int8(None), ScalarValue::Int16(None)]
        );
        assert_eq!(internal_page.array[0].1, 0);
        assert_eq!(internal_page.array[1].0.data, vec![1i8.into(), 1i16.into()]);
        assert_eq!(internal_page.array[1].1, 1);
        assert_eq!(internal_page.array[2].0.data, vec![3i8.into(), 3i16.into()]);
        assert_eq!(internal_page.array[2].1, 3);
        assert_eq!(internal_page.array[3].0.data, vec![4i8.into(), 4i16.into()]);
        assert_eq!(internal_page.array[3].1, 4);
        internal_page.delete(
            &Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            &key_schema,
        );
        assert_eq!(internal_page.header.current_size, 3);
        internal_page.delete(
            &Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            &key_schema,
        );
        assert_eq!(internal_page.header.current_size, 2);
        internal_page.delete(
            &Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            &key_schema,
        );
        assert_eq!(internal_page.header.current_size, 0);
        internal_page.delete(
            &Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            &key_schema,
        );
        assert_eq!(internal_page.header.current_size, 0);
    }

    #[test]
    pub fn test_leaf_page_delete() {
        let key_schema = Arc::new(Schema::new(vec![
            Column::new("a".to_string(), DataType::Int8, false),
            Column::new("b".to_string(), DataType::Int16, false),
        ]));
        let mut leaf_page = BPlusTreeLeafPage::new(key_schema.clone(), 5);
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            Rid::new(2, 2),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            Rid::new(1, 1),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            Rid::new(3, 3),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![5i8.into(), 5i16.into()]),
            Rid::new(5, 5),
            &key_schema,
        );
        leaf_page.insert(
            Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            Rid::new(4, 4),
            &key_schema,
        );

        leaf_page.delete(
            &Tuple::new(key_schema.clone(), vec![2i8.into(), 2i16.into()]),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 4);
        assert_eq!(leaf_page.array[0].0.data, vec![1i8.into(), 1i16.into()]);
        assert_eq!(leaf_page.array[0].1, Rid::new(1, 1));
        assert_eq!(leaf_page.array[1].0.data, vec![3i8.into(), 3i16.into()]);
        assert_eq!(leaf_page.array[1].1, Rid::new(3, 3));
        assert_eq!(leaf_page.array[2].0.data, vec![4i8.into(), 4i16.into()]);
        assert_eq!(leaf_page.array[2].1, Rid::new(4, 4));
        assert_eq!(leaf_page.array[3].0.data, vec![5i8.into(), 5i16.into()]);
        assert_eq!(leaf_page.array[3].1, Rid::new(5, 5));
        leaf_page.delete(
            &Tuple::new(key_schema.clone(), vec![3i8.into(), 3i16.into()]),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 3);
        leaf_page.delete(
            &Tuple::new(key_schema.clone(), vec![5i8.into(), 5i16.into()]),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 2);
        leaf_page.delete(
            &Tuple::new(key_schema.clone(), vec![1i8.into(), 1i16.into()]),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 1);
        assert_eq!(leaf_page.array[0].0.data, vec![4i8.into(), 4i16.into()]);
        assert_eq!(leaf_page.array[0].1, Rid::new(4, 4));
        leaf_page.delete(
            &Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 0);
        leaf_page.delete(
            &Tuple::new(key_schema.clone(), vec![4i8.into(), 4i16.into()]),
            &key_schema,
        );
        assert_eq!(leaf_page.header.current_size, 0);
    }
}
