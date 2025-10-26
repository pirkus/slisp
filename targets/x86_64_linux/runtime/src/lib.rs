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
pub use strings::{
    _string_clone, _string_concat_n, _string_count, _string_equals, _string_from_boolean, _string_from_number, _string_get, _string_normalize, _string_subs, FALSE_LITERAL, NIL_LITERAL, TRUE_LITERAL,
};

mod vector;
pub use vector::{_vector_clone, _vector_count, _vector_create, _vector_free, _vector_get, _vector_slice, _vector_to_string};

mod map;
pub use map::{_map_assoc, _map_clone, _map_contains, _map_count, _map_create, _map_dissoc, _map_free, _map_get, _map_to_string};

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
    fn string_equals_handles_null_and_content() {
        unsafe {
            let original = _string_from_number(1234);
            assert!(!original.is_null());
            assert_eq!(_string_equals(original, original), 1);

            let clone = _string_clone(original);
            assert!(!clone.is_null());
            assert_eq!(_string_equals(original, clone), 1);

            let different = _string_from_number(4321);
            assert!(!different.is_null());
            assert_eq!(_string_equals(original, different), 0);

            assert_eq!(_string_equals(core::ptr::null(), core::ptr::null()), 1);
            assert_eq!(_string_equals(original, core::ptr::null()), 0);
            assert_eq!(_string_equals(core::ptr::null(), clone), 0);

            _free(different);
            _free(clone);
            _free(original);
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

    #[test]
    fn map_runtime_roundtrip() {
        unsafe {
            const TAG_STRING: i64 = 3;
            const TAG_NUMBER: i64 = 1;

            let key_a = _string_from_number(10);
            let key_b = _string_from_number(20);
            assert!(!key_a.is_null());
            assert!(!key_b.is_null());

            let keys = [key_a as i64, key_b as i64];
            let key_tags = [TAG_STRING, TAG_STRING];
            let values = [100i64, 200i64];
            let value_tags = [TAG_NUMBER, TAG_NUMBER];

            let map_ptr = _map_create(keys.as_ptr(), key_tags.as_ptr(), values.as_ptr(), value_tags.as_ptr(), 2);
            assert!(!map_ptr.is_null());
            assert_eq!(_map_count(map_ptr), 2);

            let mut out_value = 0i64;
            let mut out_tag = 0u8;
            assert_eq!(_map_get(map_ptr, key_a as i64, TAG_STRING, &mut out_value, &mut out_tag), 1);
            assert_eq!(out_value, 100);
            assert_eq!(out_tag, TAG_NUMBER as u8);
            assert_eq!(_map_contains(map_ptr, key_b as i64, TAG_STRING), 1);

            let assoc_ptr = _map_assoc(map_ptr, key_b as i64, TAG_STRING, 999, TAG_NUMBER);
            assert!(!assoc_ptr.is_null());
            assert_eq!(_map_count(assoc_ptr), 2);
            assert_eq!(_map_get(assoc_ptr, key_b as i64, TAG_STRING, &mut out_value, &mut out_tag), 1);
            assert_eq!(out_value, 999);

            let dissoc_ptr = _map_dissoc(map_ptr, key_a as i64, TAG_STRING);
            assert!(!dissoc_ptr.is_null());
            assert_eq!(_map_count(dissoc_ptr), 1);
            assert_eq!(_map_contains(dissoc_ptr, key_a as i64, TAG_STRING), 0);

            _map_free(dissoc_ptr);
            _map_free(assoc_ptr);
            _map_free(map_ptr);
        }
    }
}
