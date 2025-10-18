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
            if block_size >= (needed + HEADER_SIZE) as u64 {
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

/// # Safety
///
/// The caller must ensure that both `a` and `b` are either null or point to
/// NUL-terminated UTF-8 byte sequences allocated within the managed heap. The result
/// must eventually be released with `_free`. Providing invalid pointers leads to
/// undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn _string_concat_2(a: *const u8, b: *const u8) -> *mut u8 {
    if a.is_null() || b.is_null() {
        return null_mut();
    }

    let len_a = _string_count(a) as usize;
    let len_b = _string_count(b) as usize;
    let total = len_a.saturating_add(len_b).saturating_add(1);

    let dst = _allocate(total as u64);
    if dst.is_null() {
        return null_mut();
    }

    let mut i = 0;
    while i < len_a {
        *dst.add(i) = *a.add(i);
        i += 1;
    }

    let mut j = 0;
    while j < len_b {
        *dst.add(len_a + j) = *b.add(j);
        j += 1;
    }

    *dst.add(total - 1) = 0;
    dst
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
