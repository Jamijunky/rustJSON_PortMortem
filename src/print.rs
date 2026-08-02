//! The print engine: a faithful port of `cJSON.c`'s printer internals,
//! including the buffer growth semantics of `ensure` and the `%g` number
//! formatting via `snprintf`, exactly as the reference does.

use core::ffi::{c_char, c_double, c_int};
use core::ptr;

use crate::alloc::{cjson_alloc, cstr_len, current_hooks};
use crate::model::*;

unsafe extern "C" {
    fn strtod(nptr: *const c_char, endptr: *mut *mut c_char) -> c_double;
    fn snprintf(s: *mut c_char, n: usize, format: *const c_char, ...) -> c_int;
}

/// The decimal-point character of the current locale (`.`, in the C locale).
pub unsafe fn get_decimal_point() -> u8 {
    crate::parse::get_decimal_point()
}

/// C's `DBL_EPSILON`.
const DBL_EPSILON: f64 = f64::EPSILON;

/// `cjson_min` macro.
#[inline]
fn cjson_min(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}

/// `ensure`: make sure there is room for `needed` more bytes in the buffer.
pub unsafe fn ensure(p: *mut PrintBuffer, needed: usize) -> *mut u8 {
    if p.is_null() || (*p).buffer.is_null() {
        return ptr::null_mut();
    }

    if (*p).length > 0 && (*p).offset >= (*p).length {
        // make sure that offset is valid
        return ptr::null_mut();
    }

    if needed > i32::MAX as usize {
        // sizes bigger than INT_MAX are currently not supported
        return ptr::null_mut();
    }

    let needed = needed + (*p).offset + 1;
    if needed <= (*p).length {
        return (*p).buffer.add((*p).offset);
    }

    if (*p).noalloc != 0 {
        return ptr::null_mut();
    }

    // calculate new buffer size
    let newsize: usize;
    if needed > (i32::MAX as usize / 2) {
        // overflow of int, use INT_MAX if possible
        if needed <= i32::MAX as usize {
            newsize = i32::MAX as usize;
        } else {
            return ptr::null_mut();
        }
    } else {
        newsize = needed * 2;
    }

    let newbuffer: *mut u8;
    if let Some(reallocate) = (*p).hooks.reallocate {
        // reallocate with realloc if available
        newbuffer = reallocate((*p).buffer as *mut core::ffi::c_void, newsize) as *mut u8;
        if newbuffer.is_null() {
            if let Some(deallocate) = (*p).hooks.deallocate {
                deallocate((*p).buffer as *mut core::ffi::c_void);
            }
            (*p).length = 0;
            (*p).buffer = ptr::null_mut();
            return ptr::null_mut();
        }
    } else {
        // otherwise reallocate manually
        newbuffer = match (*p).hooks.allocate {
            Some(allocate) => allocate(newsize) as *mut u8,
            None => ptr::null_mut(),
        };
        if newbuffer.is_null() {
            if let Some(deallocate) = (*p).hooks.deallocate {
                deallocate((*p).buffer as *mut core::ffi::c_void);
            }
            (*p).length = 0;
            (*p).buffer = ptr::null_mut();
            return ptr::null_mut();
        }

        ptr::copy_nonoverlapping((*p).buffer, newbuffer, (*p).offset + 1);
        if let Some(deallocate) = (*p).hooks.deallocate {
            deallocate((*p).buffer as *mut core::ffi::c_void);
        }
    }
    (*p).length = newsize;
    (*p).buffer = newbuffer;

    newbuffer.add((*p).offset)
}

/// `update_offset`: calculate the new length of the string in a printbuffer
/// and update the offset.
pub unsafe fn update_offset(buffer: *mut PrintBuffer) {
    if buffer.is_null() || (*buffer).buffer.is_null() {
        return;
    }
    let buffer_pointer = (*buffer).buffer.add((*buffer).offset);
    (*buffer).offset += cstr_len(buffer_pointer);
}

/// `compare_double`: secure comparison of floating-point variables.
pub unsafe fn compare_double(a: f64, b: f64) -> bool {
    let max_val = if a.abs() > b.abs() { a.abs() } else { b.abs() };
    (a - b).abs() <= max_val * DBL_EPSILON
}

/// Render `d` with C's `%1.<p>g` semantics into `number_buffer`, returning the
/// string length, or -1 if it would not fit.
unsafe fn format_number_g(d: f64, precision: usize, number_buffer: &mut [u8; 26]) -> isize {
    let format = match precision {
        15 => b"%1.15g\0".as_ptr() as *const c_char,
        17 => b"%1.17g\0".as_ptr() as *const c_char,
        _ => return -1,
    };
    let length = snprintf(number_buffer.as_mut_ptr() as *mut c_char, 26, format, d);
    if length < 0 || length as usize >= number_buffer.len() {
        return -1;
    }
    length as isize
}

/// `print_number`.
pub unsafe fn print_number(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    if output_buffer.is_null() {
        return 0;
    }

    let d = (*item).valuedouble;
    let decimal_point = get_decimal_point();
    let mut number_buffer = [0u8; 26];
    let mut length: isize;

    // This checks for NaN and Infinity
    if d.is_nan() || d.is_infinite() {
        number_buffer[..4].copy_from_slice(b"null");
        length = 4;
    } else if d == (*item).valueint as f64 {
        let s = format!("{}", (*item).valueint);
        let bl = s.len();
        number_buffer[..bl].copy_from_slice(s.as_bytes());
        length = bl as isize;
    } else {
        // Try 15 decimal places of precision to avoid nonsignificant nonzero digits
        length = format_number_g(d, 15, &mut number_buffer);

        // Check whether the original double can be recovered
        let mut round_trip_ok = false;
        if length >= 0 {
            let start = number_buffer.as_ptr() as *const c_char;
            let mut end: *mut c_char = ptr::null_mut();
            let test = strtod(start, &mut end);
            round_trip_ok = end as *const c_char != start && compare_double(test, d);
        }
        if !round_trip_ok {
            // If not, print with 17 decimal places of precision
            length = format_number_g(d, 17, &mut number_buffer);
        }
    }

    // sprintf failed or buffer overrun occurred
    if length < 0 || (length as usize) > (number_buffer.len() - 1) {
        return 0;
    }

    // reserve appropriate space in the output
    let output_pointer = ensure(output_buffer, length as usize + 1);
    if output_pointer.is_null() {
        return 0;
    }

    // copy the printed number to the output and replace locale
    // dependent decimal point with '.'
    for i in 0..(length as usize) {
        if number_buffer[i] == decimal_point {
            *output_pointer.add(i) = b'.';
            continue;
        }
        *output_pointer.add(i) = number_buffer[i];
    }
    *output_pointer.add(length as usize) = 0;

    (*output_buffer).offset += length as usize;

    1
}

/// `print_string_ptr`: render a cstring as an escaped, quoted string.
pub unsafe fn print_string_ptr(input: *const u8, output_buffer: *mut PrintBuffer) -> CJsonBool {
    let mut input_pointer: *const u8;
    let output: *mut u8;
    let mut output_pointer: *mut u8;
    let output_length: usize;
    let mut escape_characters: usize = 0;

    if output_buffer.is_null() {
        return 0;
    }

    // empty string
    if input.is_null() {
        output = ensure(output_buffer, 3);
        if output.is_null() {
            return 0;
        }
        ptr::copy_nonoverlapping(b"\"\"\0".as_ptr(), output, 3);
        return 1;
    }

    // set "flag" to 1 if something needs to be escaped
    input_pointer = input;
    while *input_pointer != 0 {
        match *input_pointer {
            b'"' | b'\\' | 8 | 12 | 10 | 13 | 9 => {
                // one character escape sequence
                escape_characters += 1;
            }
            _ => {
                if *input_pointer < 32 {
                    // UTF-16 escape sequence uXXXX
                    escape_characters += 5;
                }
            }
        }
        input_pointer = input_pointer.add(1);
    }
    output_length = (input_pointer as usize - input as usize) + escape_characters;

    output = ensure(output_buffer, output_length + 3);
    if output.is_null() {
        return 0;
    }

    // no characters have to be escaped
    if escape_characters == 0 {
        *output = b'"';
        ptr::copy_nonoverlapping(input, output.add(1), output_length);
        *output.add(output_length + 1) = b'"';
        *output.add(output_length + 2) = 0;
        return 1;
    }

    *output = b'"';
    output_pointer = output.add(1);
    input_pointer = input;
    while *input_pointer != 0 {
        if *input_pointer > 31 && *input_pointer != b'"' && *input_pointer != b'\\' {
            // normal character, copy
            *output_pointer = *input_pointer;
        } else {
            // character needs to be escaped
            *output_pointer = b'\\';
            output_pointer = output_pointer.add(1);
            match *input_pointer {
                b'\\' => *output_pointer = b'\\',
                b'"' => *output_pointer = b'"',
                8 => *output_pointer = b'b',
                12 => *output_pointer = b'f',
                10 => *output_pointer = b'n',
                13 => *output_pointer = b'r',
                9 => *output_pointer = b't',
                _ => {
                    // escape and print as unicode codepoint
                    let hex = format!("u{:04x}", *input_pointer);
                    ptr::copy_nonoverlapping(hex.as_ptr(), output_pointer, 5);
                    output_pointer = output_pointer.add(4);
                }
            }
        }
        input_pointer = input_pointer.add(1);
        output_pointer = output_pointer.add(1);
    }
    *output.add(output_length + 1) = b'"';
    *output.add(output_length + 2) = 0;

    1
}

/// `print_string`: invoke `print_string_ptr` on an item's valuestring.
pub unsafe fn print_string(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    print_string_ptr((*item).valuestring as *const u8, output_buffer)
}

/// `print_value`.
pub unsafe fn print_value(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    let output: *mut u8;

    if item.is_null() || output_buffer.is_null() {
        return 0;
    }

    match (*item).base_type() {
        CJSON_NULL => {
            output = ensure(output_buffer, 5);
            if output.is_null() {
                return 0;
            }
            ptr::copy_nonoverlapping(b"null\0".as_ptr(), output, 5);
            1
        }
        CJSON_FALSE => {
            output = ensure(output_buffer, 6);
            if output.is_null() {
                return 0;
            }
            ptr::copy_nonoverlapping(b"false\0".as_ptr(), output, 6);
            1
        }
        CJSON_TRUE => {
            output = ensure(output_buffer, 5);
            if output.is_null() {
                return 0;
            }
            ptr::copy_nonoverlapping(b"true\0".as_ptr(), output, 5);
            1
        }
        CJSON_NUMBER => print_number(item, output_buffer),
        CJSON_RAW => {
            if (*item).valuestring.is_null() {
                return 0;
            }
            let raw_length = cstr_len((*item).valuestring as *const u8) + 1;
            output = ensure(output_buffer, raw_length);
            if output.is_null() {
                return 0;
            }
            ptr::copy_nonoverlapping((*item).valuestring as *const u8, output, raw_length);
            1
        }
        CJSON_STRING => print_string(item, output_buffer),
        CJSON_ARRAY => print_array(item, output_buffer),
        CJSON_OBJECT => print_object(item, output_buffer),
        _ => 0,
    }
}

/// `print_array`.
pub unsafe fn print_array(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    let mut output_pointer: *mut u8;
    let mut length: usize;
    let mut current_element = (*item).child;

    if output_buffer.is_null() {
        return 0;
    }

    if (*output_buffer).depth >= CJSON_NESTING_LIMIT {
        return 0; // nesting is too deep
    }

    // Compose the output array.
    // opening square bracket
    output_pointer = ensure(output_buffer, 1);
    if output_pointer.is_null() {
        return 0;
    }

    *output_pointer = b'[';
    (*output_buffer).offset += 1;
    (*output_buffer).depth += 1;

    while !current_element.is_null() {
        if print_value(current_element, output_buffer) == 0 {
            return 0;
        }
        update_offset(output_buffer);
        if !(*current_element).next.is_null() {
            length = if (*output_buffer).format != 0 { 2 } else { 1 };
            output_pointer = ensure(output_buffer, length + 1);
            if output_pointer.is_null() {
                return 0;
            }
            *output_pointer = b',';
            output_pointer = output_pointer.add(1);
            if (*output_buffer).format != 0 {
                *output_pointer = b' ';
                output_pointer = output_pointer.add(1);
            }
            *output_pointer = 0;
            (*output_buffer).offset += length;
        }
        current_element = (*current_element).next;
    }

    output_pointer = ensure(output_buffer, 2);
    if output_pointer.is_null() {
        return 0;
    }
    *output_pointer = b']';
    output_pointer = output_pointer.add(1);
    *output_pointer = 0;
    (*output_buffer).depth -= 1;

    1
}

/// `print_object`.
pub unsafe fn print_object(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    let mut output_pointer: *mut u8;
    let mut length: usize;
    let mut current_item = (*item).child;

    if output_buffer.is_null() {
        return 0;
    }

    if (*output_buffer).depth >= CJSON_NESTING_LIMIT {
        return 0; // nesting is too deep
    }

    // Compose the output object:
    length = if (*output_buffer).format != 0 { 2 } else { 1 }; // fmt: {\n
    output_pointer = ensure(output_buffer, length + 1);
    if output_pointer.is_null() {
        return 0;
    }

    *output_pointer = b'{';
    (*output_buffer).depth += 1;
    if (*output_buffer).format != 0 {
        *output_pointer.add(1) = b'\n';
    }
    (*output_buffer).offset += length;

    while !current_item.is_null() {
        if (*output_buffer).format != 0 {
            let mut i: usize = 0;
            output_pointer = ensure(output_buffer, (*output_buffer).depth);
            if output_pointer.is_null() {
                return 0;
            }
            while i < (*output_buffer).depth {
                *output_pointer = b'\t';
                output_pointer = output_pointer.add(1);
                i += 1;
            }
            (*output_buffer).offset += (*output_buffer).depth;
        }

        // print key
        if print_string_ptr((*current_item).string as *const u8, output_buffer) == 0 {
            return 0;
        }
        update_offset(output_buffer);

        length = if (*output_buffer).format != 0 { 2 } else { 1 };
        output_pointer = ensure(output_buffer, length);
        if output_pointer.is_null() {
            return 0;
        }
        *output_pointer = b':';
        if (*output_buffer).format != 0 {
            *output_pointer.add(1) = b'\t';
        }
        (*output_buffer).offset += length;

        // print value
        if print_value(current_item, output_buffer) == 0 {
            return 0;
        }
        update_offset(output_buffer);

        // print comma if not last
        length = if (*output_buffer).format != 0 { 1 } else { 0 }
            + if (*current_item).next.is_null() { 0 } else { 1 };
        output_pointer = ensure(output_buffer, length + 1);
        if output_pointer.is_null() {
            return 0;
        }
        if !(*current_item).next.is_null() {
            *output_pointer = b',';
            output_pointer = output_pointer.add(1);
        }

        if (*output_buffer).format != 0 {
            *output_pointer = b'\n';
            output_pointer = output_pointer.add(1);
        }
        *output_pointer = 0;
        (*output_buffer).offset += length;

        current_item = (*current_item).next;
    }

    output_pointer = ensure(
        output_buffer,
        if (*output_buffer).format != 0 {
            (*output_buffer).depth + 1
        } else {
            2
        },
    );
    if output_pointer.is_null() {
        return 0;
    }
    if (*output_buffer).format != 0 {
        let mut i: usize = 0;
        while i < ((*output_buffer).depth - 1) {
            *output_pointer = b'\t';
            output_pointer = output_pointer.add(1);
            i += 1;
        }
    }
    *output_pointer = b'}';
    output_pointer = output_pointer.add(1);
    *output_pointer = 0;
    (*output_buffer).depth -= 1;

    1
}

/// The internal `print` function.
unsafe fn print_impl(item: *const CJson, format: CJsonBool, hooks: &InternalHooks) -> *mut u8 {
    const DEFAULT_BUFFER_SIZE: usize = 256;
    let mut buffer = PrintBuffer {
        buffer: ptr::null_mut(),
        length: 0,
        offset: 0,
        depth: 0,
        noalloc: 0,
        format: format,
        hooks: *hooks,
    };
    let printed: *mut u8;

    // create buffer
    buffer.buffer = match hooks.allocate {
        Some(allocate) => allocate(DEFAULT_BUFFER_SIZE) as *mut u8,
        None => ptr::null_mut(),
    };
    buffer.length = DEFAULT_BUFFER_SIZE;
    if buffer.buffer.is_null() {
        return ptr::null_mut();
    }

    // print the value
    if print_value(item, &mut buffer) == 0 {
        if let Some(deallocate) = hooks.deallocate {
            deallocate(buffer.buffer as *mut core::ffi::c_void);
        }
        return ptr::null_mut();
    }
    update_offset(&mut buffer);

    // check if reallocate is available
    if let Some(reallocate) = hooks.reallocate {
        printed = reallocate(buffer.buffer as *mut core::ffi::c_void, buffer.offset + 1) as *mut u8;
        if printed.is_null() {
            if let Some(deallocate) = hooks.deallocate {
                deallocate(buffer.buffer as *mut core::ffi::c_void);
            }
            return ptr::null_mut();
        }
    } else {
        // otherwise copy the JSON over to a new buffer
        printed = match hooks.allocate {
            Some(allocate) => allocate(buffer.offset + 1) as *mut u8,
            None => ptr::null_mut(),
        };
        if printed.is_null() {
            if let Some(deallocate) = hooks.deallocate {
                deallocate(buffer.buffer as *mut core::ffi::c_void);
            }
            return ptr::null_mut();
        }
        ptr::copy_nonoverlapping(
            buffer.buffer,
            printed,
            cjson_min(buffer.length, buffer.offset + 1),
        );
        *printed.add(buffer.offset) = 0; // just to be sure

        // free the buffer
        if let Some(deallocate) = hooks.deallocate {
            deallocate(buffer.buffer as *mut core::ffi::c_void);
        }
    }

    printed
}

/// `cJSON_Print`.
pub unsafe fn cjson_print(item: *const CJson) -> *mut c_char {
    print_impl(item, 1, &current_hooks()) as *mut c_char
}

/// `cJSON_PrintUnformatted`.
pub unsafe fn cjson_print_unformatted(item: *const CJson) -> *mut c_char {
    print_impl(item, 0, &current_hooks()) as *mut c_char
}

/// `cJSON_PrintBuffered`.
pub unsafe fn cjson_print_buffered(
    item: *const CJson,
    prebuffer: c_int,
    fmt: CJsonBool,
) -> *mut c_char {
    let mut p = PrintBuffer {
        buffer: ptr::null_mut(),
        length: 0,
        offset: 0,
        depth: 0,
        noalloc: 0,
        format: 0,
        hooks: current_hooks(),
    };

    if prebuffer < 0 {
        return ptr::null_mut();
    }

    p.buffer = cjson_alloc(&p.hooks, prebuffer as usize) as *mut u8;
    if p.buffer.is_null() {
        return ptr::null_mut();
    }

    p.length = prebuffer as usize;
    p.offset = 0;
    p.noalloc = 0;
    p.format = fmt;

    if print_value(item, &mut p) == 0 {
        cjson_alloc_free(&p.hooks, p.buffer);
        return ptr::null_mut();
    }

    p.buffer as *mut c_char
}

/// `cJSON_PrintPreallocated`.
pub unsafe fn cjson_print_preallocated(
    item: *const CJson,
    buffer: *mut c_char,
    length: c_int,
    format: CJsonBool,
) -> CJsonBool {
    let mut p = PrintBuffer {
        buffer: ptr::null_mut(),
        length: 0,
        offset: 0,
        depth: 0,
        noalloc: 0,
        format: 0,
        hooks: current_hooks(),
    };

    if length < 0 || buffer.is_null() {
        return 0;
    }

    p.buffer = buffer as *mut u8;
    p.length = length as usize;
    p.offset = 0;
    p.noalloc = 1;
    p.format = format;

    print_value(item, &mut p)
}

/// Free a pointer allocated through `hooks`.
unsafe fn cjson_alloc_free(hooks: &InternalHooks, ptr: *mut u8) {
    crate::alloc::cjson_dealloc(hooks, ptr as *mut core::ffi::c_void);
}
