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
#[cfg(feature = "telemetry")]
const SYS_WRITE: isize = 1;
const SYS_MMAP: isize = 9;
#[cfg(feature = "telemetry")]
const STDOUT_FD: usize = 1;
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

#[cfg(feature = "telemetry")]
mod telemetry {
    use core::cmp::min;
    use core::ptr::{copy_nonoverlapping, null_mut};
    use core::sync::atomic::{AtomicBool, Ordering};

    pub const ALLOCATOR_EVENT_ALLOC: u8 = 1;
    pub const ALLOCATOR_EVENT_FREE: u8 = 2;
    pub const ALLOCATOR_EVENT_FLAG_REUSED: u8 = 0x1;

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    pub struct AllocatorTelemetryEvent {
        pub kind: u8,
        pub flags: u8,
        pub reserved: u16,
        pub size: u64,
        pub ptr: u64,
        pub in_use_after: u64,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    pub struct AllocatorTelemetryCounters {
        pub total_allocations: u64,
        pub total_frees: u64,
        pub total_reuses: u64,
        pub outstanding: u64,
        pub peak_outstanding: u64,
        pub events_dropped: u64,
    }

    const EMPTY_EVENT: AllocatorTelemetryEvent = AllocatorTelemetryEvent {
        kind: 0,
        flags: 0,
        reserved: 0,
        size: 0,
        ptr: 0,
        in_use_after: 0,
    };

    const ZERO_COUNTERS: AllocatorTelemetryCounters = AllocatorTelemetryCounters {
        total_allocations: 0,
        total_frees: 0,
        total_reuses: 0,
        outstanding: 0,
        peak_outstanding: 0,
        events_dropped: 0,
    };

    const EVENT_CAPACITY: usize = 1024;
    const RECENT_FREE_CAPACITY: usize = 256;

    static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);
    static mut EVENTS: [AllocatorTelemetryEvent; EVENT_CAPACITY] = [EMPTY_EVENT; EVENT_CAPACITY];
    static mut EVENT_LEN: usize = 0;
    static mut COUNTERS: AllocatorTelemetryCounters = ZERO_COUNTERS;
    static mut RECENT_FREES: [*mut u8; RECENT_FREE_CAPACITY] = [null_mut(); RECENT_FREE_CAPACITY];
    static mut RECENT_FREE_COUNT: usize = 0;

    #[inline]
    pub fn set_enabled(flag: bool) {
        TELEMETRY_ENABLED.store(flag, Ordering::SeqCst);
    }

    #[inline]
    fn is_enabled() -> bool {
        TELEMETRY_ENABLED.load(Ordering::Relaxed)
    }

    pub fn reset() {
        unsafe {
            EVENT_LEN = 0;
            COUNTERS = ZERO_COUNTERS;
            RECENT_FREE_COUNT = 0;

            let mut index = 0;
            while index < RECENT_FREE_CAPACITY {
                RECENT_FREES[index] = null_mut();
                index += 1;
            }
        }
    }

    pub fn record_alloc(ptr: *mut u8, size: u64) {
        if !is_enabled() {
            return;
        }

        unsafe {
            let reused = remove_recent_free(ptr);
            COUNTERS.total_allocations = COUNTERS.total_allocations.saturating_add(1);
            if reused {
                COUNTERS.total_reuses = COUNTERS.total_reuses.saturating_add(1);
            }
            COUNTERS.outstanding = COUNTERS.outstanding.saturating_add(1);
            if COUNTERS.outstanding > COUNTERS.peak_outstanding {
                COUNTERS.peak_outstanding = COUNTERS.outstanding;
            }

            let flags = if reused { ALLOCATOR_EVENT_FLAG_REUSED } else { 0 };
            log_event(ALLOCATOR_EVENT_ALLOC, ptr, size, flags, COUNTERS.outstanding);
        }
    }

    pub fn record_free(ptr: *mut u8, size: u64) {
        if !is_enabled() {
            return;
        }

        unsafe {
            COUNTERS.total_frees = COUNTERS.total_frees.saturating_add(1);
            if COUNTERS.outstanding > 0 {
                COUNTERS.outstanding -= 1;
            }

            log_event(ALLOCATOR_EVENT_FREE, ptr, size, 0, COUNTERS.outstanding);
            add_recent_free(ptr);
        }
    }

    unsafe fn log_event(kind: u8, ptr: *mut u8, size: u64, flags: u8, in_use_after: u64) {
        if EVENT_LEN < EVENT_CAPACITY {
            EVENTS[EVENT_LEN] = AllocatorTelemetryEvent {
                kind,
                flags,
                reserved: 0,
                size,
                ptr: ptr as u64,
                in_use_after,
            };
            EVENT_LEN += 1;
        } else {
            COUNTERS.events_dropped = COUNTERS.events_dropped.saturating_add(1);
        }
    }

    unsafe fn add_recent_free(ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        let mut idx = 0;
        while idx < RECENT_FREE_COUNT {
            if RECENT_FREES[idx] == ptr {
                #[cfg(debug_assertions)]
                panic!("double free detected (telemetry) for pointer {:p}", ptr);
                #[cfg(not(debug_assertions))]
                return;
            }
            idx += 1;
        }

        if RECENT_FREE_COUNT < RECENT_FREE_CAPACITY {
            RECENT_FREES[RECENT_FREE_COUNT] = ptr;
            RECENT_FREE_COUNT += 1;
            return;
        }

        idx = 1;
        while idx < RECENT_FREE_CAPACITY {
            RECENT_FREES[idx - 1] = RECENT_FREES[idx];
            idx += 1;
        }
        RECENT_FREES[RECENT_FREE_CAPACITY - 1] = ptr;
    }

    unsafe fn remove_recent_free(ptr: *mut u8) -> bool {
        if ptr.is_null() || RECENT_FREE_COUNT == 0 {
            return false;
        }

        let mut idx = 0;
        while idx < RECENT_FREE_COUNT {
            if RECENT_FREES[idx] == ptr {
                RECENT_FREE_COUNT -= 1;
                while idx < RECENT_FREE_COUNT {
                    RECENT_FREES[idx] = RECENT_FREES[idx + 1];
                    idx += 1;
                }
                RECENT_FREES[RECENT_FREE_COUNT] = null_mut();
                return true;
            }
            idx += 1;
        }
        false
    }

    pub unsafe fn drain(out: *mut AllocatorTelemetryEvent, capacity: usize) -> usize {
        if EVENT_LEN == 0 || capacity == 0 || out.is_null() {
            return 0;
        }

        let to_copy = min(EVENT_LEN, capacity);
        let src = core::ptr::addr_of!(EVENTS) as *const AllocatorTelemetryEvent;
        copy_nonoverlapping(src, out, to_copy);

        if EVENT_LEN > to_copy {
            let remaining = EVENT_LEN - to_copy;
            let mut idx = 0;
            while idx < remaining {
                EVENTS[idx] = EVENTS[to_copy + idx];
                idx += 1;
            }
            EVENT_LEN = remaining;
        } else {
            EVENT_LEN = 0;
        }

        to_copy
    }

    pub unsafe fn copy_counters(out: *mut AllocatorTelemetryCounters) {
        if out.is_null() {
            return;
        }
        *out = COUNTERS;
    }

    pub fn counters_snapshot() -> AllocatorTelemetryCounters {
        unsafe { COUNTERS }
    }

    fn print_literal(bytes: &[u8]) {
        super::stdout_write(bytes);
    }

    fn print_decimal(mut value: u64) {
        let mut buf = [0u8; 20];
        let mut idx = buf.len();

        if value == 0 {
            idx -= 1;
            buf[idx] = b'0';
        } else {
            while value > 0 {
                let digit = (value % 10) as u8;
                value /= 10;
                idx -= 1;
                buf[idx] = b'0' + digit;
            }
        }

        super::stdout_write(&buf[idx..]);
    }

    fn print_newline() {
        super::stdout_write(b"\n");
    }

    const HEX_DIGITS: [u8; 16] = *b"0123456789abcdef";

    fn print_hex_u64(mut value: u64) {
        let mut buf = [0u8; 16];

        for slot in (0..16).rev() {
            buf[slot] = HEX_DIGITS[(value & 0x0f) as usize];
            value >>= 4;
        }

        super::stdout_write(b"0x");
        super::stdout_write(&buf);
    }

    fn event_label(kind: u8) -> &'static [u8] {
        match kind {
            ALLOCATOR_EVENT_ALLOC => b"alloc",
            ALLOCATOR_EVENT_FREE => b"free",
            _ => b"event",
        }
    }

    fn print_event(event: &AllocatorTelemetryEvent) {
        print_literal(b"[allocator] ");
        print_literal(event_label(event.kind));
        print_literal(b" ptr=");
        print_hex_u64(event.ptr);
        print_literal(b" size=");
        print_decimal(event.size);
        print_literal(b" live_after=");
        print_decimal(event.in_use_after);
        if event.kind == ALLOCATOR_EVENT_ALLOC && (event.flags & ALLOCATOR_EVENT_FLAG_REUSED) != 0 {
            print_literal(b" reused");
        }
        print_newline();
    }

    fn print_summary(counters: &AllocatorTelemetryCounters) {
        print_literal(b"[allocator] summary allocations=");
        print_decimal(counters.total_allocations);
        print_literal(b" frees=");
        print_decimal(counters.total_frees);
        print_literal(b" reused=");
        print_decimal(counters.total_reuses);
        print_literal(b" outstanding=");
        print_decimal(counters.outstanding);
        print_literal(b" peak=");
        print_decimal(counters.peak_outstanding);
        print_literal(b" dropped=");
        print_decimal(counters.events_dropped);
        print_newline();
    }

    pub fn dump_stdout() {
        let counters = counters_snapshot();
        print_summary(&counters);

        let mut buffer = [AllocatorTelemetryEvent::default(); 32];
        loop {
            let copied = unsafe { drain(buffer.as_mut_ptr(), buffer.len()) };
            if copied == 0 {
                break;
            }

            for event in buffer.iter().take(copied) {
                print_event(event);
            }
        }

        set_enabled(false);
        reset();
    }
}

#[cfg(feature = "telemetry")]
pub use telemetry::{AllocatorTelemetryCounters, AllocatorTelemetryEvent, ALLOCATOR_EVENT_ALLOC, ALLOCATOR_EVENT_FLAG_REUSED, ALLOCATOR_EVENT_FREE};

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

#[cfg(feature = "telemetry")]
fn stdout_write(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }

    unsafe {
        let _ = syscall6(SYS_WRITE, STDOUT_FD, bytes.as_ptr() as usize, bytes.len(), 0, 0, 0);
    }
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

                    let user_ptr = (current as *mut u8).add(HEADER_SIZE);
                    #[cfg(feature = "telemetry")]
                    telemetry::record_alloc(user_ptr, needed as u64);
                    return user_ptr;
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

                    let user_ptr = (current as *mut u8).add(HEADER_SIZE);
                    #[cfg(feature = "telemetry")]
                    telemetry::record_alloc(user_ptr, needed as u64);
                    return user_ptr;
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

    #[cfg(feature = "telemetry")]
    telemetry::record_free(ptr, size);
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

#[cfg(feature = "telemetry")]
#[no_mangle]
pub extern "C" fn _allocator_telemetry_enable(flag: i64) {
    telemetry::set_enabled(flag != 0);
}

#[cfg(feature = "telemetry")]
#[no_mangle]
pub extern "C" fn _allocator_telemetry_reset() {
    telemetry::reset();
}

#[cfg(feature = "telemetry")]
#[no_mangle]
pub extern "C" fn _allocator_telemetry_dump_stdout() {
    telemetry::dump_stdout();
}

#[cfg(feature = "telemetry")]
#[no_mangle]
pub unsafe extern "C" fn _allocator_telemetry_drain(out: *mut AllocatorTelemetryEvent, capacity: u64) -> u64 {
    telemetry::drain(out, capacity as usize) as u64
}

#[cfg(feature = "telemetry")]
#[no_mangle]
pub unsafe extern "C" fn _allocator_telemetry_counters(out: *mut AllocatorTelemetryCounters) -> i64 {
    if out.is_null() {
        return -1;
    }
    telemetry::copy_counters(out);
    0
}

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
