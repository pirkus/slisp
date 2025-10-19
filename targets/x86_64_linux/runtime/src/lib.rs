#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

use core::arch::asm;
use core::ptr::null_mut;

const HEAP_SIZE: usize = 1 << 20; // 1MB
const PROT_READ: usize = 0x1;
const PROT_WRITE: usize = 0x2;
const MAP_PRIVATE: usize = 0x02;
const MAP_ANONYMOUS: usize = 0x20;
const SYS_MMAP: isize = 9;
const ALLOCATED_BIT: u64 = 0x8000_0000_0000_0000;
const HEADER_SIZE: usize = 8;

#[repr(C)]
struct FreeBlock {
    size: u64,
    next: *mut FreeBlock,
}

static mut HEAP_BASE: *mut u8 = null_mut();
static mut HEAP_END: *mut u8 = null_mut();
static mut FREE_LIST_HEAD: *mut FreeBlock = null_mut();
static mut HEAP_INITIALIZED: bool = false;
static TRUE_LITERAL: [u8; 5] = *b"true\0";
static FALSE_LITERAL: [u8; 6] = *b"false\0";
static NIL_LITERAL: [u8; 4] = *b"nil\0";

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic_handler(_: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("pause");
        }
    }
}

#[inline(always)]
unsafe fn syscall6(number: isize, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize, arg6: usize) -> isize {
    let mut ret = number;
    asm!(
        "syscall",
        inlateout("rax") ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack, preserves_flags),
    );
    ret
}

#[inline(always)]
unsafe fn mmap(addr: usize, length: usize, prot: usize, flags: usize, fd: usize, offset: usize) -> *mut u8 {
    let result = syscall6(SYS_MMAP, addr, length, prot, flags, fd, offset);
    if result < 0 {
        null_mut()
    } else {
        result as *mut u8
    }
}

#[inline(always)]
fn align_up_8(value: usize) -> usize {
    (value + 7) & !7
}

fn count_decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

unsafe fn ensure_heap() -> bool {
    if HEAP_INITIALIZED {
        return true;
    }

    let ptr = mmap(0, HEAP_SIZE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, usize::MAX, 0);

    if ptr.is_null() {
        return false;
    }

    HEAP_BASE = ptr;
    HEAP_END = ptr.add(HEAP_SIZE);

    let block = HEAP_BASE as *mut FreeBlock;
    (*block).size = (HEAP_SIZE - HEADER_SIZE) as u64;
    (*block).next = null_mut();
    FREE_LIST_HEAD = block;
    HEAP_INITIALIZED = true;
    true
}

#[no_mangle]
pub extern "C" fn _heap_init() -> i32 {
    unsafe {
        if ensure_heap() {
            0
        } else {
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn _allocate(size: u64) -> *mut u8 {
    unsafe {
        if size == 0 {
            return null_mut();
        }

        if !ensure_heap() {
            return null_mut();
        }

        let needed = align_up_8(size as usize);
        let mut prev: *mut FreeBlock = null_mut();
        let mut current = FREE_LIST_HEAD;

        while !current.is_null() {
            let block_size = (*current).size & !ALLOCATED_BIT;
            if block_size >= needed as u64 {
                let remaining = block_size - needed as u64;

                if remaining > HEADER_SIZE as u64 {
                    // Split the block: current serves the allocation, remainder becomes a new free block
                    let split_ptr = (current as *mut u8).add(HEADER_SIZE + needed) as *mut FreeBlock;
                    (*split_ptr).size = (remaining - HEADER_SIZE as u64) & !ALLOCATED_BIT;
                    (*split_ptr).next = (*current).next;

                    if prev.is_null() {
                        FREE_LIST_HEAD = split_ptr;
                    } else {
                        (*prev).next = split_ptr;
                    }

                    (*current).size = (needed as u64) | ALLOCATED_BIT;
                    (*current).next = null_mut();

                    return (current as *mut u8).add(HEADER_SIZE);
                } else {
                    // Not enough space to split; consume entire block
                    let next = (*current).next;
                    if prev.is_null() {
                        FREE_LIST_HEAD = next;
                    } else {
                        (*prev).next = next;
                    }

                    (*current).size = block_size | ALLOCATED_BIT;
                    (*current).next = null_mut();

                    return (current as *mut u8).add(HEADER_SIZE);
                }
            }

            prev = current;
            current = (*current).next;
        }

        null_mut()
    }
}

/// # Safety
///
/// The caller must ensure that `ptr` either points to memory previously allocated by
/// `_allocate` (and not yet freed) or is null. Passing arbitrary pointers or freeing
/// the same allocation twice results in undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn _free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    if ptr < HEAP_BASE || ptr >= HEAP_END {
        return;
    }

    let block_ptr = ptr.sub(HEADER_SIZE) as *mut FreeBlock;
    let size = (*block_ptr).size & !ALLOCATED_BIT;
    (*block_ptr).size = size;
    (*block_ptr).next = FREE_LIST_HEAD;
    FREE_LIST_HEAD = block_ptr;
}

/// # Safety
///
/// The caller must ensure that `ptr` is either null or points to a valid
/// NUL-terminated UTF-8 byte sequence. Passing a non-terminated or dangling pointer
/// causes the function to read beyond the allocation, resulting in undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn _string_count(ptr: *const u8) -> u64 {
    if ptr.is_null() {
        return 0;
    }

    let mut offset = 0usize;
    loop {
        let byte = *ptr.add(offset);
        if byte == 0 {
            return offset as u64;
        }
        offset += 1;
    }
}

unsafe fn string_concat_impl(parts: *const *const u8, count: usize) -> *mut u8 {
    if count == 0 {
        let dst = _allocate(1);
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

    let dst = _allocate(total as u64);
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

/// # Safety
///
/// The caller must ensure that `parts` is either null or points to an array of
/// `count` pointers where each pointer references a NUL-terminated UTF-8 string
/// allocated within the managed heap. The result must be released with `_free`.
/// Passing invalid pointers results in undefined behavior.
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

/// # Safety
///
/// The caller must ensure that `src` is either null or points to a
/// NUL-terminated UTF-8 string allocated within the managed heap. The result
/// must be released with `_free`. Passing an invalid pointer leads to undefined
/// behavior.
#[no_mangle]
pub unsafe extern "C" fn _string_clone(src: *const u8) -> *mut u8 {
    if src.is_null() {
        return null_mut();
    }

    let len = _string_count(src) as usize;
    let total = len.saturating_add(1);

    let dst = _allocate(total as u64);
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
        let dst = _allocate((len as u64) + 1);
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

/// # Safety
///
/// The caller must ensure that `src` is either null or points to a NUL-terminated UTF-8 string
/// allocated by the managed heap. `index` must represent a valid character position. When the
/// index is out of bounds or allocation fails, the function returns null. The caller owns the
/// returned pointer and must release it with `_free`.
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

    let dst = _allocate(2);
    if dst.is_null() {
        return null_mut();
    }

    *dst = *src.add(idx);
    *dst.add(1) = 0;
    dst
}

/// # Safety
///
/// The caller must ensure that `src` is either null or points to a NUL-terminated UTF-8 string
/// allocated within the managed heap. `start` and `end` describe the byte range to slice. If `end`
/// is negative, the range extends to the end of the string. When the indices are invalid or
/// allocation fails, the function returns null. The caller owns the returned pointer and must
/// release it with `_free`.
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

    let dst = _allocate(total as u64);
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

#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dest.add(i) = *src.add(i);
        i += 1;
    }
    dest
}

#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest as usize <= src as usize {
        memcpy(dest, src, n)
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            *dest.add(i) = *src.add(i);
        }
        dest
    }
}

#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut u8, value: i32, n: usize) -> *mut u8 {
    let byte = value as u8;
    let mut i = 0;
    while i < n {
        *dest.add(i) = byte;
        i += 1;
    }
    dest
}

#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let av = *a.add(i);
        let bv = *b.add(i);
        if av != bv {
            return av as i32 - bv as i32;
        }
        i += 1;
    }
    0
}

#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    memcmp(a, b, n)
}

#[cfg(not(feature = "std"))]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_from_number_returns_pointer() {
        unsafe {
            let ptr = _string_from_number(42);
            assert!(!ptr.is_null());
            assert_eq!(_string_count(ptr), 2);
            let extra = _allocate(16);
            assert!(!extra.is_null());
            let literal: &[u8] = b"Result: \0";
            let parts = [literal.as_ptr(), ptr];
            let combined = _string_concat_n(parts.as_ptr(), 2);
            assert!(!combined.is_null());
            assert_eq!(_string_count(combined), 10);
            _free(ptr as *mut u8);
            _free(extra);
            _free(combined);
        }
    }
}
