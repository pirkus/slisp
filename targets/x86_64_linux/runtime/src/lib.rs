#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

use core::arch::asm;

#[cfg(feature = "telemetry")]
const SYS_WRITE: isize = 1;
#[cfg(feature = "telemetry")]
const STDOUT_FD: usize = 1;

mod allocator;
pub use allocator::{_allocate, _free, _heap_init};

mod strings;
pub use strings::{_string_clone, _string_concat_n, _string_count, _string_from_boolean, _string_from_number, _string_get, _string_normalize, _string_subs, FALSE_LITERAL, NIL_LITERAL, TRUE_LITERAL};

mod vector;
pub use vector::{_vector_clone, _vector_count, _vector_create, _vector_free, _vector_get, _vector_slice, _vector_to_string};

#[cfg(not(feature = "std"))]
mod memory;

#[cfg(not(feature = "std"))]
pub use memory::{bcmp, memcmp, memcpy, memmove, memset, rust_eh_personality};

#[cfg(feature = "telemetry")]
mod telemetry;

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
pub(crate) unsafe fn syscall6(number: isize, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize, arg6: usize) -> isize {
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

    #[test]
    fn vector_runtime_roundtrip() {
        unsafe {
            let values = [1i64, 2, 3, 4];
            let vec_ptr = _vector_create(values.as_ptr(), core::ptr::null(), values.len() as u64);
            assert!(!vec_ptr.is_null());
            assert_eq!(_vector_count(vec_ptr), 4);

            let mut out = 0i64;
            assert_eq!(_vector_get(vec_ptr, 2, &mut out), 1);
            assert_eq!(out, 3);

            let clone_ptr = _vector_clone(vec_ptr);
            assert!(!clone_ptr.is_null());
            assert_eq!(_vector_count(clone_ptr), 4);
            assert_eq!(_vector_get(clone_ptr, 1, &mut out), 1);
            assert_eq!(out, 2);

            let slice_ptr = _vector_slice(vec_ptr, 1, 3);
            assert!(!slice_ptr.is_null());
            assert_eq!(_vector_count(slice_ptr), 2);
            assert_eq!(_vector_get(slice_ptr, 0, &mut out), 1);
            assert_eq!(out, 2);
            assert_eq!(_vector_get(slice_ptr, 1, &mut out), 1);
            assert_eq!(out, 3);
            assert_eq!(_vector_get(slice_ptr, 2, &mut out), 0);

            _vector_free(slice_ptr);
            _vector_free(clone_ptr);
            _vector_free(vec_ptr);
        }
    }
}
