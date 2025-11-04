use core::mem::size_of;
use core::ptr::{copy_nonoverlapping, null_mut};

use crate::{_allocate, _free, _map_to_string, _string_clone, _string_count, _string_from_number, FALSE_LITERAL, NIL_LITERAL, TRUE_LITERAL};

#[repr(C)]
pub(crate) struct VectorHeader {
    length: u64,
    capacity: u64,
}

const TAG_NIL: u8 = 0;
const TAG_NUMBER: u8 = 1;
const TAG_BOOLEAN: u8 = 2;
const TAG_STRING: u8 = 3;
const TAG_VECTOR: u8 = 4;
const TAG_MAP: u8 = 5;
pub(crate) const TAG_ANY: u8 = 0xff;

#[repr(C)]
struct ElementRender {
    ptr: *mut u8,
    len: usize,
    owned: bool,
}

#[inline]
pub(crate) fn padded_tag_bytes(len: usize) -> usize {
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
unsafe fn vector_tags_ptr(vec: *const VectorHeader) -> *const u8 {
    (vec as *const u8).add(size_of::<VectorHeader>())
}

#[inline]
pub(crate) unsafe fn vector_tags_ptr_mut(vec: *mut VectorHeader) -> *mut u8 {
    (vec as *mut u8).add(size_of::<VectorHeader>())
}

#[inline]
fn vector_allocation_size(len: usize) -> Option<usize> {
    let header = size_of::<VectorHeader>();
    let value_bytes = len.checked_mul(size_of::<i64>())?;
    header.checked_add(padded_tag_bytes(len))?.checked_add(value_bytes)
}

#[inline]
unsafe fn vector_data_ptr(vec: *const VectorHeader) -> *const i64 {
    let len = (*vec).length as usize;
    let offset = size_of::<VectorHeader>() + padded_tag_bytes(len);
    (vec as *const u8).add(offset) as *const i64
}

#[inline]
pub(crate) unsafe fn vector_data_ptr_mut(vec: *mut VectorHeader) -> *mut i64 {
    let len = (*vec).length as usize;
    let offset = size_of::<VectorHeader>() + padded_tag_bytes(len);
    (vec as *mut u8).add(offset) as *mut i64
}

pub(crate) unsafe fn vector_allocate(len: usize) -> *mut VectorHeader {
    match vector_allocation_size(len) {
        Some(total) => {
            let raw = _allocate(total as u64);
            if raw.is_null() {
                null_mut()
            } else {
                let header = raw as *mut VectorHeader;
                (*header).length = len as u64;
                (*header).capacity = len as u64;
                if len > 0 {
                    let tags_ptr = vector_tags_ptr_mut(header);
                    let tag_bytes = padded_tag_bytes(len);
                    let mut idx = 0usize;
                    while idx < tag_bytes {
                        *tags_ptr.add(idx) = TAG_ANY;
                        idx += 1;
                    }
                }
                header
            }
        }
        None => null_mut(),
    }
}

unsafe fn release_element_renders(entries: *mut ElementRender, len: usize) {
    if entries.is_null() {
        return;
    }

    let mut idx = 0usize;
    while idx < len {
        let entry = entries.add(idx);
        if (*entry).owned && !(*entry).ptr.is_null() {
            _free((*entry).ptr);
        }
        idx += 1;
    }
}

unsafe fn materialize_element_render(value: i64, tag: u8) -> ElementRender {
    match tag {
        TAG_NIL => ElementRender {
            ptr: NIL_LITERAL.as_ptr() as *mut u8,
            len: 3,
            owned: false,
        },
        TAG_BOOLEAN => {
            if value == 0 {
                ElementRender {
                    ptr: FALSE_LITERAL.as_ptr() as *mut u8,
                    len: 5,
                    owned: false,
                }
            } else {
                ElementRender {
                    ptr: TRUE_LITERAL.as_ptr() as *mut u8,
                    len: 4,
                    owned: false,
                }
            }
        }
        TAG_STRING => {
            if value == 0 {
                ElementRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let cloned = _string_clone(value as *const u8);
                if cloned.is_null() {
                    ElementRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    ElementRender {
                        ptr: cloned,
                        len: _string_count(cloned) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_VECTOR => {
            if value == 0 {
                ElementRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let nested = _vector_to_string(value as *const u8);
                if nested.is_null() {
                    ElementRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    ElementRender {
                        ptr: nested,
                        len: _string_count(nested) as usize,
                        owned: true,
                    }
                }
            }
        }
        TAG_MAP => {
            if value == 0 {
                ElementRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                let rendered = _map_to_string(value as *const u8);
                if rendered.is_null() {
                    ElementRender {
                        ptr: NIL_LITERAL.as_ptr() as *mut u8,
                        len: 3,
                        owned: false,
                    }
                } else {
                    ElementRender {
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
                ElementRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                ElementRender {
                    ptr: rendered,
                    len: _string_count(rendered) as usize,
                    owned: true,
                }
            }
        }
        _ => {
            let rendered = _string_from_number(value);
            if rendered.is_null() {
                ElementRender {
                    ptr: NIL_LITERAL.as_ptr() as *mut u8,
                    len: 3,
                    owned: false,
                }
            } else {
                ElementRender {
                    ptr: rendered,
                    len: _string_count(rendered) as usize,
                    owned: true,
                }
            }
        }
    }
}

/// # Safety
///
/// The caller must ensure that `elements` either points to at least `count` 64-bit values
/// previously allocated by the caller or is null when `count` is zero. The returned vector
/// resides in the managed heap and must be released with `_vector_free`.
#[no_mangle]
pub unsafe extern "C" fn _vector_create(elements: *const i64, tags: *const i64, count: u64) -> *mut u8 {
    if count > usize::MAX as u64 {
        return null_mut();
    }

    let len = count as usize;
    let vector = vector_allocate(len);
    if vector.is_null() {
        return null_mut();
    }

    if len > 0 {
        let dst_values = vector_data_ptr_mut(vector);
        if elements.is_null() {
            let mut idx = 0;
            while idx < len {
                *dst_values.add(idx) = 0;
                idx += 1;
            }
        } else {
            copy_nonoverlapping(elements, dst_values, len);
        }

        let dst_tags = vector_tags_ptr_mut(vector);
        if tags.is_null() {
            let mut idx = 0;
            while idx < len {
                *dst_tags.add(idx) = TAG_ANY;
                idx += 1;
            }
        } else {
            let mut idx = 0;
            while idx < len {
                let tag_value = *tags.add(idx);
                *dst_tags.add(idx) = (tag_value & 0xff) as u8;
                idx += 1;
            }
        }

        let padded = padded_tag_bytes(len);
        let mut idx = len;
        while idx < padded {
            *dst_tags.add(idx) = TAG_ANY;
            idx += 1;
        }
    }

    vector as *mut u8
}

/// # Safety
///
/// The caller must ensure that `vec` is either null or points to a vector created by the
/// runtime. Passing arbitrary pointers results in undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn _vector_count(vec: *const u8) -> u64 {
    if vec.is_null() {
        return 0;
    }
    let header = vec as *const VectorHeader;
    (*header).length
}

/// # Safety
///
/// The caller must ensure that `vec` is either null or points to a managed vector and that `out`
/// is either null or writable. When the index lies outside the bounds of the vector, the function
/// returns 0 without writing to `out`. On success it stores the element value into `out` and
/// returns 1.
#[no_mangle]
pub unsafe extern "C" fn _vector_get(vec: *const u8, index: i64, out: *mut i64) -> i64 {
    if vec.is_null() || out.is_null() || index < 0 {
        return 0;
    }

    let header = vec as *const VectorHeader;

    if (*header).length > usize::MAX as u64 {
        return 0;
    }

    let len = (*header).length as usize;
    let idx = index as usize;
    if idx >= len {
        return 0;
    }

    let data = vector_data_ptr(header);
    *out = *data.add(idx);
    1
}

/// # Safety
///
/// The caller must ensure that `vec` is either null or points to a managed vector. The returned
/// vector owns its storage and must be released with `_vector_free`.
#[no_mangle]
pub unsafe extern "C" fn _vector_slice(vec: *const u8, start: i64, end: i64) -> *mut u8 {
    if vec.is_null() || start < 0 {
        return null_mut();
    }

    let header = vec as *const VectorHeader;

    if (*header).length > usize::MAX as u64 {
        return null_mut();
    }

    let len = (*header).length as usize;
    let start_idx = start as usize;
    if start_idx > len {
        return null_mut();
    }

    let end_idx = if end < 0 {
        len
    } else if end < start {
        return null_mut();
    } else {
        let end_usize = end as usize;
        if end_usize > len {
            return null_mut();
        }
        end_usize
    };

    if start_idx > end_idx {
        return null_mut();
    }

    let slice_len = end_idx - start_idx;
    let new_vec = vector_allocate(slice_len);
    if new_vec.is_null() {
        return null_mut();
    }

    if slice_len > 0 {
        let src = vector_data_ptr(header);
        let dst = vector_data_ptr_mut(new_vec);
        copy_nonoverlapping(src.add(start_idx), dst, slice_len);

        let src_tags = vector_tags_ptr(header);
        let dst_tags = vector_tags_ptr_mut(new_vec);
        copy_nonoverlapping(src_tags.add(start_idx), dst_tags, slice_len);

        let padded = padded_tag_bytes(slice_len);
        let mut idx = slice_len;
        while idx < padded {
            *dst_tags.add(idx) = TAG_ANY;
            idx += 1;
        }
    }

    new_vec as *mut u8
}

/// # Safety
///
/// The caller must ensure that `vec` is either null or points to a managed vector. The returned
/// vector owns its storage and must be released with `_vector_free`.
#[no_mangle]
pub unsafe extern "C" fn _vector_clone(vec: *const u8) -> *mut u8 {
    if vec.is_null() {
        return null_mut();
    }

    let header = vec as *const VectorHeader;

    if (*header).length > usize::MAX as u64 {
        return null_mut();
    }

    let len = (*header).length as usize;
    let new_vec = vector_allocate(len);
    if new_vec.is_null() {
        return null_mut();
    }

    if len > 0 {
        let src = vector_data_ptr(header);
        let dst = vector_data_ptr_mut(new_vec);
        copy_nonoverlapping(src, dst, len);

        let src_tags = vector_tags_ptr(header);
        let dst_tags = vector_tags_ptr_mut(new_vec);
        copy_nonoverlapping(src_tags, dst_tags, len);

        let padded = padded_tag_bytes(len);
        let mut idx = len;
        while idx < padded {
            *dst_tags.add(idx) = TAG_ANY;
            idx += 1;
        }
    }

    new_vec as *mut u8
}

/// # Safety
///
/// The caller must ensure `vec` is either null or points to a managed vector. The caller owns the
/// returned string and must release it with `_free`.
#[no_mangle]
pub unsafe extern "C" fn _vector_to_string(vec: *const u8) -> *mut u8 {
    if vec.is_null() {
        let dst = _allocate(3);
        if dst.is_null() {
            return null_mut();
        }
        *dst = b'[';
        *dst.add(1) = b']';
        *dst.add(2) = 0;
        return dst;
    }

    let header = vec as *const VectorHeader;
    let len = (*header).length as usize;

    if len == 0 {
        let dst = _allocate(3);
        if dst.is_null() {
            return null_mut();
        }
        *dst = b'[';
        *dst.add(1) = b']';
        *dst.add(2) = 0;
        return dst;
    }

    let entries_size = len.checked_mul(size_of::<ElementRender>()).unwrap_or(0);
    if entries_size == 0 {
        return null_mut();
    }

    let entries_ptr = _allocate(entries_size as u64) as *mut ElementRender;
    if entries_ptr.is_null() {
        return null_mut();
    }

    let values = vector_data_ptr(header);
    let tags = vector_tags_ptr(header);

    let mut total_len = 2usize; // '[' and ']'
    let mut idx = 0usize;
    let mut overflow = false;

    while idx < len {
        let value = *values.add(idx);
        let tag = *tags.add(idx);
        let entry = materialize_element_render(value, tag);
        if !overflow {
            total_len = match total_len.checked_add(entry.len) {
                Some(val) => val,
                None => {
                    overflow = true;
                    total_len
                }
            };
            if idx > 0 {
                total_len = match total_len.checked_add(1) {
                    Some(val) => val,
                    None => {
                        overflow = true;
                        total_len
                    }
                };
            }
        }

        *entries_ptr.add(idx) = entry;
        idx += 1;
    }

    if overflow {
        release_element_renders(entries_ptr, len);
        _free(entries_ptr as *mut u8);
        return null_mut();
    }

    let total_with_null = match total_len.checked_add(1) {
        Some(val) => val,
        None => {
            release_element_renders(entries_ptr, len);
            _free(entries_ptr as *mut u8);
            return null_mut();
        }
    };

    let dst = _allocate(total_with_null as u64);
    if dst.is_null() {
        release_element_renders(entries_ptr, len);
        _free(entries_ptr as *mut u8);
        return null_mut();
    }

    let mut offset = 0usize;
    *dst.add(offset) = b'[';
    offset += 1;

    idx = 0;
    while idx < len {
        let entry = entries_ptr.add(idx);
        if !(*entry).ptr.is_null() && (*entry).len > 0 {
            copy_nonoverlapping((*entry).ptr, dst.add(offset), (*entry).len);
            offset += (*entry).len;
        }

        if idx + 1 < len {
            *dst.add(offset) = b' ';
            offset += 1;
        }

        idx += 1;
    }

    *dst.add(offset) = b']';
    offset += 1;
    *dst.add(offset) = 0;

    release_element_renders(entries_ptr, len);
    _free(entries_ptr as *mut u8);

    dst
}

/// # Safety
///
/// The caller must ensure that `vec` is either null or points to a vector returned by the runtime.
#[no_mangle]
pub unsafe extern "C" fn _vector_free(vec: *mut u8) {
    if vec.is_null() {
        return;
    }
    _free(vec);
}

// Higher-order function support
// Function pointer types: functions that take i64 args and return i64
type UnaryFn = unsafe extern "C" fn(i64) -> i64;
type BinaryFn = unsafe extern "C" fn(i64, i64) -> i64;

/// # Safety
///
/// map: Apply a function to each element of a vector
/// `func_ptr` must be a valid function pointer that takes an i64 and returns an i64
/// `vec` must be null or a valid vector pointer
#[no_mangle]
pub unsafe extern "C" fn _vector_map(func_ptr: *const u8, vec: *const u8) -> *mut u8 {
    if func_ptr.is_null() || vec.is_null() {
        return null_mut();
    }

    let header = vec as *const VectorHeader;
    let len = (*header).length as usize;

    if len == 0 {
        // Return empty vector
        return _vector_create(null_mut(), null_mut(), 0);
    }

    // Create new vector for results
    let result = vector_allocate(len);
    if result.is_null() {
        return null_mut();
    }

    let values = vector_data_ptr(header);
    let tags = vector_tags_ptr(header);
    let result_values = vector_data_ptr_mut(result);
    let result_tags = vector_tags_ptr_mut(result);

    // Cast function pointer
    let func: UnaryFn = core::mem::transmute(func_ptr);

    // Apply function to each element
    let mut idx = 0;
    while idx < len {
        let value = *values.add(idx);
        let result_value = func(value);
        *result_values.add(idx) = result_value;
        *result_tags.add(idx) = TAG_NUMBER; // For now, assume numeric results
        idx += 1;
    }

    // Pad remaining tag bytes
    let padded = padded_tag_bytes(len);
    let mut idx = len;
    while idx < padded {
        *result_tags.add(idx) = TAG_ANY;
        idx += 1;
    }

    result as *mut u8
}

/// # Safety
///
/// filter: Select elements that satisfy a predicate
/// `pred_ptr` must be a valid function pointer that takes an i64 and returns an i64 (0 = false, non-zero = true)
/// `vec` must be null or a valid vector pointer
#[no_mangle]
pub unsafe extern "C" fn _vector_filter(pred_ptr: *const u8, vec: *const u8) -> *mut u8 {
    if pred_ptr.is_null() || vec.is_null() {
        return null_mut();
    }

    let header = vec as *const VectorHeader;
    let len = (*header).length as usize;

    if len == 0 {
        return _vector_create(null_mut(), null_mut(), 0);
    }

    let values = vector_data_ptr(header);
    let tags = vector_tags_ptr(header);

    // Cast function pointer
    let pred: UnaryFn = core::mem::transmute(pred_ptr);

    // First pass: count matching elements
    let mut count = 0;
    let mut idx = 0;
    while idx < len {
        let value = *values.add(idx);
        if pred(value) != 0 {
            count += 1;
        }
        idx += 1;
    }

    if count == 0 {
        return _vector_create(null_mut(), null_mut(), 0);
    }

    // Create result vector
    let result = vector_allocate(count);
    if result.is_null() {
        return null_mut();
    }

    let result_values = vector_data_ptr_mut(result);
    let result_tags = vector_tags_ptr_mut(result);

    // Second pass: copy matching elements
    let mut result_idx = 0;
    idx = 0;
    while idx < len {
        let value = *values.add(idx);
        if pred(value) != 0 {
            *result_values.add(result_idx) = value;
            *result_tags.add(result_idx) = *tags.add(idx);
            result_idx += 1;
        }
        idx += 1;
    }

    // Pad remaining tag bytes
    let padded = padded_tag_bytes(count);
    let mut idx = count;
    while idx < padded {
        *result_tags.add(idx) = TAG_ANY;
        idx += 1;
    }

    result as *mut u8
}

/// # Safety
///
/// reduce: Fold/accumulate over a vector
/// `func_ptr` must be a valid function pointer that takes two i64s and returns an i64
/// `init` is the initial accumulator value
/// `vec` must be null or a valid vector pointer
#[no_mangle]
pub unsafe extern "C" fn _vector_reduce(func_ptr: *const u8, init: i64, vec: *const u8) -> i64 {
    if func_ptr.is_null() || vec.is_null() {
        return init;
    }

    let header = vec as *const VectorHeader;
    let len = (*header).length as usize;

    if len == 0 {
        return init;
    }

    let values = vector_data_ptr(header);

    // Cast function pointer
    let func: BinaryFn = core::mem::transmute(func_ptr);

    // Apply function to accumulate
    let mut acc = init;
    let mut idx = 0;
    while idx < len {
        let value = *values.add(idx);
        acc = func(acc, value);
        idx += 1;
    }

    acc
}

/// Prepend an element to a collection (cons)
/// Returns a new vector with the element prepended
#[no_mangle]
pub unsafe extern "C" fn _vector_cons(elem: i64, vec: *const u8) -> *mut u8 {
    if vec.is_null() {
        // cons on nil creates a single-element vector
        let result = vector_allocate(1);
        if result.is_null() {
            return null_mut();
        }

        (*result).length = 1;
        (*result).capacity = 1;

        let data = vector_data_ptr_mut(result);
        *data = elem;

        let tags = vector_tags_ptr_mut(result);
        *tags = TAG_NUMBER; // Assume number for now

        // Pad remaining tag bytes
        let tag_bytes = padded_tag_bytes(1);
        let mut idx = 1;
        while idx < tag_bytes {
            *tags.add(idx) = 0xff;
            idx += 1;
        }

        return result as *mut u8;
    }

    let header = vec as *const VectorHeader;
    let old_len = (*header).length as usize;
    let new_len = old_len + 1;

    // Allocate new vector
    let result = vector_allocate(new_len);
    if result.is_null() {
        return null_mut();
    }

    (*result).length = new_len as u64;
    (*result).capacity = new_len as u64;

    // Copy data - elem goes first
    let result_data = vector_data_ptr_mut(result);
    *result_data = elem;

    let old_data = vector_data_ptr(header);
    let mut idx = 0;
    while idx < old_len {
        *result_data.add(idx + 1) = *old_data.add(idx);
        idx += 1;
    }

    // Copy tags - assume number tag for elem
    let result_tags = vector_tags_ptr_mut(result);
    *result_tags = TAG_NUMBER;

    let old_tags = vector_tags_ptr(header);
    idx = 0;
    while idx < old_len {
        *result_tags.add(idx + 1) = *old_tags.add(idx);
        idx += 1;
    }

    // Pad remaining tag bytes
    let tag_bytes = padded_tag_bytes(new_len);
    idx = new_len;
    while idx < tag_bytes {
        *result_tags.add(idx) = 0xff;
        idx += 1;
    }

    result as *mut u8
}

/// Append an element to a collection (conj)
/// Returns a new vector with the element appended
#[no_mangle]
pub unsafe extern "C" fn _vector_conj(vec: *const u8, elem: i64) -> *mut u8 {
    if vec.is_null() {
        // conj on nil creates a single-element vector
        let result = vector_allocate(1);
        if result.is_null() {
            return null_mut();
        }

        (*result).length = 1;
        (*result).capacity = 1;

        let data = vector_data_ptr_mut(result);
        *data = elem;

        let tags = vector_tags_ptr_mut(result);
        *tags = TAG_NUMBER; // Assume number for now

        // Pad remaining tag bytes
        let tag_bytes = padded_tag_bytes(1);
        let mut idx = 1;
        while idx < tag_bytes {
            *tags.add(idx) = 0xff;
            idx += 1;
        }

        return result as *mut u8;
    }

    let header = vec as *const VectorHeader;
    let old_len = (*header).length as usize;
    let new_len = old_len + 1;

    // Allocate new vector
    let result = vector_allocate(new_len);
    if result.is_null() {
        return null_mut();
    }

    (*result).length = new_len as u64;
    (*result).capacity = new_len as u64;

    // Copy old data first
    let result_data = vector_data_ptr_mut(result);
    let old_data = vector_data_ptr(header);
    let mut idx = 0;
    while idx < old_len {
        *result_data.add(idx) = *old_data.add(idx);
        idx += 1;
    }

    // Append new element
    *result_data.add(old_len) = elem;

    // Copy old tags
    let result_tags = vector_tags_ptr_mut(result);
    let old_tags = vector_tags_ptr(header);
    idx = 0;
    while idx < old_len {
        *result_tags.add(idx) = *old_tags.add(idx);
        idx += 1;
    }

    // Add tag for new element
    *result_tags.add(old_len) = TAG_NUMBER;

    // Pad remaining tag bytes
    let tag_bytes = padded_tag_bytes(new_len);
    idx = new_len;
    while idx < tag_bytes {
        *result_tags.add(idx) = 0xff;
        idx += 1;
    }

    result as *mut u8
}
