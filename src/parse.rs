//! The parse engine: a faithful port of `cJSON.c`'s parser internals.
//!
//! All functions operate directly on the raw C ABI structs so that the
//! original test code can drive them exactly as it drives the C originals.

use core::ffi::{c_char, c_double, c_int};
use core::ptr;

use crate::alloc::{cjson_alloc, cjson_dealloc, cjson_new_item, cstr_len, current_hooks};
use crate::manip::cjson_delete;
use crate::model::*;

unsafe extern "C" {
    fn strtod(nptr: *const c_char, endptr: *mut *mut c_char) -> c_double;
}

/// Minimal view of `struct lconv`; we only ever read `decimal_point`, which is
/// guaranteed to be its first member.
#[repr(C)]
struct Lconv {
    decimal_point: *mut c_char,
}

unsafe extern "C" {
    fn localeconv() -> *mut Lconv;
}

/// `get_decimal_point()`: the decimal-point character of the current locale.
pub unsafe fn get_decimal_point() -> u8 {
    let lconv = localeconv();
    if lconv.is_null() {
        return b'.';
    }
    let p = (*lconv).decimal_point;
    if p.is_null() {
        return b'.';
    }
    *p as u8
}

/// The global error state backing `cJSON_GetErrorPtr`.
static mut GLOBAL_ERROR: GlobalError = GlobalError {
    json: ptr::null(),
    position: 0,
};

/// `cJSON_GetErrorPtr()`.
pub unsafe fn get_error_ptr() -> *const c_char {
    if GLOBAL_ERROR.json.is_null() {
        ptr::null()
    } else {
        (GLOBAL_ERROR.json.wrapping_add(GLOBAL_ERROR.position)) as *const c_char
    }
}

#[inline]
unsafe fn can_read(buffer: &ParseBuffer, size: usize) -> bool {
    buffer.offset + size <= buffer.length
}

#[inline]
unsafe fn can_access_at_index(buffer: &ParseBuffer, index: usize) -> bool {
    buffer.offset + index < buffer.length
}

#[inline]
unsafe fn buffer_at_offset(buffer: &ParseBuffer) -> *const u8 {
    buffer.content.add(buffer.offset)
}

/// `buffer_skip_whitespace` (bug-for-bug compatible, including the final
/// `offset--` when the whole buffer is whitespace).
pub unsafe fn buffer_skip_whitespace(buffer: *mut ParseBuffer) -> *mut ParseBuffer {
    if buffer.is_null() {
        return ptr::null_mut();
    }
    let buf = &mut *buffer;
    if buf.content.is_null() {
        return ptr::null_mut();
    }
    if !can_access_at_index(buf, 0) {
        return buffer;
    }
    while can_access_at_index(buf, 0) && *buffer_at_offset(buf) <= 32 {
        buf.offset += 1;
    }
    if buf.offset == buf.length {
        buf.offset -= 1;
    }
    buffer
}

/// `skip_utf8_bom`.
pub unsafe fn skip_utf8_bom(buffer: *mut ParseBuffer) -> *mut ParseBuffer {
    if buffer.is_null() {
        return ptr::null_mut();
    }
    let buf = &mut *buffer;
    if buf.content.is_null() || buf.offset != 0 {
        return ptr::null_mut();
    }
    if can_access_at_index(buf, 4)
        && core::slice::from_raw_parts(buffer_at_offset(buf), 3) == b"\xEF\xBB\xBF"
    {
        buf.offset += 3;
    }
    buffer
}

unsafe fn matches_at(buffer: &ParseBuffer, text: &[u8]) -> bool {
    if buffer.offset + text.len() > buffer.length {
        return false;
    }
    core::slice::from_raw_parts(buffer_at_offset(buffer), text.len()) == text
}

/// `parse_hex4`.
pub unsafe fn parse_hex4(input: *const u8) -> u32 {
    let mut h: u32 = 0;
    for i in 0..4usize {
        let c = *input.add(i);
        if (b'0'..=b'9').contains(&c) {
            h += (c - b'0') as u32;
        } else if (b'A'..=b'F').contains(&c) {
            h += 10 + (c - b'A') as u32;
        } else if (b'a'..=b'f').contains(&c) {
            h += 10 + (c - b'a') as u32;
        } else {
            return 0;
        }
        if i < 3 {
            h <<= 4;
        }
    }
    h
}

/// `utf16_literal_to_utf8`: converts a `\uXXXX` (or surrogate pair) literal to
/// UTF-8 bytes at `*output_pointer`, advancing it. Returns the number of input
/// bytes consumed (0 on failure).
unsafe fn utf16_literal_to_utf8(
    input_pointer: *const u8,
    input_end: *const u8,
    output_pointer: &mut *mut u8,
) -> u32 {
    if (input_end as usize - input_pointer as usize) < 6 {
        return 0;
    }
    let first_code = parse_hex4(input_pointer.add(2));

    if (0xDC00..=0xDFFF).contains(&first_code) {
        return 0;
    }

    let (codepoint, sequence_length): (u64, u32) = if (0xD800..=0xDBFF).contains(&first_code) {
        let second_sequence = input_pointer.add(6);
        if (input_end as usize - second_sequence as usize) < 6 {
            return 0;
        }
        if *second_sequence != b'\\' || *second_sequence.add(1) != b'u' {
            return 0;
        }
        let second_code = parse_hex4(second_sequence.add(2));
        if !(0xDC00..=0xDFFF).contains(&second_code) {
            return 0;
        }
        (
            0x10000u64 + (((first_code & 0x3FF) as u64) << 10) | (second_code & 0x3FF) as u64,
            12,
        )
    } else {
        (first_code as u64, 6)
    };

    let utf8_length: u8;
    let mut first_byte_mark: u8 = 0;
    if codepoint < 0x80 {
        utf8_length = 1;
    } else if codepoint < 0x800 {
        utf8_length = 2;
        first_byte_mark = 0xC0;
    } else if codepoint < 0x10000 {
        utf8_length = 3;
        first_byte_mark = 0xE0;
    } else if codepoint <= 0x10FFFF {
        utf8_length = 4;
        first_byte_mark = 0xF0;
    } else {
        return 0;
    }

    let mut cp = codepoint;
    let mut pos = utf8_length as i32 - 1;
    while pos > 0 {
        (*output_pointer)
            .add(pos as usize)
            .write(((cp as u8) | 0x80) & 0xBF);
        cp >>= 6;
        pos -= 1;
    }
    if utf8_length > 1 {
        (*output_pointer).write(((cp as u8) | first_byte_mark) & 0xFF);
    } else {
        (*output_pointer).write(cp as u8 & 0x7F);
    }
    *output_pointer = (*output_pointer).add(utf8_length as usize);

    sequence_length
}

/// `parse_number`.
pub unsafe fn parse_number(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    if input_buffer.is_null() {
        return 0;
    }
    let buf = &mut *input_buffer;
    if buf.content.is_null() {
        return 0;
    }

    let decimal_point = get_decimal_point();
    let mut number_string_length: usize = 0;
    let mut has_decimal_point = false;

    loop {
        if !can_access_at_index(buf, number_string_length) {
            break;
        }
        match *buffer_at_offset(buf).add(number_string_length) {
            b'0'..=b'9' | b'+' | b'-' | b'e' | b'E' => number_string_length += 1,
            b'.' => {
                number_string_length += 1;
                has_decimal_point = true;
            }
            _ => break,
        }
    }

    let number_c_string = cjson_alloc(&buf.hooks, number_string_length + 1) as *mut u8;
    if number_c_string.is_null() {
        return 0;
    }
    ptr::copy_nonoverlapping(buffer_at_offset(buf), number_c_string, number_string_length);
    *number_c_string.add(number_string_length) = 0;

    if has_decimal_point {
        for i in 0..number_string_length {
            if *number_c_string.add(i) == b'.' {
                *number_c_string.add(i) = decimal_point;
            }
        }
    }

    let mut after_end: *mut c_char = ptr::null_mut();
    let number = strtod(number_c_string as *const c_char, &mut after_end);
    if number_c_string as *const c_char == after_end {
        cjson_alloc_free(&buf.hooks, number_c_string);
        return 0;
    }

    (*item).valuedouble = number;

    if number >= i32::MAX as f64 {
        (*item).valueint = i32::MAX;
    } else if number <= i32::MIN as f64 {
        (*item).valueint = i32::MIN;
    } else {
        (*item).valueint = number as c_int;
    }

    (*item).type_ = CJSON_NUMBER;

    buf.offset += after_end as usize - number_c_string as usize;
    cjson_alloc_free(&buf.hooks, number_c_string);
    1
}

/// `parse_string`.
pub unsafe fn parse_string(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    if input_buffer.is_null() {
        return 0;
    }
    let buf = &mut *input_buffer;
    if buf.content.is_null() {
        return 0;
    }
    let base = buf.content;
    let at = buffer_at_offset(buf);
    let start_offset = at as usize - base as usize;

    if *at != b'"' {
        // C: goto fail (input_pointer == buffer_at_offset + 1)
        buf.offset = start_offset + 1;
        return 0;
    }

    let mut input_pointer = at.add(1);
    let mut input_end = at.add(1);
    let output: *mut u8;

    {
        let mut skipped_bytes: usize = 0;
        while (input_end as usize - base as usize) < buf.length && *input_end != b'"' {
            if *input_end == b'\\' {
                if (input_end.add(1) as usize - base as usize) >= buf.length {
                    // C: goto fail (input_pointer == buffer_at_offset + 1)
                    buf.offset = start_offset + 1;
                    return 0;
                }
                skipped_bytes += 1;
                input_end = input_end.add(1);
            }
            input_end = input_end.add(1);
        }
        if (input_end as usize - base as usize) >= buf.length || *input_end != b'"' {
            // C: goto fail (input_pointer == buffer_at_offset + 1)
            buf.offset = start_offset + 1;
            return 0;
        }
        let allocation_length = (input_end as usize - at as usize) - skipped_bytes;
        output = cjson_alloc(&buf.hooks, allocation_length + 1) as *mut u8;
        if output.is_null() {
            // C: goto fail (input_pointer == buffer_at_offset + 1)
            buf.offset = start_offset + 1;
            return 0;
        }
    }

    let mut output_pointer = output;
    'parse: loop {
        while input_pointer < input_end {
            if *input_pointer != b'\\' {
                *output_pointer = *input_pointer;
                output_pointer = output_pointer.add(1);
                input_pointer = input_pointer.add(1);
            } else {
                break;
            }
        }
        if input_pointer >= input_end {
            break;
        }
        // escape sequence
        let mut sequence_length: u32 = 2;
        if (input_end as usize - input_pointer as usize) < 1 {
            break 'parse;
        }
        match *input_pointer.add(1) {
            b'b' => {
                *output_pointer = 8;
                output_pointer = output_pointer.add(1);
            }
            b'f' => {
                *output_pointer = 12;
                output_pointer = output_pointer.add(1);
            }
            b'n' => {
                *output_pointer = 10;
                output_pointer = output_pointer.add(1);
            }
            b'r' => {
                *output_pointer = 13;
                output_pointer = output_pointer.add(1);
            }
            b't' => {
                *output_pointer = 9;
                output_pointer = output_pointer.add(1);
            }
            b'"' | b'\\' | b'/' => {
                *output_pointer = *input_pointer.add(1);
                output_pointer = output_pointer.add(1);
            }
            b'u' => {
                sequence_length =
                    utf16_literal_to_utf8(input_pointer, input_end, &mut output_pointer);
                if sequence_length == 0 {
                    break 'parse;
                }
            }
            _ => break 'parse,
        }
        input_pointer = input_pointer.add(sequence_length as usize);
    }

    if input_pointer < input_end {
        // failure: the parse loop aborted early
        if !output.is_null() {
            cjson_alloc_free(&buf.hooks, output);
        }
        buf.offset = input_pointer as usize - base as usize;
        return 0;
    }

    *output_pointer = 0;

    (*item).type_ = CJSON_STRING;
    (*item).valuestring = output as *mut c_char;

    buf.offset = (input_end as usize - base as usize) + 1;
    1
}

/// `parse_value`.
pub unsafe fn parse_value(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    if input_buffer.is_null() {
        return 0;
    }
    let buf = &mut *input_buffer;
    if buf.content.is_null() {
        return 0;
    }

    if can_read(buf, 4) && matches_at(buf, b"null") {
        (*item).type_ = CJSON_NULL;
        buf.offset += 4;
        return 1;
    }
    if can_read(buf, 5) && matches_at(buf, b"false") {
        (*item).type_ = CJSON_FALSE;
        buf.offset += 5;
        return 1;
    }
    if can_read(buf, 4) && matches_at(buf, b"true") {
        (*item).type_ = CJSON_TRUE;
        (*item).valueint = 1;
        buf.offset += 4;
        return 1;
    }
    if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b'"' {
        return parse_string(item, input_buffer);
    }
    if can_access_at_index(buf, 0) {
        let c = *buffer_at_offset(buf);
        if c == b'-' || (b'0'..=b'9').contains(&c) {
            return parse_number(item, input_buffer);
        }
    }
    if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b'[' {
        return parse_array(item, input_buffer);
    }
    if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b'{' {
        return parse_object(item, input_buffer);
    }

    0
}

/// `parse_array`.
pub unsafe fn parse_array(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    if input_buffer.is_null() {
        return 0;
    }
    let buf = &mut *input_buffer;
    if buf.content.is_null() {
        return 0;
    }
    let mut head: *mut CJson = ptr::null_mut();
    let mut current_item: *mut CJson = ptr::null_mut();

    if buf.depth >= CJSON_NESTING_LIMIT {
        return 0;
    }
    buf.depth += 1;

    if *buffer_at_offset(buf) != b'[' {
        return 0;
    }

    buf.offset += 1;
    buffer_skip_whitespace(input_buffer);
    if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b']' {
        // empty array -> success
        buf.depth -= 1;
        (*item).type_ = CJSON_ARRAY;
        (*item).child = head;
        buf.offset += 1;
        return 1;
    }

    if !can_access_at_index(buf, 0) {
        buf.offset -= 1;
        return 0;
    }

    buf.offset -= 1;
    'parse: loop {
        let new_item = cjson_new_item(&buf.hooks);
        if new_item.is_null() {
            // C: goto fail (allocation failure)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }
        if head.is_null() {
            current_item = new_item;
            head = new_item;
        } else {
            (*current_item).next = new_item;
            (*new_item).prev = current_item;
            current_item = new_item;
        }

        buf.offset += 1;
        buffer_skip_whitespace(input_buffer);
        if parse_value(current_item, input_buffer) == 0 {
            // C: goto fail (failed to parse value)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }
        buffer_skip_whitespace(input_buffer);
        if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b',' {
            continue;
        }
        break 'parse;
    }

    if !can_access_at_index(buf, 0) || *buffer_at_offset(buf) != b']' {
        if !head.is_null() {
            cjson_delete(head);
        }
        return 0;
    }

    buf.depth -= 1;
    if !head.is_null() {
        (*head).prev = current_item;
    }
    (*item).type_ = CJSON_ARRAY;
    (*item).child = head;
    buf.offset += 1;
    1
}

/// `parse_object`.
pub unsafe fn parse_object(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    if input_buffer.is_null() {
        return 0;
    }
    let buf = &mut *input_buffer;
    if buf.content.is_null() {
        return 0;
    }
    let mut head: *mut CJson = ptr::null_mut();
    let mut current_item: *mut CJson = ptr::null_mut();

    if buf.depth >= CJSON_NESTING_LIMIT {
        return 0;
    }
    buf.depth += 1;

    if !can_access_at_index(buf, 0) || *buffer_at_offset(buf) != b'{' {
        return 0;
    }

    buf.offset += 1;
    buffer_skip_whitespace(input_buffer);
    if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b'}' {
        buf.depth -= 1;
        (*item).type_ = CJSON_OBJECT;
        (*item).child = head;
        buf.offset += 1;
        return 1;
    }

    if !can_access_at_index(buf, 0) {
        buf.offset -= 1;
        return 0;
    }

    buf.offset -= 1;
    'parse: loop {
        let new_item = cjson_new_item(&buf.hooks);
        if new_item.is_null() {
            // C: goto fail (allocation failure)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }
        if head.is_null() {
            current_item = new_item;
            head = new_item;
        } else {
            (*current_item).next = new_item;
            (*new_item).prev = current_item;
            current_item = new_item;
        }

        if !can_access_at_index(buf, 1) {
            // C: goto fail (nothing comes after the comma)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }

        buf.offset += 1;
        buffer_skip_whitespace(input_buffer);
        if parse_string(current_item, input_buffer) == 0 {
            // C: goto fail (failed to parse name)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }
        buffer_skip_whitespace(input_buffer);

        // swap valuestring and string, because we parsed the name
        (*current_item).string = (*current_item).valuestring;
        (*current_item).valuestring = ptr::null_mut();

        if !can_access_at_index(buf, 0) || *buffer_at_offset(buf) != b':' {
            // C: goto fail (invalid object)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }

        buf.offset += 1;
        buffer_skip_whitespace(input_buffer);
        if parse_value(current_item, input_buffer) == 0 {
            // C: goto fail (failed to parse value)
            if !head.is_null() {
                cjson_delete(head);
            }
            return 0;
        }
        buffer_skip_whitespace(input_buffer);
        if can_access_at_index(buf, 0) && *buffer_at_offset(buf) == b',' {
            continue;
        }
        break 'parse;
    }

    if !can_access_at_index(buf, 0) || *buffer_at_offset(buf) != b'}' {
        if !head.is_null() {
            cjson_delete(head);
        }
        return 0;
    }

    buf.depth -= 1;
    if !head.is_null() {
        (*head).prev = current_item;
    }
    (*item).type_ = CJSON_OBJECT;
    (*item).child = head;
    buf.offset += 1;
    1
}

/// `cJSON_ParseWithOpts`.
pub unsafe fn cjson_parse_with_opts(
    value: *const c_char,
    return_parse_end: *mut *const c_char,
    require_null_terminated: CJsonBool,
) -> *mut CJson {
    if value.is_null() {
        return ptr::null_mut();
    }
    let buffer_length = cstr_len(value as *const u8) + 1;
    cjson_parse_with_length_opts(
        value,
        buffer_length,
        return_parse_end,
        require_null_terminated,
    )
}

/// `cJSON_ParseWithLengthOpts`.
pub unsafe fn cjson_parse_with_length_opts(
    value: *const c_char,
    buffer_length: usize,
    return_parse_end: *mut *const c_char,
    require_null_terminated: CJsonBool,
) -> *mut CJson {
    let mut buffer = ParseBuffer {
        content: ptr::null(),
        length: 0,
        offset: 0,
        depth: 0,
        hooks: InternalHooks {
            allocate: None,
            deallocate: None,
            reallocate: None,
        },
    };
    let item: *mut CJson;

    // reset error position
    GLOBAL_ERROR = GlobalError {
        json: ptr::null(),
        position: 0,
    };

    if value.is_null() || buffer_length == 0 {
        return finish_parse_failure(value, &buffer, return_parse_end);
    }

    buffer.content = value as *const u8;
    buffer.length = buffer_length;
    buffer.offset = 0;
    buffer.hooks = current_hooks();

    item = cjson_new_item(&buffer.hooks);
    if item.is_null() {
        return finish_parse_failure(value, &buffer, return_parse_end);
    }

    let ws = buffer_skip_whitespace(skip_utf8_bom(&mut buffer));
    if parse_value(item, ws) == 0 {
        cjson_delete(item);
        return finish_parse_failure(value, &buffer, return_parse_end);
    }

    if require_null_terminated != 0 {
        buffer_skip_whitespace(&mut buffer);
        if buffer.offset >= buffer.length || *buffer_at_offset(&buffer) != 0 {
            cjson_delete(item);
            return finish_parse_failure(value, &buffer, return_parse_end);
        }
    }

    if !return_parse_end.is_null() {
        *return_parse_end = buffer_at_offset(&buffer) as *const c_char;
    }

    item
}

/// Shared failure path of `cJSON_ParseWithLengthOpts`.
unsafe fn finish_parse_failure(
    value: *const c_char,
    buffer: &ParseBuffer,
    return_parse_end: *mut *const c_char,
) -> *mut CJson {
    if !value.is_null() {
        let mut local_error = GlobalError {
            json: value as *const u8,
            position: 0,
        };
        if buffer.offset < buffer.length {
            local_error.position = buffer.offset;
        } else if buffer.length > 0 {
            local_error.position = buffer.length - 1;
        }
        if !return_parse_end.is_null() {
            *return_parse_end =
                (local_error.json).wrapping_add(local_error.position) as *const c_char;
        }
        GLOBAL_ERROR = local_error;
    }
    ptr::null_mut()
}

/// `cJSON_Parse`.
pub unsafe fn cjson_parse(value: *const c_char) -> *mut CJson {
    cjson_parse_with_opts(value, ptr::null_mut(), 0)
}

/// `cJSON_ParseWithLength`.
pub unsafe fn cjson_parse_with_length(value: *const c_char, buffer_length: usize) -> *mut CJson {
    cjson_parse_with_length_opts(value, buffer_length, ptr::null_mut(), 0)
}

/// Free a pointer allocated through `hooks` (used locally where `cJSON_free`
/// semantics are needed but the global hooks may differ).
unsafe fn cjson_alloc_free(hooks: &InternalHooks, ptr: *mut u8) {
    cjson_dealloc(hooks, ptr as *mut core::ffi::c_void);
}
