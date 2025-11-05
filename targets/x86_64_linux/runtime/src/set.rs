use core::mem::size_of;
use core::ptr::{copy_nonoverlapping, null_mut};

use crate::{
    _allocate, _free, _map_assoc, _map_clone, _map_contains, _map_count, _map_create, _map_dissoc, _map_free, _map_to_string, _string_clone, _string_count, _string_equals, _string_from_number,
    _vector_to_string, FALSE_LITERAL, NIL_LITERAL, TRUE_LITERAL,
};

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

const TAG_NIL: u8 = 0;
const TAG_NUMBER: u8 = 1;
const TAG_BOOLEAN: u8 = 2;
const TAG_STRING: u8 = 3;
const TAG_VECTOR: u8 = 4;
const TAG_MAP: u8 = 5;
const TAG_KEYWORD: u8 = 6;
const TAG_SET: u8 = 7;
const TAG_BOOLEAN_I64: i64 = TAG_BOOLEAN as i64;

// Forward declarations for cross-module equality comparisons
extern "C" {
    fn _vector_equals(left: *const u8, right: *const u8) -> i64;
    fn _map_equals(left: *const u8, right: *const u8) -> i64;
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
unsafe fn map_key_tags_ptr(map: *const MapHeader) -> *const u8 {
    (map as *const u8).add(size_of::<MapHeader>())
}

#[inline]
unsafe fn map_key_data_ptr(map: *const MapHeader) -> *const i64 {
    let len = (*map).capacity as usize;
    let offset = size_of::<MapHeader>() + padded_tag_bytes(len) * 2;
    (map as *const u8).add(offset) as *const i64
}

#[inline]
unsafe fn release_entry(entry: &EntryRender) {
    if entry.owned && !entry.ptr.is_null() {
        _free(entry.ptr);
    }
}

unsafe fn release_entries(entries: *mut EntryRender, len: usize) {
    if entries.is_null() {
        return;
    }

    let mut idx = 0usize;
    while idx < len {
        release_entry(&*entries.add(idx));
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

unsafe fn render_set_entry(tag: u8, value: i64) -> EntryRender {
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

#[no_mangle]
pub unsafe extern "C" fn _set_create(values: *const i64, value_tags: *const i64, count: u64) -> *mut u8 {
    if count > usize::MAX as u64 {
        return null_mut();
    }

    if count == 0 {
        return _map_create(null_mut(), null_mut(), null_mut(), null_mut(), 0);
    }

    if values.is_null() || value_tags.is_null() {
        return null_mut();
    }

    let mut current: *mut u8 = null_mut();
    let mut idx = 0usize;

    while idx < count as usize {
        let key = *values.add(idx);
        let key_tag = *value_tags.add(idx);
        let next = _map_assoc(current as *const u8, key, key_tag, 1, TAG_BOOLEAN_I64);
        if next.is_null() {
            if !current.is_null() {
                _map_free(current);
            }
            return null_mut();
        }
        if !current.is_null() {
            _map_free(current);
        }
        current = next;
        idx += 1;
    }

    current
}

#[no_mangle]
pub unsafe extern "C" fn _set_clone(set: *const u8) -> *mut u8 {
    _map_clone(set)
}

#[no_mangle]
pub unsafe extern "C" fn _set_disj(set: *const u8, value: i64, value_tag: i64) -> *mut u8 {
    _map_dissoc(set, value, value_tag)
}

#[no_mangle]
pub unsafe extern "C" fn _set_contains(set: *const u8, value: i64, value_tag: i64) -> i64 {
    _map_contains(set, value, value_tag)
}

#[no_mangle]
pub unsafe extern "C" fn _set_count(set: *const u8) -> u64 {
    _map_count(set)
}

#[no_mangle]
pub unsafe extern "C" fn _set_free(set: *mut u8) {
    _map_free(set);
}

#[no_mangle]
pub unsafe extern "C" fn _set_to_string(set: *const u8) -> *mut u8 {
    if set.is_null() {
        let dst = _allocate(4);
        if dst.is_null() {
            return null_mut();
        }
        *dst = b'#';
        *dst.add(1) = b'{';
        *dst.add(2) = b'}';
        *dst.add(3) = 0;
        return dst;
    }

    let header = set as *const MapHeader;
    if header.is_null() {
        return null_mut();
    }

    let len = (*header).length as usize;
    if len == 0 {
        let dst = _allocate(4);
        if dst.is_null() {
            return null_mut();
        }
        *dst = b'#';
        *dst.add(1) = b'{';
        *dst.add(2) = b'}';
        *dst.add(3) = 0;
        return dst;
    }

    let slots_size = len.checked_mul(size_of::<EntryRender>()).unwrap_or(0);
    if slots_size == 0 {
        return null_mut();
    }

    let entries = _allocate(slots_size as u64) as *mut EntryRender;
    if entries.is_null() {
        return null_mut();
    }

    let key_tags = map_key_tags_ptr(header);
    let key_data = map_key_data_ptr(header);

    let mut total_len = 3usize; // '#', '{', '}'
    let mut idx = 0usize;
    let mut overflow = false;

    while idx < len {
        let entry = entries.add(idx);
        (*entry) = render_set_entry(*key_tags.add(idx), *key_data.add(idx));

        if !overflow {
            if idx > 0 {
                total_len = match total_len.checked_add(1) {
                    Some(val) => val,
                    None => {
                        overflow = true;
                        total_len
                    }
                };
            }

            total_len = match total_len.checked_add((*entry).len) {
                Some(val) => val,
                None => {
                    overflow = true;
                    total_len
                }
            };
        }

        idx += 1;
    }

    if overflow {
        release_entries(entries, len);
        return null_mut();
    }

    let total_with_null = match total_len.checked_add(1) {
        Some(val) => val,
        None => {
            release_entries(entries, len);
            return null_mut();
        }
    };

    let dst = _allocate(total_with_null as u64);
    if dst.is_null() {
        release_entries(entries, len);
        return null_mut();
    }

    let mut offset = 0usize;
    *dst.add(offset) = b'#';
    offset += 1;
    *dst.add(offset) = b'{';
    offset += 1;

    idx = 0usize;
    while idx < len {
        let entry = entries.add(idx);
        if idx > 0 {
            *dst.add(offset) = b' ';
            offset += 1;
        }

        if !(*entry).ptr.is_null() && (*entry).len > 0 {
            copy_nonoverlapping((*entry).ptr, dst.add(offset), (*entry).len);
            offset += (*entry).len;
        }

        idx += 1;
    }

    *dst.add(offset) = b'}';
    offset += 1;
    *dst.add(offset) = 0;

    release_entries(entries, len);
    dst
}

/// Helper function to compare two tagged values for set element equality
unsafe fn set_elements_equal(left_tag: u8, left_val: i64, right_tag: u8, right_val: i64) -> bool {
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
/// The caller must ensure that `left` and `right` are either null or point to sets created by the runtime.
/// Returns 1 if the sets are equal, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn _set_equals(left: *const u8, right: *const u8) -> i64 {
    // Fast path: same pointer
    if left == right {
        return 1;
    }

    // If either is null, they're not equal (we already checked if both are the same)
    if left.is_null() || right.is_null() {
        return 0;
    }

    // Sets are implemented as maps, so use map structure
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

    // Get pointers to key data (sets only use keys, values are just presence indicators)
    let left_key_data = map_key_data_ptr(left_header);
    let left_key_tags = map_key_tags_ptr(left_header);

    // For each element in left set, check if it exists in right set
    let mut idx = 0usize;
    while idx < len {
        let left_element = *left_key_data.add(idx);
        let left_tag = *left_key_tags.add(idx);

        // Check if this element exists in the right set
        let contains = _set_contains(right, left_element, left_tag as i64);
        if contains == 0 {
            return 0; // Element not found in right set
        }

        idx += 1;
    }

    1
}
