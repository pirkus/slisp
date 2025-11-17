use core::slice;

fn write_string(ptr: *const u8) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        let len = crate::_string_count(ptr) as usize;
        if len == 0 {
            return;
        }
        let bytes = slice::from_raw_parts(ptr, len);
        crate::stdout_write(bytes);
    }
}

#[no_mangle]
pub extern "C" fn _print_values(strings: *const *const u8, count: u64, newline: i64) -> i64 {
    unsafe {
        if count > 0 && !strings.is_null() {
            let mut idx = 0usize;
            while idx < count as usize {
                if idx > 0 {
                    crate::stdout_write(b" ");
                }
                let current = *strings.add(idx);
                write_string(current);
                idx += 1;
            }
        }

        if newline != 0 {
            crate::stdout_write(b"\n");
        }
    }

    0
}

#[no_mangle]
pub extern "C" fn _printf_values(format_ptr: *const u8, args_ptr: *const *const u8, arg_count: u64) -> i64 {
    if format_ptr.is_null() {
        return 0;
    }

    unsafe {
        let mut offset = 0usize;
        let mut arg_index = 0usize;
        let total_args = arg_count as usize;

        loop {
            let byte = *format_ptr.add(offset);
            if byte == 0 {
                break;
            }

            if byte != b'%' {
                let mut plain_end = offset;
                while *format_ptr.add(plain_end) != 0 && *format_ptr.add(plain_end) != b'%' {
                    plain_end += 1;
                }
                if plain_end > offset {
                    let slice = slice::from_raw_parts(format_ptr.add(offset), plain_end - offset);
                    crate::stdout_write(slice);
                }
                offset = plain_end;
                continue;
            }

            let placeholder_start = offset;
            offset += 1;
            let next = *format_ptr.add(offset);
            if next == 0 {
                crate::stdout_write(b"%");
                break;
            }

            if next == b'%' {
                crate::stdout_write(b"%");
                offset += 1;
                continue;
            }

            let mut spec_idx = offset;
            while *format_ptr.add(spec_idx) != 0 && !(*format_ptr.add(spec_idx)).is_ascii_alphabetic() {
                spec_idx += 1;
            }

            let spec_char = *format_ptr.add(spec_idx);
            if spec_char == 0 {
                let remaining = slice::from_raw_parts(format_ptr.add(placeholder_start), spec_idx - placeholder_start);
                crate::stdout_write(remaining);
                break;
            }

            let lower = spec_char.to_ascii_lowercase();
            if lower == b'n' {
                crate::stdout_write(b"\n");
                offset = spec_idx + 1;
                continue;
            }

            if args_ptr.is_null() || arg_index >= total_args {
                let literal = slice::from_raw_parts(format_ptr.add(placeholder_start), (spec_idx + 1) - placeholder_start);
                crate::stdout_write(literal);
                offset = spec_idx + 1;
                continue;
            }

            let value_ptr = *args_ptr.add(arg_index);
            write_string(value_ptr);
            arg_index += 1;
            offset = spec_idx + 1;
        }
    }

    0
}
