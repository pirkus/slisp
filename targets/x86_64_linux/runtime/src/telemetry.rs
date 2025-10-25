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
