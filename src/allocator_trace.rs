#[cfg(feature = "allocator-telemetry")]
use slisp_runtime::{AllocatorTelemetryCounters, AllocatorTelemetryEvent, ALLOCATOR_EVENT_ALLOC, ALLOCATOR_EVENT_FLAG_REUSED, ALLOCATOR_EVENT_FREE};

#[cfg(feature = "allocator-telemetry")]
use std::fmt::Write;

#[cfg(feature = "allocator-telemetry")]
pub fn is_available() -> bool {
    true
}

#[cfg(not(feature = "allocator-telemetry"))]
pub fn is_available() -> bool {
    false
}

#[cfg(feature = "allocator-telemetry")]
pub fn set_enabled(enabled: bool) {
    if enabled {
        slisp_runtime::_allocator_telemetry_reset();
        slisp_runtime::_allocator_telemetry_enable(1);
    } else {
        slisp_runtime::_allocator_telemetry_enable(0);
        slisp_runtime::_allocator_telemetry_reset();
    }
}

#[cfg(not(feature = "allocator-telemetry"))]
pub fn set_enabled(_enabled: bool) {}

#[cfg(feature = "allocator-telemetry")]
pub fn prepare_run() {
    slisp_runtime::_allocator_telemetry_reset();
}

#[cfg(not(feature = "allocator-telemetry"))]
pub fn prepare_run() {}

#[cfg(feature = "allocator-telemetry")]
pub fn collect_report() -> Option<String> {
    let mut counters = AllocatorTelemetryCounters::default();
    unsafe {
        slisp_runtime::_allocator_telemetry_counters(&mut counters as *mut _);
    }

    let mut buffer = vec![AllocatorTelemetryEvent::default(); 256];
    let mut events = Vec::new();

    loop {
        let copied = unsafe { slisp_runtime::_allocator_telemetry_drain(buffer.as_mut_ptr(), buffer.len() as u64) } as usize;

        if copied == 0 {
            break;
        }

        events.extend_from_slice(&buffer[..copied]);

        if copied < buffer.len() {
            break;
        }
    }

    if counters.total_allocations == 0 && counters.total_frees == 0 {
        return None;
    }

    let mut output = String::new();
    let _ = writeln!(
        output,
        "[allocator] allocations={} frees={} reused={} outstanding={} peak={} dropped={}",
        counters.total_allocations, counters.total_frees, counters.total_reuses, counters.outstanding, counters.peak_outstanding, counters.events_dropped,
    );

    for event in events {
        let label = match event.kind {
            ALLOCATOR_EVENT_ALLOC => "alloc",
            ALLOCATOR_EVENT_FREE => "free",
            _ => "event",
        };

        let mut line = format!("[allocator] {:<5} ptr=0x{:016x} size={} live_after={}", label, event.ptr, event.size, event.in_use_after,);

        if event.kind == ALLOCATOR_EVENT_ALLOC && (event.flags & ALLOCATOR_EVENT_FLAG_REUSED) != 0 {
            line.push_str(" reused");
        }

        output.push_str(&line);
        output.push('\n');
    }

    Some(output)
}

#[cfg(not(feature = "allocator-telemetry"))]
pub fn collect_report() -> Option<String> {
    None
}
