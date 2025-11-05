use core::mem::size_of;
use core::ptr::{copy_nonoverlapping, null_mut};

use crate::{_allocate, _free, _set_to_string, _string_clone, _string_count, _string_equals, _string_from_number, _vector_to_string, FALSE_LITERAL, NIL_LITERAL, TRUE_LITERAL};

#[repr(C)]
struct MapHeader {
    length: u64,
    capacity: u64,
}

#[repr(C)]
struct EntryRender {
    ptr: *mut u8,
    len: usize,
    owned: bool,
}

#[repr(C)]
struct MapRenderSlot {
    key: EntryRender,
    value: EntryRender,
}

const TAG_NIL: u8 = 0;
const TAG_NUMBER: u8 = 1;
const TAG_BOOLEAN: u8 = 2;
const TAG_STRING: u8 = 3;
const TAG_VECTOR: u8 = 4;
const TAG_MAP: u8 = 5;
const TAG_KEYWORD: u8 = 6;
const TAG_SET: u8 = 7;
const TAG_ANY: u8 = 0xff;

// Forward declarations for cross-module equality comparisons
extern "C" {
    fn _vector_equals(left: *const u8, right: *const u8) -> i64;
    fn _set_equals(left: *const u8, right: *const u8) -> i64;
}

#[inline]
fn padded_tag_bytes(len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    let align = size_of::<i64>();
    let remainder = len % align;
    if remainder == 0 {
        len
    } else {
        len + (align - remainder)
    }
}

#[inline]
fn map_allocation_size(len: usize) -> Option<usize> {
    let header = size_of::<MapHeader>();
    let padded = padded_tag_bytes(len);
    let tags_total = padded.checked_mul(2)?;
    let data_bytes = len.checked_mul(size_of::<i64>())?;
    let values_total = data_bytes.checked_mul(2)?;
    header.checked_add(tags_total)?.checked_add(values_total)
}

#[inline]
unsafe fn map_key_tags_ptr(map: *const MapHeader) -> *const u8 {
    (map as *const u8).add(size_of::<MapHeader>())
}

#[inline]
unsafe fn map_key_tags_ptr_mut(map: *mut MapHeader) -> *mut u8 {
    (map as *mut u8).add(size_of::<MapHeader>())
}

#[inline]
unsafe fn map_value_tags_ptr(map: *const MapHeader) -> *const u8 {
    let len = (*map).capacity as usize;
    map_key_tags_ptr(map).add(padded_tag_bytes(len))
}

#[inline]
unsafe fn map_value_tags_ptr_mut(map: *mut MapHeader) -> *mut u8 {
    let len = (*map).capacity as usize;
    map_key_tags_ptr_mut(map).add(padded_tag_bytes(len))
}

#[inline]
unsafe fn map_key_data_ptr(map: *const MapHeader) -> *const i64 {
    let len = (*map).capacity as usize;
    let offset = size_of::<MapHeader>() + padded_tag_bytes(len) * 2;
    (map as *const u8).add(offset) as *const i64
}

#[inline]
unsafe fn map_key_data_ptr_mut(map: *mut MapHeader) -> *mut i64 {
    let len = (*map).capacity as usize;
    let offset = size_of::<MapHeader>() + padded_tag_bytes(len) * 2;
    (map as *mut u8).add(offset) as *mut i64
}

#[inline]
unsafe fn map_value_data_ptr(map: *const MapHeader) -> *const i64 {
    let len = (*map).capacity as usize;
    map_key_data_ptr(map).add(len)
}

#[inline]
unsafe fn map_value_data_ptr_mut(map: *mut MapHeader) -> *mut i64 {
    let len = (*map).capacity as usize;
    map_key_data_ptr_mut(map).add(len)
}

unsafe fn map_allocate(len: usize) -> *mut MapHeader {
    match map_allocation_size(len) {
        Some(total) => {
            let raw = _allocate(total as u64);
            if raw.is_null() {
                null_mut()
            } else {
                let header = raw as *mut MapHeader;
                (*header).length = len as u64;
                (*header).capacity = len as u64;

                if len > 0 {
                    let padded = padded_tag_bytes(len);
                    let key_tags = map_key_tags_ptr_mut(header);
                    let value_tags = map_value_tags_ptr_mut(header);
                    let mut idx = 0usize;
                    while idx < padded {
                        *key_tags.add(idx) = TAG_ANY;
                        *value_tags.add(idx) = TAG_ANY;
                        idx += 1;
                    }
                }

                header
            }
        }
        None => null_mut(),
    }
}

#[inline]
unsafe fn release_entry_render(entry: &EntryRender) {
    if entry.owned && !entry.ptr.is_null() {
        _free(entry.ptr);
    }
}

unsafe fn release_render_slots(entries: *mut MapRenderSlot, len: usize) {
    if entries.is_null() {
        return;
    }

    let mut idx = 0usize;
    while idx < len {
        let slot = entries.add(idx);
        release_entry_render(&(*slot).key);
        release_entry_render(&(*slot).value);
        idx += 1;
    }
    _free(entries as *mut u8);
}

#[inline]
fn canonical_boolean(value: i64) -> i64 {
    if value == 0 {
        0
    } else {
        1
    }
}

unsafe fn map_keys_equal(stored_tag: u8, stored_value: i64, query_tag: u8, query_value: i64) -> bool {
    if stored_tag != query_tag {
        return false;
    }

    match stored_tag {
        TAG_NIL | TAG_NUMBER => stored_value == query_value,
        TAG_BOOLEAN => canonical_boolean(stored_value) == canonical_boolean(query_value),
        TAG_STRING | TAG_KEYWORD => {
            let left = stored_value as *const u8;
            let right = query_value as *const u8;
            _string_equals(left, right) != 0
        }
        TAG_VECTOR | TAG_MAP => stored_value == query_value,
        _ => false,
    }
}

unsafe fn map_find_index(map: *const MapHeader, key_tag: u8, key_value: i64) -> Option<usize> {
    if map.is_null() {
        return None;
    }

    let len = (*map).length as usize;
    if len == 0 {
        return None;
    }

    let key_tags = map_key_tags_ptr(map);
    let key_data = map_key_data_ptr(map);

    let mut idx = 0usize;
    while idx < len {
        let stored_tag = *key_tags.add(idx);
        let stored_value = *key_data.add(idx);
        if map_keys_equal(stored_tag, stored_value, key_tag, key_value) {
            return Some(idx);
        }
        idx += 1;
    }

    None
}

unsafe fn map_copy_entries(dst: *mut MapHeader, src: *const MapHeader) {
    if dst.is_null() || src.is_null() {
        return;
    }

    let len = (*src).length as usize;
    if len == 0 {
        return;
    }

    let src_key_tags = map_key_tags_ptr(src);
    let src_value_tags = map_value_tags_ptr(src);
    let src_key_data = map_key_data_ptr(src);
    let src_value_data = map_value_data_ptr(src);

    let dst_key_tags = map_key_tags_ptr_mut(dst);
    let dst_value_tags = map_value_tags_ptr_mut(dst);
    let dst_key_data = map_key_data_ptr_mut(dst);
    let dst_value_data = map_value_data_ptr_mut(dst);

    copy_nonoverlapping(src_key_tags, dst_key_tags, len);
    copy_nonoverlapping(src_value_tags, dst_value_tags, len);
    copy_nonoverlapping(src_key_data, dst_key_data, len);
    copy_nonoverlapping(src_value_data, dst_value_data, len);

    let padded = padded_tag_bytes((*dst).capacity as usize);
    let mut idx = len;
    while idx < padded {
        *dst_key_tags.add(idx) = TAG_ANY;
        *dst_value_tags.add(idx) = TAG_ANY;
        idx += 1;
    }
}

#[inline]
unsafe fn map_write_entry(map: *mut MapHeader, index: usize, key_tag: u8, key_value: i64, value_tag: u8, value_value: i64) {
    let key_tags = map_key_tags_ptr_mut(map);
    let value_tags = map_value_tags_ptr_mut(map);
    let key_data = map_key_data_ptr_mut(map);
    let value_data = map_value_data_ptr_mut(map);
    *key_tags.add(index) = key_tag;
    *key_data.add(index) = key_value;
    *value_tags.add(index) = value_tag;
    *value_data.add(index) = value_value;
}

#[inline]
unsafe fn map_write_value(map: *mut MapHeader, index: usize, value_tag: u8, value_value: i64) {
    let value_tags = map_value_tags_ptr_mut(map);
    let value_data = map_value_data_ptr_mut(map);
    *value_tags.add(index) = value_tag;
    *value_data.add(index) = value_value;
}

unsafe fn render_map_key(tag: u8, value: i64) -> EntryRender {
    match tag {
        TAG_NIL => EntryRender {
            ptr: NIL_LITERAL.as_ptr() as *mut u8,
            len: 3,
            owned: false,
        },
        TAG_BOOLEAN => {
            if canonical_boolean(value) == 0 {
                EntryRender {
                    ptr: FALSE_LITERAL.as_ptr() as *mut u8,
                    len: 5,
                    owned: false,
                }
            } else {
                EntryRender {
                    ptr: TRUE_LITERAL.as_ptr() as *mut u8,
                    len: 4,
                    owned: false,
                }
            }
        }
        TAG_NUMBER => {
            let rendered = _string_from_number(value);
            if rendered.is_null() {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                EntryRender {
                    ptr: rendered,
                    len: _string_count(rendered) as usize,
                    owned: true,
                }
            }
        }
        TAG_STRING => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let source = value as *const u8;
                if source.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    let len = _string_count(source) as usize;
                    let total = match len.checked_add(3) {
                        Some(size) => size,
                        None => {
                            return EntryRender {
                                ptr: NIL_LITERAL.as_ptr() as *mut u8,
                                len: 3,
                                owned: false,
                            };
                        }
                    };
                    let dst = _allocate(total as u64);
                    if dst.is_null() {
                        EntryRender {
                            ptr: NIL_LITERAL.as_ptr() as *mut u8,
                            len: 3,
                            owned: false,
                        }
                    } else {
                        *dst = b'"';
                        copy_nonoverlapping(source, dst.add(1), len);
                        *dst.add(len + 1) = b'"';
                        *dst.add(total - 1) = 0;
                        EntryRender { ptr: dst, len: len + 2, owned: true }
                    }
                }
            }
        }
        TAG_KEYWORD => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let source = value as *const u8;
                if source.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    let cloned = _string_clone(source);
                    if cloned.is_null() {
                        EntryRender {
                            ptr: NIL_LITERAL.as_ptr() as *mut u8,
                            len: 3,
                            owned: false,
                        }
                    } else {
                        EntryRender {
                            ptr: cloned,
                            len: _string_count(cloned) as usize,
                            owned: true,
                        }
                    }
                }
            }
        }
        _ => EntryRender {
            ptr: NIL_LITERAL.as_ptr() as *mut u8,
            len: 3,
            owned: false,
        },
    }
}

unsafe fn render_map_value(tag: u8, value: i64) -> EntryRender {
    match tag {
        TAG_NIL => EntryRender {
            ptr: NIL_LITERAL.as_ptr() as *mut u8,
            len: 3,
            owned: false,
        },
        TAG_BOOLEAN => {
            if canonical_boolean(value) == 0 {
                EntryRender {
                    ptr: FALSE_LITERAL.as_ptr() as *mut u8,
                    len: 5,
                    owned: false,
                }
            } else {
                EntryRender {
                    ptr: TRUE_LITERAL.as_ptr() as *mut u8,
                    len: 4,
                    owned: false,
                }
            }
        }
        TAG_STRING => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let cloned = _string_clone(value as *const u8);
                if cloned.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    EntryRender {
                        ptr: cloned,
                        len: _string_count(cloned) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_KEYWORD => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let cloned = _string_clone(value as *const u8);
                if cloned.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    EntryRender {
                        ptr: cloned,
                        len: _string_count(cloned) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_VECTOR => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let rendered = _vector_to_string(value as *const u8);
                if rendered.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    EntryRender {
                        ptr: rendered,
                        len: _string_count(rendered) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_MAP => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let rendered = _map_to_string(value as *const u8);
                if rendered.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    EntryRender {
                        ptr: rendered,
                        len: _string_count(rendered) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_SET => {
            if value == 0 {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let rendered = _set_to_string(value as *const u8);
                if rendered.is_null() {
                    EntryRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    EntryRender {
                        ptr: rendered,
                        len: _string_count(rendered) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_NUMBER => {
            let rendered = _string_from_number(value);
            if rendered.is_null() {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                EntryRender {
                    ptr: rendered,
                    len: _string_count(rendered) as usize,
                    owned: true,
                }
            }
        }
        _ => {
            let rendered = _string_from_number(value);
            if rendered.is_null() {
                EntryRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                EntryRender {
                    ptr: rendered,
                    len: _string_count(rendered) as usize,
                    owned: true,
                }
            }
        }
    }
}

unsafe fn map_clone_impl(map: *const MapHeader) -> *mut MapHeader {
    if map.is_null() {
        return map_allocate(0);
    }

    let len = (*map).length as usize;
    let cloned = map_allocate(len);
    if cloned.is_null() {
        return null_mut();
    }

    map_copy_entries(cloned, map);
    cloned
}

unsafe fn map_assoc_impl(map: *const MapHeader, key_tag: u8, key_value: i64, value_tag: u8, value_value: i64) -> *mut MapHeader {
    if map.is_null() {
        let new_map = map_allocate(1);
        if new_map.is_null() {
            return null_mut();
        }
        map_write_entry(new_map, 0, key_tag, key_value, value_tag, value_value);
        return new_map;
    }

    let len = (*map).length as usize;
    match map_find_index(map, key_tag, key_value) {
        Some(index) => {
            let new_map = map_allocate(len);
            if new_map.is_null() {
                return null_mut();
            }
            map_copy_entries(new_map, map);
            map_write_value(new_map, index, value_tag, value_value);
            new_map
        }
        None => {
            let new_map = map_allocate(len + 1);
            if new_map.is_null() {
                return null_mut();
            }
            map_copy_entries(new_map, map);
            map_write_entry(new_map, len, key_tag, key_value, value_tag, value_value);
            (*new_map).length = (len + 1) as u64;
            (*new_map).capacity = (len + 1) as u64;
            new_map
        }
    }
}

unsafe fn map_dissoc_impl(map: *const MapHeader, key_tag: u8, key_value: i64) -> *mut MapHeader {
    if map.is_null() {
        return map_allocate(0);
    }

    let len = (*map).length as usize;
    if len == 0 {
        return map_allocate(0);
    }

    match map_find_index(map, key_tag, key_value) {
        Some(index) => {
            if len == 1 {
                return map_allocate(0);
            }

            let new_len = len - 1;
            let new_map = map_allocate(new_len);
            if new_map.is_null() {
                return null_mut();
            }

            let src_key_tags = map_key_tags_ptr(map);
            let src_value_tags = map_value_tags_ptr(map);
            let src_key_data = map_key_data_ptr(map);
            let src_value_data = map_value_data_ptr(map);

            let dst_key_tags = map_key_tags_ptr_mut(new_map);
            let dst_value_tags = map_value_tags_ptr_mut(new_map);
            let dst_key_data = map_key_data_ptr_mut(new_map);
            let dst_value_data = map_value_data_ptr_mut(new_map);

            if index > 0 {
                copy_nonoverlapping(src_key_tags, dst_key_tags, index);
                copy_nonoverlapping(src_value_tags, dst_value_tags, index);
                copy_nonoverlapping(src_key_data, dst_key_data, index);
                copy_nonoverlapping(src_value_data, dst_value_data, index);
            }

            if index + 1 < len {
                let tail_len = len - index - 1;
                copy_nonoverlapping(src_key_tags.add(index + 1), dst_key_tags.add(index), tail_len);
                copy_nonoverlapping(src_value_tags.add(index + 1), dst_value_tags.add(index), tail_len);
                copy_nonoverlapping(src_key_data.add(index + 1), dst_key_data.add(index), tail_len);
                copy_nonoverlapping(src_value_data.add(index + 1), dst_value_data.add(index), tail_len);
            }

            (*new_map).length = new_len as u64;
            (*new_map).capacity = new_len as u64;

            new_map
        }
        None => map_clone_impl(map),
    }
}

#[no_mangle]
pub unsafe extern "C" fn _map_create(keys: *const i64, key_tags: *const i64, values: *const i64, value_tags: *const i64, count: u64) -> *mut u8 {
    if count > usize::MAX as u64 {
        return null_mut();
    }

    let len = count as usize;
    let map = map_allocate(len);
    if map.is_null() {
        return null_mut();
    }

    if len == 0 {
        return map as *mut u8;
    }

    if keys.is_null() || key_tags.is_null() || values.is_null() || value_tags.is_null() {
        _map_free(map as *mut u8);
        return null_mut();
    }

    let key_tags_src = key_tags;
    let value_tags_src = value_tags;
    let key_tags_dst = map_key_tags_ptr_mut(map);
    let value_tags_dst = map_value_tags_ptr_mut(map);
    let key_data_dst = map_key_data_ptr_mut(map);
    let value_data_dst = map_value_data_ptr_mut(map);

    let mut idx = 0usize;
    while idx < len {
        let key_tag = (*key_tags_src.add(idx) & 0xff) as u8;
        let value_tag = (*value_tags_src.add(idx) & 0xff) as u8;
        *key_tags_dst.add(idx) = key_tag;
        *value_tags_dst.add(idx) = value_tag;
        *key_data_dst.add(idx) = *keys.add(idx);
        *value_data_dst.add(idx) = *values.add(idx);
        idx += 1;
    }

    map as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn _map_clone(map: *const u8) -> *mut u8 {
    let src = map as *const MapHeader;
    map_clone_impl(src) as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn _map_assoc(map: *const u8, key: i64, key_tag: i64, value: i64, value_tag: i64) -> *mut u8 {
    let key_tag_u8 = (key_tag & 0xff) as u8;
    let value_tag_u8 = (value_tag & 0xff) as u8;
    let src = map as *const MapHeader;
    map_assoc_impl(src, key_tag_u8, key, value_tag_u8, value) as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn _map_dissoc(map: *const u8, key: i64, key_tag: i64) -> *mut u8 {
    let key_tag_u8 = (key_tag & 0xff) as u8;
    let src = map as *const MapHeader;
    map_dissoc_impl(src, key_tag_u8, key) as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn _map_contains(map: *const u8, key: i64, key_tag: i64) -> i64 {
    let key_tag_u8 = (key_tag & 0xff) as u8;
    let header = map as *const MapHeader;
    if header.is_null() {
        return 0;
    }
    if map_find_index(header, key_tag_u8, key).is_some() {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn _map_get(map: *const u8, key: i64, key_tag: i64, out_value: *mut i64, out_tag: *mut u8) -> i64 {
    if out_value.is_null() || out_tag.is_null() {
        return 0;
    }
    let key_tag_u8 = (key_tag & 0xff) as u8;
    let header = map as *const MapHeader;
    if header.is_null() {
        return 0;
    }

    match map_find_index(header, key_tag_u8, key) {
        Some(index) => {
            let value_tags = map_value_tags_ptr(header);
            let value_data = map_value_data_ptr(header);
            *out_tag = *value_tags.add(index);
            *out_value = *value_data.add(index);
            1
        }
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn _map_count(map: *const u8) -> u64 {
    if map.is_null() {
        return 0;
    }
    let header = map as *const MapHeader;
    (*header).length
}

#[no_mangle]
pub unsafe extern "C" fn _map_to_string(map: *const u8) -> *mut u8 {
    if map.is_null() {
        let dst = _allocate(3);
        if dst.is_null() {
            return null_mut();
        }
        *dst = b'{';
        *dst.add(1) = b'}';
        *dst.add(2) = 0;
        return dst;
    }

    let header = map as *const MapHeader;
    let len = (*header).length as usize;

    if len == 0 {
        let dst = _allocate(3);
        if dst.is_null() {
            return null_mut();
        }
        *dst = b'{';
        *dst.add(1) = b'}';
        *dst.add(2) = 0;
        return dst;
    }

    let slots_size = len.checked_mul(size_of::<MapRenderSlot>()).unwrap_or(0);
    if slots_size == 0 {
        return null_mut();
    }

    let slots_ptr = _allocate(slots_size as u64) as *mut MapRenderSlot;
    if slots_ptr.is_null() {
        return null_mut();
    }

    let key_tags = map_key_tags_ptr(header);
    let value_tags = map_value_tags_ptr(header);
    let key_data = map_key_data_ptr(header);
    let value_data = map_value_data_ptr(header);

    let mut total_len = 2usize; // '{' and '}'
    let mut idx = 0usize;
    let mut overflow = false;

    while idx < len {
        let slot = slots_ptr.add(idx);
        (*slot).key = render_map_key(*key_tags.add(idx), *key_data.add(idx));
        (*slot).value = render_map_value(*value_tags.add(idx), *value_data.add(idx));

        if !overflow {
            total_len = match total_len.checked_add((*slot).key.len) {
                Some(val) => val,
                None => {
                    overflow = true;
                    total_len
                }
            };

            total_len = match total_len.checked_add(1) {
                Some(val) => val,
                None => {
                    overflow = true;
                    total_len
                }
            };

            total_len = match total_len.checked_add((*slot).value.len) {
                Some(val) => val,
                None => {
                    overflow = true;
                    total_len
                }
            };

            if idx + 1 < len {
                total_len = match total_len.checked_add(1) {
                    Some(val) => val,
                    None => {
                        overflow = true;
                        total_len
                    }
                };
            }
        }

        idx += 1;
    }

    if overflow {
        release_render_slots(slots_ptr, len);
        return null_mut();
    }

    let total_with_null = match total_len.checked_add(1) {
        Some(val) => val,
        None => {
            release_render_slots(slots_ptr, len);
            return null_mut();
        }
    };

    let dst = _allocate(total_with_null as u64);
    if dst.is_null() {
        release_render_slots(slots_ptr, len);
        return null_mut();
    }

    let mut offset = 0usize;
    *dst.add(offset) = b'{';
    offset += 1;

    idx = 0usize;
    while idx < len {
        let slot = slots_ptr.add(idx);

        if idx > 0 {
            *dst.add(offset) = b' ';
            offset += 1;
        }

        if !(*slot).key.ptr.is_null() && (*slot).key.len > 0 {
            copy_nonoverlapping((*slot).key.ptr, dst.add(offset), (*slot).key.len);
            offset += (*slot).key.len;
        }

        *dst.add(offset) = b' ';
        offset += 1;

        if !(*slot).value.ptr.is_null() && (*slot).value.len > 0 {
            copy_nonoverlapping((*slot).value.ptr, dst.add(offset), (*slot).value.len);
            offset += (*slot).value.len;
        }

        idx += 1;
    }

    *dst.add(offset) = b'}';
    offset += 1;
    *dst.add(offset) = 0;

    release_render_slots(slots_ptr, len);
    dst
}

#[no_mangle]
pub unsafe extern "C" fn _map_free(map: *mut u8) {
    if map.is_null() {
        return;
    }
    _free(map);
}

/// Helper function to compare two tagged values for equality
unsafe fn tagged_values_equal(left_tag: u8, left_val: i64, right_tag: u8, right_val: i64) -> bool {
    if left_tag != right_tag {
        return false;
    }

    match left_tag {
        TAG_NIL => true,
        TAG_NUMBER => left_val == right_val,
        TAG_BOOLEAN => canonical_boolean(left_val) == canonical_boolean(right_val),
        TAG_STRING | TAG_KEYWORD => {
            _string_equals(left_val as *const u8, right_val as *const u8) != 0
        }
        TAG_VECTOR => {
            _vector_equals(left_val as *const u8, right_val as *const u8) != 0
        }
        TAG_MAP => {
            _map_equals(left_val as *const u8, right_val as *const u8) != 0
        }
        TAG_SET => {
            _set_equals(left_val as *const u8, right_val as *const u8) != 0
        }
        _ => left_val == right_val,
    }
}

/// # Safety
///
/// The caller must ensure that `left` and `right` are either null or point to maps created by the runtime.
/// Returns 1 if the maps are equal, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn _map_equals(left: *const u8, right: *const u8) -> i64 {
    // Fast path: same pointer
    if left == right {
        return 1;
    }

    // If either is null, they're not equal (we already checked if both are the same)
    if left.is_null() || right.is_null() {
        return 0;
    }

    let left_header = left as *const MapHeader;
    let right_header = right as *const MapHeader;

    // Compare counts
    let left_count = (*left_header).length;
    let right_count = (*right_header).length;
    if left_count != right_count {
        return 0;
    }

    let len = left_count as usize;
    if len == 0 {
        return 1; // Both empty
    }

    // Get pointers to key and value data
    let left_key_data = map_key_data_ptr(left_header);
    let left_key_tags = map_key_tags_ptr(left_header);
    let left_value_data = map_value_data_ptr(left_header);
    let left_value_tags = map_value_tags_ptr(left_header);

    // For each key-value pair in left map, check if it exists in right map with same value
    let mut idx = 0usize;
    while idx < len {
        let left_key = *left_key_data.add(idx);
        let left_key_tag = *left_key_tags.add(idx);
        let left_value = *left_value_data.add(idx);
        let left_value_tag = *left_value_tags.add(idx);

        // Look up this key in the right map
        let mut right_value = 0i64;
        let mut right_value_tag = 0u8;
        let found = _map_get(
            right,
            left_key,
            left_key_tag as i64,
            &mut right_value,
            &mut right_value_tag,
        );

        if found == 0 {
            return 0; // Key not found in right map
        }

        // Compare the values
        if !tagged_values_equal(left_value_tag, left_value, right_value_tag, right_value) {
            return 0;
        }

        idx += 1;
    }

    1
}
