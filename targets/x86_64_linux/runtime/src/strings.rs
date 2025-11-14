use core::ptr::null_mut;

pub static TRUE_LITERAL: [u8; 5] = *b"true\0";
pub static FALSE_LITERAL: [u8; 6] = *b"false\0";
pub static NIL_LITERAL: [u8; 4] = *b"nil\0";

fn count_decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

#[no_mangle]
pub unsafe extern "C" fn _string_count(ptr: *const u8) -> u64 {
    if ptr.is_null() {
        return 0;
    }

    let mut offset = 0usize;
    loop {
        let byte = *ptr.add(offset);
        if byte == 0 {
            let len = offset as u64;
            return len;
        }
        offset += 1;
    }
}

unsafe fn string_concat_impl(parts: *const *const u8, count: usize) -> *mut u8 {
    if count == 0 {
        let dst = crate::_allocate(1);
        if dst.is_null() {
            return null_mut();
        }
        *dst = 0;
        return dst;
    }

    if parts.is_null() {
        return null_mut();
    }

    let mut total = 1usize;
    let mut i = 0;
    while i < count {
        let part = *parts.add(i);
        if part.is_null() {
            return null_mut();
        }

        let len = _string_count(part) as usize;
        match total.checked_add(len) {
            Some(next) => total = next,
            None => return null_mut(),
        }

        i += 1;
    }

    let dst = crate::_allocate(total as u64);
    if dst.is_null() {
        return null_mut();
    }

    let mut offset = 0usize;
    let mut j = 0;
    while j < count {
        let part = *parts.add(j);
        let len = _string_count(part) as usize;

        let mut k = 0usize;
        while k < len {
            *dst.add(offset + k) = *part.add(k);
            k += 1;
        }

        offset += len;
        j += 1;
    }

    *dst.add(total - 1) = 0;
    dst
}

#[no_mangle]
pub unsafe extern "C" fn _string_concat_n(parts: *const *const u8, count: u64) -> *mut u8 {
    if count == 0 {
        return string_concat_impl(parts, 0);
    }

    if parts.is_null() {
        return null_mut();
    }

    if count > usize::MAX as u64 {
        return null_mut();
    }

    string_concat_impl(parts, count as usize)
}

#[no_mangle]
pub unsafe extern "C" fn _string_clone(src: *const u8) -> *mut u8 {
    if src.is_null() {
        return null_mut();
    }

    let len = _string_count(src) as usize;
    let total = len.saturating_add(1);

    let dst = crate::_allocate(total as u64);
    if dst.is_null() {
        return null_mut();
    }

    let mut i = 0;
    while i < total {
        *dst.add(i) = *src.add(i);
        i += 1;
    }

    dst
}

#[no_mangle]
pub unsafe extern "C" fn _string_equals(left: *const u8, right: *const u8) -> i64 {
    if left == right {
        return 1;
    }

    if left.is_null() || right.is_null() {
        return 0;
    }

    let left_len = _string_count(left) as usize;
    let right_len = _string_count(right) as usize;
    if left_len != right_len {
        return 0;
    }

    let mut idx = 0usize;
    while idx < left_len {
        if *left.add(idx) != *right.add(idx) {
            return 0;
        }
        idx += 1;
    }

    1
}

#[no_mangle]
pub extern "C" fn _string_from_boolean(value: i64) -> *mut u8 {
    if value == 0 {
        FALSE_LITERAL.as_ptr() as *mut u8
    } else {
        TRUE_LITERAL.as_ptr() as *mut u8
    }
}

#[no_mangle]
pub extern "C" fn _string_normalize(ptr: *const u8, clone_flag: i64) -> *mut u8 {
    unsafe {
        if ptr.is_null() {
            return NIL_LITERAL.as_ptr() as *mut u8;
        }

        if clone_flag != 0 {
            let cloned = _string_clone(ptr);
            if cloned.is_null() {
                return NIL_LITERAL.as_ptr() as *mut u8;
            }
            cloned
        } else {
            ptr as *mut u8
        }
    }
}

#[no_mangle]
pub extern "C" fn _string_from_number(value: i64) -> *mut u8 {
    unsafe {
        let negative = value < 0;
        let mut magnitude = if negative { value.wrapping_neg() as u64 } else { value as u64 };

        let mut digits = count_decimal_digits(magnitude);
        if magnitude == 0 {
            digits = 1;
        }

        let len = digits + if negative { 1 } else { 0 };
        let dst = crate::_allocate((len as u64) + 1);
        if dst.is_null() {
            return null_mut();
        }

        let mut write_index = len;
        *dst.add(write_index) = 0;

        if magnitude == 0 {
            write_index -= 1;
            *dst.add(write_index) = b'0';
        } else {
            while magnitude > 0 {
                let digit = (magnitude % 10) as u8;
                magnitude /= 10;
                write_index -= 1;
                *dst.add(write_index) = b'0' + digit;
            }
        }

        if negative {
            write_index -= 1;
            *dst.add(write_index) = b'-';
        }

        dst
    }
}

#[no_mangle]
pub unsafe extern "C" fn _string_get(src: *const u8, index: i64) -> *mut u8 {
    if src.is_null() || index < 0 {
        return null_mut();
    }

    let len = _string_count(src) as usize;
    let idx = index as usize;
    if idx >= len {
        return null_mut();
    }

    let dst = crate::_allocate(2);
    if dst.is_null() {
        return null_mut();
    }

    *dst = *src.add(idx);
    *dst.add(1) = 0;
    dst
}

#[no_mangle]
pub unsafe extern "C" fn _string_subs(src: *const u8, start: i64, end: i64) -> *mut u8 {
    if src.is_null() || start < 0 {
        return null_mut();
    }

    let len = _string_count(src) as usize;
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
    let total = slice_len.saturating_add(1);

    let dst = crate::_allocate(total as u64);
    if dst.is_null() {
        return null_mut();
    }

    let mut i = 0;
    while i < slice_len {
        *dst.add(i) = *src.add(start_idx + i);
        i += 1;
    }

    *dst.add(slice_len) = 0;

    dst
}
