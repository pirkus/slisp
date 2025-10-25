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

#[inline(always)]
fn align_up_8(value: usize) -> usize {
    (value + 7) & !7
}

#[inline(always)]
unsafe fn mmap(addr: usize, length: usize, prot: usize, flags: usize, fd: usize, offset: usize) -> *mut u8 {
    let result = crate::syscall6(SYS_MMAP, addr, length, prot, flags, fd, offset);
    if result < 0 {
        null_mut()
    } else {
        result as *mut u8
    }
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
                    crate::telemetry::record_alloc(user_ptr, needed as u64);
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
                    crate::telemetry::record_alloc(user_ptr, needed as u64);
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
    crate::telemetry::record_free(ptr, size);
}
