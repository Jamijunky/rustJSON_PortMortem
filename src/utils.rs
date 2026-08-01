//! `cJSON_Utils.c` ported to Rust: RFC 6901 (JSON Pointer), RFC 6902 (JSON
//! Patch), RFC 7396 (Merge Patch), object sorting and pointer discovery.
//!
//! Every function mirrors its `cJSON_Utils.c` counterpart line for line,
//! including documented quirks (e.g. `decode_pointer_inplace`).

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

use crate::alloc::cstr_len;
use crate::manip::*;
use crate::model::*;

/// `cJSONUtils_strdup`.
unsafe fn cjson_utils_strdup(string: *const u8) -> *mut u8 {
    let length = cstr_len(string) + 1;
    let copy = cjson_malloc(length) as *mut u8;
    if copy.is_null() {
        return ptr::null_mut();
    }
    ptr::copy_nonoverlapping(string, copy, length);
    copy
}

/// C `strcmp`: compare two NUL-terminated byte strings.
unsafe fn cstr_cmp(mut a: *const u8, mut b: *const u8) -> c_int {
    loop {
        let x = *a as c_int;
        let y = *b as c_int;
        if x != y || x == 0 {
            return x - y;
        }
        a = a.add(1);
        b = b.add(1);
    }
}

/// C `strrchr`: last occurrence of `ch` in a NUL-terminated string.
unsafe fn cstr_rchr(s: *const u8, ch: u8) -> *mut u8 {
    let mut last = ptr::null_mut();
    let mut p = s;
    loop {
        if *p == ch {
            last = p as *mut u8;
        }
        if *p == 0 {
            break;
        }
        p = p.add(1);
    }
    last
}

/// `compare_strings`: string comparison which doesn't consider NULL pointers equal.
unsafe fn compare_strings(
    mut string1: *const u8,
    mut string2: *const u8,
    case_sensitive: CJsonBool,
) -> c_int {
    if string1.is_null() || string2.is_null() {
        return 1;
    }
    if string1 == string2 {
        return 0;
    }
    if case_sensitive != 0 {
        return cstr_cmp(string1, string2);
    }
    loop {
        let a = (*string1).to_ascii_lowercase() as c_int;
        let b = (*string2).to_ascii_lowercase() as c_int;
        if a != b {
            return a - b;
        }
        if *string1 == 0 {
            return 0;
        }
        string1 = string1.add(1);
        string2 = string2.add(1);
    }
}

/// `compare_double`: securely comparison of floating-point variables.
unsafe fn compare_double(a: f64, b: f64) -> CJsonBool {
    let fa = a.abs();
    let fb = b.abs();
    let max_val = if fa > fb { fa } else { fb };
    ((a - b).abs() <= max_val * f64::EPSILON) as CJsonBool
}

/// `compare_pointers`: compare the next path element of two JSON pointers,
/// two NULL pointers are considered unequal.
unsafe fn compare_pointers(
    mut name: *const u8,
    mut pointer: *const u8,
    case_sensitive: CJsonBool,
) -> CJsonBool {
    if name.is_null() || pointer.is_null() {
        return 0;
    }
    while *name != 0 && *pointer != 0 && *pointer != b'/' {
        if *pointer == b'~' {
            /* check for escaped '~' (~0) and '/' (~1) */
            if ((*pointer.add(1) != b'0') || (*name != b'~'))
                && ((*pointer.add(1) != b'1') || (*name != b'/'))
            {
                /* invalid escape sequence or wrong character in *name */
                return 0;
            } else {
                pointer = pointer.add(1);
            }
        } else if (case_sensitive == 0
            && (*name).to_ascii_lowercase() != (*pointer).to_ascii_lowercase())
            || (case_sensitive != 0 && *name != *pointer)
        {
            return 0;
        }
        name = name.add(1);
        pointer = pointer.add(1);
    }
    if (*pointer != 0 && *pointer != b'/') != (*name != 0) {
        /* one string has ended, the other not */
        return 0;
    }
    1
}

/// `pointer_encoded_length`: calculate the length of a string if encoded as
/// JSON pointer with ~0 and ~1 escape sequences.
unsafe fn pointer_encoded_length(mut string: *const u8) -> usize {
    let mut length = 0usize;
    while *string != 0 {
        /* character needs to be escaped? */
        if *string == b'~' || *string == b'/' {
            length += 1;
        }
        length += 1;
        string = string.add(1);
    }
    length
}

/// `encode_string_as_pointer`: copy a string while escaping '~' and '/' with
/// ~0 and ~1 JSON pointer escape codes.
unsafe fn encode_string_as_pointer(mut destination: *mut u8, mut source: *const u8) {
    while *source != 0 {
        if *source == b'/' {
            *destination = b'~';
            *destination.add(1) = b'1';
            destination = destination.add(1);
        } else if *source == b'~' {
            *destination = b'~';
            *destination.add(1) = b'0';
            destination = destination.add(1);
        } else {
            *destination = *source;
        }
        source = source.add(1);
        destination = destination.add(1);
    }
    *destination = 0;
}

/// `cJSONUtils_FindPointerFromObjectTo`.
pub unsafe fn cjson_utils_find_pointer_from_object_to(
    object: *const CJson,
    target: *const CJson,
) -> *mut c_char {
    let mut child_index = 0usize;
    let mut current_child: *mut CJson;

    if object.is_null() || target.is_null() {
        return ptr::null_mut();
    }
    if object == target {
        /* found */
        return cjson_utils_strdup(b"\0".as_ptr()) as *mut c_char;
    }

    /* recursively search all children of the object or array */
    current_child = (*object).child;
    while !current_child.is_null() {
        let target_pointer = cjson_utils_find_pointer_from_object_to(current_child, target);
        /* found the target? */
        if !target_pointer.is_null() {
            if cjson_is_array(object) != 0 {
                /* reserve enough memory for a 64 bit integer + '/' and '\0' */
                let full_pointer =
                    cjson_malloc(cstr_len(target_pointer as *const u8) + 20 + 2) as *mut u8;
                /* "/<array_index><path>" */
                *full_pointer = b'/';
                let mut full_ptr = full_pointer.add(1);
                for &d in child_index.to_string().as_bytes() {
                    *full_ptr = d;
                    full_ptr = full_ptr.add(1);
                }
                let mut i = 0usize;
                while *target_pointer.add(i) != 0 {
                    *full_ptr = *target_pointer.add(i) as u8;
                    full_ptr = full_ptr.add(1);
                    i += 1;
                }
                *full_ptr = 0;
                cjson_free_public(target_pointer as *mut c_void);
                return full_pointer as *mut c_char;
            }

            if cjson_is_object(object) != 0 {
                let full_pointer = cjson_malloc(
                    cstr_len(target_pointer as *const u8)
                        + pointer_encoded_length((*current_child).string as *const u8)
                        + 2,
                ) as *mut u8;
                *full_pointer = b'/';
                encode_string_as_pointer(full_pointer.add(1), (*current_child).string as *const u8);
                /* strcat */
                let mut end = 0usize;
                while *full_pointer.add(end) != 0 {
                    end += 1;
                }
                let mut i = 0usize;
                while *target_pointer.add(i) != 0 {
                    *full_pointer.add(end + i) = *target_pointer.add(i) as u8;
                    i += 1;
                }
                *full_pointer.add(end + i) = 0;
                cjson_free_public(target_pointer as *mut c_void);
                return full_pointer as *mut c_char;
            }

            /* reached leaf of the tree, found nothing */
            cjson_free_public(target_pointer as *mut c_void);
            return ptr::null_mut();
        }
        current_child = (*current_child).next;
        child_index += 1;
    }

    /* not found */
    ptr::null_mut()
}

/// `get_array_item`: non broken version of `cJSON_GetArrayItem`.
unsafe fn get_array_item(array: *const CJson, mut item: usize) -> *mut CJson {
    let mut child = if array.is_null() {
        ptr::null_mut()
    } else {
        (*array).child
    };
    while !child.is_null() && item > 0 {
        item -= 1;
        child = (*child).next;
    }
    child
}

/// `decode_array_index_from_pointer`.
unsafe fn decode_array_index_from_pointer(pointer: *const u8, index: *mut usize) -> CJsonBool {
    let mut parsed_index = 0usize;
    let mut position = 0usize;

    if *pointer == b'0' && *pointer.add(1) != 0 && *pointer.add(1) != b'/' {
        /* leading zeroes are not permitted */
        return 0;
    }

    while *pointer.add(position) >= b'0' && *pointer.add(position) <= b'9' {
        parsed_index = parsed_index
            .wrapping_mul(10)
            .wrapping_add((*pointer.add(position) - b'0') as usize);
        position += 1;
    }

    if *pointer.add(position) != 0 && *pointer.add(position) != b'/' {
        return 0;
    }

    *index = parsed_index;
    1
}

/// `get_item_from_pointer`: follow a JSON pointer path from an object.
unsafe fn get_item_from_pointer(
    object: *mut CJson,
    mut pointer: *const u8,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    let mut current_element = object;

    if pointer.is_null() {
        return ptr::null_mut();
    }

    /* follow path of the pointer */
    while *pointer == b'/' && !current_element.is_null() {
        pointer = pointer.add(1);
        if cjson_is_array(current_element) != 0 {
            let mut index = 0usize;
            if decode_array_index_from_pointer(pointer, &mut index) == 0 {
                return ptr::null_mut();
            }
            current_element = get_array_item(current_element, index);
        } else if cjson_is_object(current_element) != 0 {
            current_element = (*current_element).child;
            /* GetObjectItem. */
            while !current_element.is_null()
                && compare_pointers(
                    (*current_element).string as *const u8,
                    pointer,
                    case_sensitive,
                ) == 0
            {
                current_element = (*current_element).next;
            }
        } else {
            return ptr::null_mut();
        }

        /* skip to the next path token or end of string */
        while *pointer != 0 && *pointer != b'/' {
            pointer = pointer.add(1);
        }
    }

    current_element
}

/// `cJSONUtils_GetPointer`.
pub unsafe fn cjson_utils_get_pointer(object: *mut CJson, pointer: *const u8) -> *mut CJson {
    get_item_from_pointer(object, pointer, 0)
}

/// `cJSONUtils_GetPointerCaseSensitive`.
pub unsafe fn cjson_utils_get_pointer_case_sensitive(
    object: *mut CJson,
    pointer: *const u8,
) -> *mut CJson {
    get_item_from_pointer(object, pointer, 1)
}

/// `decode_pointer_inplace` (RFC 6902 patch path decoding).
unsafe fn decode_pointer_inplace(mut string: *mut u8) {
    let mut decoded_string = string;

    if string.is_null() {
        return;
    }

    while *string != 0 {
        if *string == b'~' {
            if *string.add(1) == b'0' {
                *decoded_string = b'~';
            } else if *string.add(1) == b'1' {
                *decoded_string.add(1) = b'/';
            } else {
                /* invalid escape sequence */
                return;
            }
            string = string.add(1);
        }
        decoded_string = decoded_string.add(1);
        string = string.add(1);
    }

    *decoded_string = 0;
}

/// `detach_item_from_array`: non-broken `cJSON_DetachItemFromArray`.
unsafe fn detach_item_from_array(array: *mut CJson, mut which: usize) -> *mut CJson {
    let mut c = (*array).child;
    while !c.is_null() && which > 0 {
        c = (*c).next;
        which -= 1;
    }
    if c.is_null() {
        /* item doesn't exist */
        return ptr::null_mut();
    }
    if c != (*array).child {
        /* not the first element */
        (*(*c).prev).next = (*c).next;
    }
    if !(*c).next.is_null() {
        (*(*c).next).prev = (*c).prev;
    }
    if c == (*array).child {
        (*array).child = (*c).next;
    } else if (*c).next.is_null() {
        (*array).child.as_mut().unwrap().prev = (*c).prev;
    }
    /* make sure the detached item doesn't point anywhere anymore */
    (*c).prev = ptr::null_mut();
    (*c).next = ptr::null_mut();

    c
}

/// `detach_path`: detach an item at the given path.
unsafe fn detach_path(
    object: *mut CJson,
    path: *const u8,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    let parent_pointer = cjson_utils_strdup(path);
    if parent_pointer.is_null() {
        return ptr::null_mut();
    }
    let child_pointer = cstr_rchr(parent_pointer, b'/'); /* last '/' */
    if child_pointer.is_null() {
        cjson_free_public(parent_pointer as *mut c_void);
        return ptr::null_mut();
    }
    /* split strings */
    *child_pointer = 0;
    let child_pointer = child_pointer.add(1);

    let parent = get_item_from_pointer(object, parent_pointer, case_sensitive);
    decode_pointer_inplace(child_pointer);

    let mut detached_item: *mut CJson = ptr::null_mut();
    if cjson_is_array(parent) != 0 {
        let mut index = 0usize;
        if decode_array_index_from_pointer(child_pointer, &mut index) != 0 {
            detached_item = detach_item_from_array(parent, index);
        }
    } else if cjson_is_object(parent) != 0 {
        detached_item = cjson_detach_item_from_object(parent, child_pointer as *const c_char);
    }

    cjson_free_public(parent_pointer as *mut c_void);
    detached_item
}

/// `sort_list`: sort lists using mergesort.
unsafe fn sort_list(list: *mut CJson, case_sensitive: CJsonBool) -> *mut CJson {
    let mut first = list;
    let mut second = list;
    let mut current_item = list;
    let mut result = list;
    let mut result_tail: *mut CJson = ptr::null_mut();

    if list.is_null() || (*list).next.is_null() {
        /* One entry is sorted already. */
        return result;
    }

    while !current_item.is_null()
        && !(*current_item).next.is_null()
        && compare_strings(
            (*current_item).string as *const u8,
            (*(*current_item).next).string as *const u8,
            case_sensitive,
        ) < 0
    {
        /* Test for list sorted. */
        current_item = (*current_item).next;
    }
    if current_item.is_null() || (*current_item).next.is_null() {
        /* Leave sorted lists unmodified. */
        return result;
    }

    /* reset pointer to the beginning */
    current_item = list;
    while !current_item.is_null() {
        /* Walk two pointers to find the middle. */
        second = (*second).next;
        current_item = (*current_item).next;
        /* advances current_item two steps at a time */
        if !current_item.is_null() {
            current_item = (*current_item).next;
        }
    }
    if !second.is_null() && !(*second).prev.is_null() {
        /* Split the lists */
        (*(*second).prev).next = ptr::null_mut();
        (*second).prev = ptr::null_mut();
    }

    /* Recursively sort the sub-lists. */
    first = sort_list(first, case_sensitive);
    second = sort_list(second, case_sensitive);
    result = ptr::null_mut();

    /* Merge the sub-lists */
    while !first.is_null() && !second.is_null() {
        let smaller: *mut CJson;
        if compare_strings(
            (*first).string as *const u8,
            (*second).string as *const u8,
            case_sensitive,
        ) < 0
        {
            smaller = first;
        } else {
            smaller = second;
        }

        if result.is_null() {
            /* start merged list with the smaller element */
            result_tail = smaller;
            result = smaller;
        } else {
            /* add smaller element to the list */
            (*result_tail).next = smaller;
            (*smaller).prev = result_tail;
            result_tail = smaller;
        }

        if first == smaller {
            first = (*first).next;
        } else {
            second = (*second).next;
        }
    }

    if !first.is_null() {
        /* Append rest of first list. */
        if result.is_null() {
            return first;
        }
        (*result_tail).next = first;
        (*first).prev = result_tail;
    }
    if !second.is_null() {
        /* Append rest of second list */
        if result.is_null() {
            return second;
        }
        (*result_tail).next = second;
        (*second).prev = result_tail;
    }

    result
}

/// `sort_object`.
unsafe fn sort_object(object: *mut CJson, case_sensitive: CJsonBool) {
    if object.is_null() {
        return;
    }
    (*object).child = sort_list((*object).child, case_sensitive);
}

/// `compare_json`.
unsafe fn compare_json(
    mut a: *mut CJson,
    mut b: *mut CJson,
    case_sensitive: CJsonBool,
) -> CJsonBool {
    if a.is_null() || b.is_null() || ((*a).type_ & 0xFF) != ((*b).type_ & 0xFF) {
        /* mismatched type. */
        return 0;
    }
    match (*a).type_ & 0xFF {
        CJSON_NUMBER => {
            /* numeric mismatch. */
            if ((*a).valueint != (*b).valueint)
                || (compare_double((*a).valuedouble, (*b).valuedouble) == 0)
            {
                return 0;
            }
            1
        }
        CJSON_STRING => {
            /* string mismatch. */
            if cstr_cmp((*a).valuestring as *const u8, (*b).valuestring as *const u8) != 0 {
                return 0;
            }
            1
        }
        CJSON_ARRAY => {
            a = (*a).child;
            b = (*b).child;
            while !a.is_null() && !b.is_null() {
                if compare_json(a, b, case_sensitive) == 0 {
                    return 0;
                }
                a = (*a).next;
                b = (*b).next;
            }

            /* array size mismatch? (one of both children is not NULL) */
            if !a.is_null() || !b.is_null() {
                return 0;
            }
            1
        }
        CJSON_OBJECT => {
            sort_object(a, case_sensitive);
            sort_object(b, case_sensitive);
            a = (*a).child;
            b = (*b).child;
            while !a.is_null() && !b.is_null() {
                /* compare object keys */
                if compare_strings(
                    (*a).string as *const u8,
                    (*b).string as *const u8,
                    case_sensitive,
                ) != 0
                {
                    /* missing member */
                    return 0;
                }
                if compare_json(a, b, case_sensitive) == 0 {
                    return 0;
                }
                a = (*a).next;
                b = (*b).next;
            }

            /* object length mismatch (one of both children is not null) */
            if !a.is_null() || !b.is_null() {
                return 0;
            }
            1
        }
        _ => 1, /* null, true or false */
    }
}

/// `insert_item_in_array`: non broken version of `cJSON_InsertItemInArray`.
unsafe fn insert_item_in_array(
    array: *mut CJson,
    mut which: usize,
    newitem: *mut CJson,
) -> CJsonBool {
    let mut child = (*array).child;
    while !child.is_null() && which > 0 {
        child = (*child).next;
        which -= 1;
    }
    if which > 0 {
        /* item is after the end of the array */
        return 0;
    }
    if child.is_null() {
        cjson_add_item_to_array(array, newitem);
        return 1;
    }

    /* insert into the linked list */
    (*newitem).next = child;
    (*newitem).prev = (*child).prev;
    (*child).prev = newitem;

    /* was it at the beginning */
    if child == (*array).child {
        (*array).child = newitem;
    } else {
        (*(*newitem).prev).next = newitem;
    }

    1
}

/// `get_object_item`.
unsafe fn get_object_item(
    object: *const CJson,
    name: *const c_char,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    if case_sensitive != 0 {
        return cjson_get_object_item_case_sensitive(object, name);
    }
    cjson_get_object_item(object, name)
}

/// JSON patch operations (`enum patch_operation`).
const PATCH_INVALID: c_int = 0;
const PATCH_ADD: c_int = 1;
const PATCH_REMOVE: c_int = 2;
const PATCH_REPLACE: c_int = 3;
const PATCH_MOVE: c_int = 4;
const PATCH_COPY: c_int = 5;
const PATCH_TEST: c_int = 6;

/// `decode_patch_operation`.
unsafe fn decode_patch_operation(patch: *const CJson, case_sensitive: CJsonBool) -> c_int {
    let operation = get_object_item(patch, b"op\0".as_ptr() as *const c_char, case_sensitive);
    if cjson_is_string(operation) == 0 {
        return PATCH_INVALID;
    }

    let valuestring = (*operation).valuestring as *const u8;
    if cstr_cmp(valuestring, b"add\0".as_ptr()) == 0 {
        return PATCH_ADD;
    }
    if cstr_cmp(valuestring, b"remove\0".as_ptr()) == 0 {
        return PATCH_REMOVE;
    }
    if cstr_cmp(valuestring, b"replace\0".as_ptr()) == 0 {
        return PATCH_REPLACE;
    }
    if cstr_cmp(valuestring, b"move\0".as_ptr()) == 0 {
        return PATCH_MOVE;
    }
    if cstr_cmp(valuestring, b"copy\0".as_ptr()) == 0 {
        return PATCH_COPY;
    }
    if cstr_cmp(valuestring, b"test\0".as_ptr()) == 0 {
        return PATCH_TEST;
    }

    PATCH_INVALID
}

/// `overwrite_item`: overwrite an existing item with another one and free
/// resources on the way.
unsafe fn overwrite_item(root: *mut CJson, replacement: CJson) {
    if root.is_null() {
        return;
    }

    if !(*root).string.is_null() {
        cjson_free_public((*root).string as *mut c_void);
    }
    if !(*root).valuestring.is_null() {
        cjson_free_public((*root).valuestring as *mut c_void);
    }
    if !(*root).child.is_null() {
        cjson_delete((*root).child);
    }

    ptr::write(root, replacement);
}

/// `apply_patch`.
unsafe fn apply_patch(object: *mut CJson, patch: *const CJson, case_sensitive: CJsonBool) -> c_int {
    let path: *mut CJson;
    let mut value: *mut CJson = ptr::null_mut();
    let parent: *mut CJson;
    let opcode: c_int;
    let parent_pointer: *mut u8;
    let mut child_pointer: *mut u8 = ptr::null_mut();
    let mut status = 0;

    path = get_object_item(patch, b"path\0".as_ptr() as *const c_char, case_sensitive);
    if cjson_is_string(path) == 0 {
        /* malformed patch. */
        status = 2;
        return status;
    }

    opcode = decode_patch_operation(patch, case_sensitive);
    if opcode == PATCH_INVALID {
        status = 3;
        return status;
    } else if opcode == PATCH_TEST {
        /* compare value: {...} with the given path */
        status = if compare_json(
            get_item_from_pointer(object, (*path).valuestring as *const u8, case_sensitive),
            get_object_item(patch, b"value\0".as_ptr() as *const c_char, case_sensitive),
            case_sensitive,
        ) != 0
        {
            0
        } else {
            1
        };
        return status;
    }

    /* special case for replacing the root */
    if *(*path).valuestring as u8 == 0 {
        if opcode == PATCH_REMOVE {
            let invalid = CJson {
                next: ptr::null_mut(),
                prev: ptr::null_mut(),
                child: ptr::null_mut(),
                type_: CJSON_INVALID,
                valuestring: ptr::null_mut(),
                valueint: 0,
                valuedouble: 0.0,
                string: ptr::null_mut(),
            };

            overwrite_item(object, invalid);
            return 0;
        }

        if opcode == PATCH_REPLACE || opcode == PATCH_ADD {
            value = get_object_item(patch, b"value\0".as_ptr() as *const c_char, case_sensitive);
            if value.is_null() {
                /* missing "value" for add/replace. */
                status = 7;
                return status;
            }

            value = cjson_duplicate(value, 1);
            if value.is_null() {
                /* out of memory for add/replace. */
                status = 8;
                return status;
            }

            overwrite_item(object, ptr::read(value));

            /* delete the duplicated value */
            cjson_free_public(value as *mut c_void);

            /* the string "value" isn't needed */
            if !(*object).string.is_null() {
                cjson_free_public((*object).string as *mut c_void);
                (*object).string = ptr::null_mut();
            }

            return 0;
        }
    }

    if opcode == PATCH_REMOVE || opcode == PATCH_REPLACE {
        /* Get rid of old. */
        let old_item = detach_path(object, (*path).valuestring as *const u8, case_sensitive);
        if old_item.is_null() {
            status = 13;
            return status;
        }
        cjson_delete(old_item);
        if opcode == PATCH_REMOVE {
            /* For Remove, this job is done. */
            return 0;
        }
    }

    /* Copy/Move uses "from". */
    if opcode == PATCH_MOVE || opcode == PATCH_COPY {
        let from = get_object_item(patch, b"from\0".as_ptr() as *const c_char, case_sensitive);
        if cjson_is_string(from) == 0 {
            /* missing "from" for copy/move. */
            status = 4;
            return status;
        }

        if opcode == PATCH_MOVE {
            value = detach_path(object, (*from).valuestring as *const u8, case_sensitive);
        }
        if opcode == PATCH_COPY {
            value = get_item_from_pointer(object, (*from).valuestring as *const u8, case_sensitive);
        }
        if value.is_null() {
            /* missing "from" for copy/move. */
            status = 5;
            return status;
        }
        if opcode == PATCH_COPY {
            value = cjson_duplicate(value, 1);
        }
        if value.is_null() {
            /* out of memory for copy/move. */
            status = 6;
            return status;
        }
    } else {
        /* Add/Replace uses "value". */
        value = get_object_item(patch, b"value\0".as_ptr() as *const c_char, case_sensitive);
        if value.is_null() {
            /* missing "value" for add/replace. */
            status = 7;
            return status;
        }
        value = cjson_duplicate(value, 1);
        if value.is_null() {
            /* out of memory for add/replace. */
            status = 8;
            return status;
        }
    }

    /* Now, just add "value" to "path". */

    /* split pointer in parent and child */
    parent_pointer = cjson_utils_strdup((*path).valuestring as *const u8);
    if !parent_pointer.is_null() {
        child_pointer = cstr_rchr(parent_pointer, b'/');
    }
    if !child_pointer.is_null() {
        *child_pointer = 0;
        child_pointer = child_pointer.add(1);
    }
    parent = get_item_from_pointer(object, parent_pointer, case_sensitive);
    decode_pointer_inplace(child_pointer);

    /* add, remove, replace, move, copy, test. */
    if parent.is_null() || child_pointer.is_null() {
        /* Couldn't find object to add to. */
        status = 9;
    } else if cjson_is_array(parent) != 0 {
        if cstr_cmp(child_pointer, b"-\0".as_ptr()) == 0 {
            cjson_add_item_to_array(parent, value);
            value = ptr::null_mut();
        } else {
            let mut index = 0usize;
            if decode_array_index_from_pointer(child_pointer, &mut index) == 0 {
                status = 11;
            } else if insert_item_in_array(parent, index, value) == 0 {
                status = 10;
            } else {
                value = ptr::null_mut();
            }
        }
    } else if cjson_is_object(parent) != 0 {
        if case_sensitive != 0 {
            cjson_delete_item_from_object_case_sensitive(parent, child_pointer as *const c_char);
        } else {
            cjson_delete_item_from_object(parent, child_pointer as *const c_char);
        }
        cjson_add_item_to_object(parent, child_pointer as *const c_char, value);
        value = ptr::null_mut();
    } else {
        /* parent is not an object */
        /* Couldn't find object to add to. */
        status = 9;
    }

    if !value.is_null() {
        cjson_delete(value);
    }
    if !parent_pointer.is_null() {
        cjson_free_public(parent_pointer as *mut c_void);
    }

    status
}

/// `cJSONUtils_ApplyPatches`.
pub unsafe fn cjson_utils_apply_patches(object: *mut CJson, patches: *const CJson) -> c_int {
    if cjson_is_array(patches) == 0 {
        /* malformed patches. */
        return 1;
    }

    let mut current_patch: *const CJson = if patches.is_null() {
        ptr::null()
    } else {
        (*patches).child
    };

    while !current_patch.is_null() {
        let status = apply_patch(object, current_patch, 0);
        if status != 0 {
            return status;
        }
        current_patch = (*current_patch).next;
    }

    0
}

/// `cJSONUtils_ApplyPatchesCaseSensitive`.
pub unsafe fn cjson_utils_apply_patches_case_sensitive(
    object: *mut CJson,
    patches: *const CJson,
) -> c_int {
    if cjson_is_array(patches) == 0 {
        /* malformed patches. */
        return 1;
    }

    let mut current_patch: *const CJson = if patches.is_null() {
        ptr::null()
    } else {
        (*patches).child
    };

    while !current_patch.is_null() {
        let status = apply_patch(object, current_patch, 1);
        if status != 0 {
            return status;
        }
        current_patch = (*current_patch).next;
    }

    0
}

/// `compose_patch`.
unsafe fn compose_patch(
    patches: *mut CJson,
    operation: *const u8,
    path: *const u8,
    suffix: *const u8,
    value: *const CJson,
) {
    if patches.is_null() || operation.is_null() || path.is_null() {
        return;
    }

    let patch = cjson_create_object();
    if patch.is_null() {
        return;
    }
    cjson_add_item_to_object(
        patch,
        b"op\0".as_ptr() as *const c_char,
        cjson_create_string(operation as *const c_char),
    );

    if suffix.is_null() {
        cjson_add_item_to_object(
            patch,
            b"path\0".as_ptr() as *const c_char,
            cjson_create_string(path as *const c_char),
        );
    } else {
        let suffix_length = pointer_encoded_length(suffix);
        let path_length = cstr_len(path);
        let full_path = cjson_malloc(path_length + suffix_length + 2) as *mut u8;

        let mut i = 0usize;
        while *path.add(i) != 0 {
            *full_path.add(i) = *path.add(i);
            i += 1;
        }
        *full_path.add(i) = b'/';
        encode_string_as_pointer(full_path.add(path_length + 1), suffix);

        cjson_add_item_to_object(
            patch,
            b"path\0".as_ptr() as *const c_char,
            cjson_create_string(full_path as *const c_char),
        );
        cjson_free_public(full_path as *mut c_void);
    }

    if !value.is_null() {
        cjson_add_item_to_object(
            patch,
            b"value\0".as_ptr() as *const c_char,
            cjson_duplicate(value, 1),
        );
    }
    cjson_add_item_to_array(patches, patch);
}

/// `cJSONUtils_AddPatchToArray`.
pub unsafe fn cjson_utils_add_patch_to_array(
    array: *mut CJson,
    operation: *const c_char,
    path: *const c_char,
    value: *const CJson,
) {
    compose_patch(
        array,
        operation as *const u8,
        path as *const u8,
        ptr::null(),
        value,
    );
}

/// Build `"<path>/<index>"` into a fresh `cJSON_malloc`'d buffer (like
/// `sprintf("%s/%lu", path, index)`).
unsafe fn alloc_indexed_path(path: *const u8, index: usize) -> *mut u8 {
    let path_length = cstr_len(path);
    let digits = index.to_string();
    let full_path = cjson_malloc(path_length + 1 + digits.len() + 1) as *mut u8;
    let mut i = 0usize;
    while *path.add(i) != 0 {
        *full_path.add(i) = *path.add(i);
        i += 1;
    }
    *full_path.add(i) = b'/';
    i += 1;
    for &d in digits.as_bytes() {
        *full_path.add(i) = d;
        i += 1;
    }
    *full_path.add(i) = 0;
    full_path
}

/// Build `"%lu"` of `index` into a fresh `cJSON_malloc`'d buffer (like
/// `sprintf("%lu", index)`).
unsafe fn alloc_index_string(index: usize) -> *mut u8 {
    let digits = index.to_string();
    let buf = cjson_malloc(digits.len() + 1) as *mut u8;
    for (i, &d) in digits.as_bytes().iter().enumerate() {
        *buf.add(i) = d;
    }
    *buf.add(digits.len()) = 0;
    buf
}

/// `create_patches`.
unsafe fn create_patches(
    patches: *mut CJson,
    path: *const u8,
    from: *mut CJson,
    to: *mut CJson,
    case_sensitive: CJsonBool,
) {
    if from.is_null() || to.is_null() {
        return;
    }

    if ((*from).type_ & 0xFF) != ((*to).type_ & 0xFF) {
        compose_patch(patches, b"replace\0".as_ptr(), path, ptr::null(), to);
        return;
    }

    match (*from).type_ & 0xFF {
        CJSON_NUMBER => {
            if (*from).valueint != (*to).valueint
                || compare_double((*from).valuedouble, (*to).valuedouble) == 0
            {
                compose_patch(patches, b"replace\0".as_ptr(), path, ptr::null(), to);
            }
        }
        CJSON_STRING => {
            if cstr_cmp(
                (*from).valuestring as *const u8,
                (*to).valuestring as *const u8,
            ) != 0
            {
                compose_patch(patches, b"replace\0".as_ptr(), path, ptr::null(), to);
            }
        }
        CJSON_ARRAY => {
            let mut index = 0usize;
            let mut from_child = (*from).child;
            let mut to_child = (*to).child;

            /* generate patches for all array elements that exist in both "from" and "to" */
            while !from_child.is_null() && !to_child.is_null() {
                let new_path = alloc_indexed_path(path, index); /* path of the current array element */
                create_patches(patches, new_path, from_child, to_child, case_sensitive);
                cjson_free_public(new_path as *mut c_void);
                from_child = (*from_child).next;
                to_child = (*to_child).next;
                index += 1;
            }

            /* remove leftover elements from 'from' that are not in 'to' */
            while !from_child.is_null() {
                let suffix = alloc_index_string(index);
                compose_patch(patches, b"remove\0".as_ptr(), path, suffix, ptr::null());
                cjson_free_public(suffix as *mut c_void);
                from_child = (*from_child).next;
            }

            /* add new elements in 'to' that were not in 'from' */
            while !to_child.is_null() {
                compose_patch(patches, b"add\0".as_ptr(), path, b"-\0".as_ptr(), to_child);
                to_child = (*to_child).next;
            }
        }
        CJSON_OBJECT => {
            let mut from_child: *mut CJson;
            let mut to_child: *mut CJson;
            sort_object(from, case_sensitive);
            sort_object(to, case_sensitive);

            from_child = (*from).child;
            to_child = (*to).child;
            /* for all object values in the object with more of them */
            while !from_child.is_null() || !to_child.is_null() {
                let diff: c_int;
                if from_child.is_null() {
                    diff = 1;
                } else if to_child.is_null() {
                    diff = -1;
                } else {
                    diff = compare_strings(
                        (*from_child).string as *const u8,
                        (*to_child).string as *const u8,
                        case_sensitive,
                    );
                }

                if diff == 0 {
                    /* both object keys are the same */
                    let path_length = cstr_len(path);
                    let from_child_name_length =
                        pointer_encoded_length((*from_child).string as *const u8);
                    let new_path =
                        cjson_malloc(path_length + from_child_name_length + 2) as *mut u8;

                    let mut i = 0usize;
                    while *path.add(i) != 0 {
                        *new_path.add(i) = *path.add(i);
                        i += 1;
                    }
                    *new_path.add(i) = b'/';
                    encode_string_as_pointer(
                        new_path.add(path_length + 1),
                        (*from_child).string as *const u8,
                    );

                    /* create a patch for the element */
                    create_patches(patches, new_path, from_child, to_child, case_sensitive);
                    cjson_free_public(new_path as *mut c_void);

                    from_child = (*from_child).next;
                    to_child = (*to_child).next;
                } else if diff < 0 {
                    /* object element doesn't exist in 'to' --> remove it */
                    compose_patch(
                        patches,
                        b"remove\0".as_ptr(),
                        path,
                        (*from_child).string as *const u8,
                        ptr::null(),
                    );

                    from_child = (*from_child).next;
                } else {
                    /* object element doesn't exist in 'from' --> add it */
                    compose_patch(
                        patches,
                        b"add\0".as_ptr(),
                        path,
                        (*to_child).string as *const u8,
                        to_child,
                    );

                    to_child = (*to_child).next;
                }
            }
        }
        _ => {}
    }
}

/// `cJSONUtils_GeneratePatches`.
pub unsafe fn cjson_utils_generate_patches(from: *mut CJson, to: *mut CJson) -> *mut CJson {
    if from.is_null() || to.is_null() {
        return ptr::null_mut();
    }

    let patches = cjson_create_array();
    create_patches(patches, b"\0".as_ptr(), from, to, 0);

    patches
}

/// `cJSONUtils_GeneratePatchesCaseSensitive`.
pub unsafe fn cjson_utils_generate_patches_case_sensitive(
    from: *mut CJson,
    to: *mut CJson,
) -> *mut CJson {
    if from.is_null() || to.is_null() {
        return ptr::null_mut();
    }

    let patches = cjson_create_array();
    create_patches(patches, b"\0".as_ptr(), from, to, 1);

    patches
}

/// `cJSONUtils_SortObject`.
pub unsafe fn cjson_utils_sort_object(object: *mut CJson) {
    sort_object(object, 0);
}

/// `cJSONUtils_SortObjectCaseSensitive`.
pub unsafe fn cjson_utils_sort_object_case_sensitive(object: *mut CJson) {
    sort_object(object, 1);
}

/// `merge_patch`.
unsafe fn merge_patch(
    mut target: *mut CJson,
    patch: *const CJson,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    let mut patch_child: *mut CJson;

    if cjson_is_object(patch) == 0 {
        /* scalar value, array or NULL, just duplicate */
        cjson_delete(target);
        return cjson_duplicate(patch, 1);
    }

    if cjson_is_object(target) == 0 {
        cjson_delete(target);
        target = cjson_create_object();
    }

    patch_child = (*patch).child;
    while !patch_child.is_null() {
        if cjson_is_null(patch_child) != 0 {
            /* NULL is the indicator to remove a value, see RFC7396 */
            if case_sensitive != 0 {
                cjson_delete_item_from_object_case_sensitive(target, (*patch_child).string);
            } else {
                cjson_delete_item_from_object(target, (*patch_child).string);
            }
        } else {
            let replace_me: *mut CJson;
            let replacement: *mut CJson;

            if case_sensitive != 0 {
                replace_me =
                    cjson_detach_item_from_object_case_sensitive(target, (*patch_child).string);
            } else {
                replace_me = cjson_detach_item_from_object(target, (*patch_child).string);
            }

            replacement = merge_patch(replace_me, patch_child, case_sensitive);
            if replacement.is_null() {
                cjson_delete(target);
                return ptr::null_mut();
            }

            cjson_add_item_to_object(target, (*patch_child).string, replacement);
        }
        patch_child = (*patch_child).next;
    }

    target
}

/// `cJSONUtils_MergePatch`.
pub unsafe fn cjson_utils_merge_patch(target: *mut CJson, patch: *const CJson) -> *mut CJson {
    merge_patch(target, patch, 0)
}

/// `cJSONUtils_MergePatchCaseSensitive`.
pub unsafe fn cjson_utils_merge_patch_case_sensitive(
    target: *mut CJson,
    patch: *const CJson,
) -> *mut CJson {
    merge_patch(target, patch, 1)
}

/// `generate_merge_patch`.
unsafe fn generate_merge_patch(
    from: *mut CJson,
    to: *mut CJson,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    let mut from_child: *mut CJson;
    let mut to_child: *mut CJson;
    let patch: *mut CJson;

    if to.is_null() {
        /* patch to delete everything */
        return cjson_create_null();
    }
    if cjson_is_object(to) == 0 || cjson_is_object(from) == 0 {
        return cjson_duplicate(to, 1);
    }

    sort_object(from, case_sensitive);
    sort_object(to, case_sensitive);

    from_child = (*from).child;
    to_child = (*to).child;
    patch = cjson_create_object();
    if patch.is_null() {
        return ptr::null_mut();
    }

    while !from_child.is_null() || !to_child.is_null() {
        let diff: c_int;
        if !from_child.is_null() {
            if !to_child.is_null() {
                diff = cstr_cmp(
                    (*from_child).string as *const u8,
                    (*to_child).string as *const u8,
                );
            } else {
                diff = -1;
            }
        } else {
            diff = 1;
        }

        if diff < 0 {
            /* from has a value that to doesn't have -> remove */
            cjson_add_item_to_object(patch, (*from_child).string, cjson_create_null());

            from_child = (*from_child).next;
        } else if diff > 0 {
            /* to has a value that from doesn't have -> add to patch */
            cjson_add_item_to_object(patch, (*to_child).string, cjson_duplicate(to_child, 1));

            to_child = (*to_child).next;
        } else {
            /* object key exists in both objects */
            if compare_json(from_child, to_child, case_sensitive) == 0 {
                /* not identical --> generate a patch */
                cjson_add_item_to_object(
                    patch,
                    (*to_child).string,
                    cjson_utils_generate_merge_patch(from_child, to_child),
                );
            }

            /* next key in the object */
            from_child = (*from_child).next;
            to_child = (*to_child).next;
        }
    }

    if (*patch).child.is_null() {
        /* no patch generated */
        cjson_delete(patch);
        return ptr::null_mut();
    }

    patch
}

/// `cJSONUtils_GenerateMergePatch`.
pub unsafe fn cjson_utils_generate_merge_patch(from: *mut CJson, to: *mut CJson) -> *mut CJson {
    generate_merge_patch(from, to, 0)
}

/// `cJSONUtils_GenerateMergePatchCaseSensitive`.
pub unsafe fn cjson_utils_generate_merge_patch_case_sensitive(
    from: *mut CJson,
    to: *mut CJson,
) -> *mut CJson {
    generate_merge_patch(from, to, 1)
}
