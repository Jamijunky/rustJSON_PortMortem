//! Item construction/destruction and tree manipulation, ported from `cJSON.c`.

use core::ffi::{c_char, c_double, c_int, c_void};
use core::ptr;

use crate::alloc::{cjson_alloc, cjson_dealloc, cjson_free, cjson_strdup, cstr_len, current_hooks};
use crate::model::*;
use crate::print::compare_double;

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

/// C `strcpy`: copy a NUL-terminated string (inclusive of the NUL).
unsafe fn cstr_copy(dst: *mut u8, src: *const u8) {
    let mut i = 0usize;
    loop {
        let b = *src.add(i);
        *dst.add(i) = b;
        if b == 0 {
            return;
        }
        i += 1;
    }
}

/// `case_insensitive_strcmp` from `cJSON.c`.
pub unsafe fn case_insensitive_strcmp(mut s1: *const u8, mut s2: *const u8) -> c_int {
    if s1.is_null() || s2.is_null() {
        return 1;
    }
    if s1 == s2 {
        return 0;
    }
    loop {
        let a = (*s1).to_ascii_lowercase() as c_int;
        let b = (*s2).to_ascii_lowercase() as c_int;
        if a != b {
            return a - b;
        }
        if *s1 == 0 {
            return 0;
        }
        s1 = s1.add(1);
        s2 = s2.add(1);
    }
}

/// `cJSON_Version`: a pointer to the static version string "1.7.19".
pub unsafe fn cjson_version() -> *const c_char {
    static VERSION: [u8; 15] = *b"1.7.19\0\0\0\0\0\0\0\0\0";
    VERSION.as_ptr() as *const c_char
}

/// `cJSON_GetStringValue`.
pub unsafe fn cjson_get_string_value(item: *const CJson) -> *mut c_char {
    if cjson_is_string(item) == 0 {
        return ptr::null_mut();
    }
    (*item).valuestring
}

/// `cJSON_GetNumberValue`.
pub unsafe fn cjson_get_number_value(item: *const CJson) -> c_double {
    if cjson_is_number(item) == 0 {
        return f64::NAN;
    }
    (*item).valuedouble
}

/// `cJSON_SetNumberHelper`.
pub unsafe fn cjson_set_number_helper(object: *mut CJson, number: c_double) -> c_double {
    if object.is_null() {
        return f64::NAN;
    }

    if number >= i32::MAX as c_double {
        (*object).valueint = i32::MAX;
    } else if number <= i32::MIN as c_double {
        (*object).valueint = i32::MIN;
    } else {
        (*object).valueint = number as c_int;
    }

    (*object).valuedouble = number;
    number
}

/// `cJSON_SetValuestring`.
pub unsafe fn cjson_set_valuestring(object: *mut CJson, valuestring: *const c_char) -> *mut c_char {
    // if object's type is not cJSON_String or is cJSON_IsReference, it should not set valuestring
    if object.is_null()
        || (*object).type_ & CJSON_STRING == 0
        || (*object).type_ & CJSON_IS_REFERENCE != 0
    {
        return ptr::null_mut();
    }
    // return NULL if the object is corrupted or valuestring is NULL
    if (*object).valuestring.is_null() || valuestring.is_null() {
        return ptr::null_mut();
    }

    let v1_len = cstr_len(valuestring as *const u8);
    let v2_len = cstr_len((*object).valuestring as *const u8);

    if v1_len <= v2_len {
        // strcpy does not handle overlapping strings
        let a_ends_before_b =
            (valuestring as *const u8).add(v1_len) < (*object).valuestring as *const u8;
        let b_ends_before_a =
            ((*object).valuestring as *const u8).add(v2_len) < valuestring as *const u8;
        if !(a_ends_before_b || b_ends_before_a) {
            return ptr::null_mut();
        }
        cstr_copy((*object).valuestring as *mut u8, valuestring as *const u8);
        return (*object).valuestring;
    }
    let hooks = current_hooks();
    let copy = cjson_strdup(&hooks, valuestring as *const u8);
    if copy.is_null() {
        return ptr::null_mut();
    }
    if !(*object).valuestring.is_null() {
        cjson_free(&hooks, (*object).valuestring as *mut c_void);
    }
    (*object).valuestring = copy;

    copy
}

/// `cJSON_New_Item` (zeroed node allocation through hooks).
#[inline]
pub unsafe fn new_item(hooks: &InternalHooks) -> *mut CJson {
    let node = cjson_alloc(hooks, core::mem::size_of::<CJson>()) as *mut CJson;
    if !node.is_null() {
        ptr::write_bytes(node as *mut u8, 0, core::mem::size_of::<CJson>());
    }
    node
}

/// `cJSON_Delete`: recursively free a cJSON tree (or NULL).
pub unsafe fn cjson_delete(item: *mut CJson) {
    let hooks = current_hooks();
    let mut cur = item;
    while !cur.is_null() {
        let next = (*cur).next;
        if !(*cur).is_reference() && !(*cur).child.is_null() {
            cjson_delete((*cur).child);
        }
        if !(*cur).is_reference() && !(*cur).valuestring.is_null() {
            cjson_dealloc(&hooks, (*cur).valuestring as *mut c_void);
            (*cur).valuestring = ptr::null_mut();
        }
        if !(*cur).is_string_is_const() && !(*cur).string.is_null() {
            cjson_dealloc(&hooks, (*cur).string as *mut c_void);
            (*cur).string = ptr::null_mut();
        }
        cjson_dealloc(&hooks, cur as *mut c_void);
        cur = next;
    }
}

/// `cJSON_GetArraySize`.
pub unsafe fn cjson_get_array_size(array: *const CJson) -> c_int {
    if array.is_null() {
        return 0;
    }
    let mut child = (*array).child;
    let mut size: usize = 0;
    while !child.is_null() {
        size += 1;
        child = (*child).next;
    }
    size as c_int
}

/// `get_array_item`.
unsafe fn get_array_item(array: *const CJson, mut index: usize) -> *mut CJson {
    if array.is_null() {
        return ptr::null_mut();
    }
    let mut current_child = (*array).child;
    while !current_child.is_null() && index > 0 {
        index -= 1;
        current_child = (*current_child).next;
    }
    current_child
}

/// Public re-export of `get_array_item` for the FFI layer.
#[doc(hidden)]
pub unsafe fn get_array_item_pub(array: *const CJson, index: usize) -> *mut CJson {
    get_array_item(array, index)
}

/// `cJSON_GetArrayItem`.
pub unsafe fn cjson_get_array_item(array: *const CJson, index: c_int) -> *mut CJson {
    if index < 0 {
        return ptr::null_mut();
    }
    get_array_item(array, index as usize)
}

/// `get_object_item`.
unsafe fn get_object_item(
    object: *const CJson,
    name: *const c_char,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    if object.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let mut current_element = (*object).child;
    if case_sensitive != 0 {
        while !current_element.is_null()
            && !(*current_element).string.is_null()
            && cstr_cmp((*current_element).string as *const u8, name as *const u8) != 0
        {
            current_element = (*current_element).next;
        }
    } else {
        while !current_element.is_null()
            && case_insensitive_strcmp(name as *const u8, (*current_element).string as *const u8)
                != 0
        {
            current_element = (*current_element).next;
        }
    }
    if current_element.is_null() || (*current_element).string.is_null() {
        return ptr::null_mut();
    }
    current_element
}

/// Public re-export of `get_object_item` for the FFI layer.
#[doc(hidden)]
pub unsafe fn get_object_item_pub(
    object: *const CJson,
    name: *const c_char,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    get_object_item(object, name, case_sensitive)
}

/// `cJSON_GetObjectItem`.
pub unsafe fn cjson_get_object_item(object: *const CJson, string: *const c_char) -> *mut CJson {
    get_object_item(object, string, 0)
}

/// `cJSON_GetObjectItemCaseSensitive`.
pub unsafe fn cjson_get_object_item_case_sensitive(
    object: *const CJson,
    string: *const c_char,
) -> *mut CJson {
    get_object_item(object, string, 1)
}

/// `cJSON_HasObjectItem`.
pub unsafe fn cjson_has_object_item(object: *const CJson, string: *const c_char) -> CJsonBool {
    if cjson_get_object_item(object, string).is_null() {
        0
    } else {
        1
    }
}

/// `suffix_object`.
unsafe fn suffix_object(prev: *mut CJson, item: *mut CJson) {
    (*prev).next = item;
    (*item).prev = prev;
}

/// Public re-export of `suffix_object` for the FFI layer.
#[doc(hidden)]
pub unsafe fn suffix_object_pub(prev: *mut CJson, item: *mut CJson) {
    suffix_object(prev, item)
}

/// `create_reference`.
unsafe fn create_reference(item: *const CJson, hooks: &InternalHooks) -> *mut CJson {
    if item.is_null() {
        return ptr::null_mut();
    }
    let reference = new_item(hooks);
    if reference.is_null() {
        return ptr::null_mut();
    }
    ptr::copy_nonoverlapping(item, reference, 1);
    (*reference).string = ptr::null_mut();
    (*reference).type_ |= CJSON_IS_REFERENCE;
    (*reference).next = ptr::null_mut();
    (*reference).prev = ptr::null_mut();
    reference
}

/// Public re-export of `create_reference` for the FFI layer.
#[doc(hidden)]
pub unsafe fn create_reference_pub(item: *const CJson, hooks: *const InternalHooks) -> *mut CJson {
    create_reference(item, &*hooks)
}

/// `add_item_to_array`.
unsafe fn add_item_to_array(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    if item.is_null() || array.is_null() || array == item {
        return 0;
    }
    let child = (*array).child;
    if child.is_null() {
        // list is empty, start new one
        (*array).child = item;
        (*item).prev = item;
        (*item).next = ptr::null_mut();
    } else if !(*child).prev.is_null() {
        // append to the end
        suffix_object((*child).prev, item);
        (*(*array).child).prev = item;
    }
    1
}

/// Public re-export of `add_item_to_array` for the FFI layer.
#[doc(hidden)]
pub unsafe fn add_item_to_array_pub(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    add_item_to_array(array, item)
}

/// `cJSON_AddItemToArray`.
pub unsafe fn cjson_add_item_to_array(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    add_item_to_array(array, item)
}

/// `add_item_to_object`.
unsafe fn add_item_to_object(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
    hooks: &InternalHooks,
    constant_key: CJsonBool,
) -> CJsonBool {
    if object.is_null() || string.is_null() || item.is_null() || object == item {
        return 0;
    }
    let new_key: *mut c_char;
    let new_type: c_int;
    if constant_key != 0 {
        new_key = string as *mut c_char;
        new_type = (*item).type_ | CJSON_STRING_IS_CONST;
    } else {
        new_key = cjson_strdup(hooks, string as *const u8);
        if new_key.is_null() {
            return 0;
        }
        new_type = (*item).type_ & !CJSON_STRING_IS_CONST;
    }

    if (*item).type_ & CJSON_STRING_IS_CONST == 0 && !(*item).string.is_null() {
        cjson_dealloc(hooks, (*item).string as *mut c_void);
    }

    (*item).string = new_key;
    (*item).type_ = new_type;

    add_item_to_array(object, item)
}

/// Public re-export of `add_item_to_object` for the FFI layer.
#[doc(hidden)]
pub unsafe fn add_item_to_object_pub(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
    hooks: *const InternalHooks,
    constant_key: CJsonBool,
) -> CJsonBool {
    add_item_to_object(object, string, item, &*hooks, constant_key)
}

/// `cJSON_AddItemToObject`.
pub unsafe fn cjson_add_item_to_object(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
) -> CJsonBool {
    let hooks = current_hooks();
    add_item_to_object(object, string, item, &hooks, 0)
}

/// `cJSON_AddItemToObjectCS`.
pub unsafe fn cjson_add_item_to_object_cs(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
) -> CJsonBool {
    let hooks = current_hooks();
    add_item_to_object(object, string, item, &hooks, 1)
}

/// `cJSON_AddItemReferenceToArray`.
pub unsafe fn cjson_add_item_reference_to_array(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    if array.is_null() {
        return 0;
    }
    let hooks = current_hooks();
    add_item_to_array(array, create_reference(item, &hooks))
}

/// `cJSON_AddItemReferenceToObject`.
pub unsafe fn cjson_add_item_reference_to_object(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
) -> CJsonBool {
    if object.is_null() || string.is_null() {
        return 0;
    }
    let hooks = current_hooks();
    add_item_to_object(object, string, create_reference(item, &hooks), &hooks, 0)
}

macro_rules! add_to_object {
    ($name:ident, $create:ident) => {
        pub unsafe fn $name(object: *mut CJson, name: *const c_char) -> *mut CJson {
            let item = $create();
            let hooks = current_hooks();
            if add_item_to_object(object, name, item, &hooks, 0) != 0 {
                return item;
            }
            cjson_delete(item);
            ptr::null_mut()
        }
    };
}

add_to_object!(cjson_add_null_to_object, cjson_create_null);
add_to_object!(cjson_add_true_to_object, cjson_create_true);
add_to_object!(cjson_add_false_to_object, cjson_create_false);
add_to_object!(cjson_add_object_to_object, cjson_create_object);
add_to_object!(cjson_add_array_to_object, cjson_create_array);

/// `cJSON_AddBoolToObject`.
pub unsafe fn cjson_add_bool_to_object(
    object: *mut CJson,
    name: *const c_char,
    boolean: CJsonBool,
) -> *mut CJson {
    let bool_item = cjson_create_bool(boolean);
    let hooks = current_hooks();
    if add_item_to_object(object, name, bool_item, &hooks, 0) != 0 {
        return bool_item;
    }
    cjson_delete(bool_item);
    ptr::null_mut()
}

/// `cJSON_AddNumberToObject`.
pub unsafe fn cjson_add_number_to_object(
    object: *mut CJson,
    name: *const c_char,
    number: c_double,
) -> *mut CJson {
    let number_item = cjson_create_number(number);
    let hooks = current_hooks();
    if add_item_to_object(object, name, number_item, &hooks, 0) != 0 {
        return number_item;
    }
    cjson_delete(number_item);
    ptr::null_mut()
}

/// `cJSON_AddStringToObject`.
pub unsafe fn cjson_add_string_to_object(
    object: *mut CJson,
    name: *const c_char,
    string: *const c_char,
) -> *mut CJson {
    let string_item = cjson_create_string(string);
    let hooks = current_hooks();
    if add_item_to_object(object, name, string_item, &hooks, 0) != 0 {
        return string_item;
    }
    cjson_delete(string_item);
    ptr::null_mut()
}

/// `cJSON_AddRawToObject`.
pub unsafe fn cjson_add_raw_to_object(
    object: *mut CJson,
    name: *const c_char,
    raw: *const c_char,
) -> *mut CJson {
    let raw_item = cjson_create_raw(raw);
    let hooks = current_hooks();
    if add_item_to_object(object, name, raw_item, &hooks, 0) != 0 {
        return raw_item;
    }
    cjson_delete(raw_item);
    ptr::null_mut()
}

/// `cJSON_DetachItemViaPointer`.
pub unsafe fn cjson_detach_item_via_pointer(parent: *mut CJson, item: *mut CJson) -> *mut CJson {
    if parent.is_null() || item.is_null() || (item != (*parent).child && (*item).prev.is_null()) {
        return ptr::null_mut();
    }

    if item != (*parent).child {
        // not the first element
        (*(*item).prev).next = (*item).next;
    }
    if !(*item).next.is_null() {
        // not the last element
        (*(*item).next).prev = (*item).prev;
    }

    if item == (*parent).child {
        // first element
        (*parent).child = (*item).next;
    } else if (*item).next.is_null() {
        // last element
        (*(*parent).child).prev = (*item).prev;
    }

    // make sure the detached item doesn't point anywhere anymore
    (*item).prev = ptr::null_mut();
    (*item).next = ptr::null_mut();

    item
}

/// `cJSON_DetachItemFromArray`.
pub unsafe fn cjson_detach_item_from_array(array: *mut CJson, which: c_int) -> *mut CJson {
    if which < 0 {
        return ptr::null_mut();
    }
    cjson_detach_item_via_pointer(array, get_array_item(array, which as usize))
}

/// `cJSON_DeleteItemFromArray`.
pub unsafe fn cjson_delete_item_from_array(array: *mut CJson, which: c_int) {
    let item = cjson_detach_item_from_array(array, which);
    cjson_delete(item);
}

/// `cJSON_DetachItemFromObject`.
pub unsafe fn cjson_detach_item_from_object(
    object: *mut CJson,
    string: *const c_char,
) -> *mut CJson {
    let to_detach = cjson_get_object_item(object, string);
    cjson_detach_item_via_pointer(object, to_detach)
}

/// `cJSON_DetachItemFromObjectCaseSensitive`.
pub unsafe fn cjson_detach_item_from_object_case_sensitive(
    object: *mut CJson,
    string: *const c_char,
) -> *mut CJson {
    let to_detach = cjson_get_object_item_case_sensitive(object, string);
    cjson_detach_item_via_pointer(object, to_detach)
}

/// `cJSON_DeleteItemFromObject`.
pub unsafe fn cjson_delete_item_from_object(object: *mut CJson, string: *const c_char) {
    let item = cjson_detach_item_from_object(object, string);
    cjson_delete(item);
}

/// `cJSON_DeleteItemFromObjectCaseSensitive`.
pub unsafe fn cjson_delete_item_from_object_case_sensitive(
    object: *mut CJson,
    string: *const c_char,
) {
    let item = cjson_detach_item_from_object_case_sensitive(object, string);
    cjson_delete(item);
}

/// `cJSON_InsertItemInArray`.
pub unsafe fn cjson_insert_item_in_array(
    array: *mut CJson,
    which: c_int,
    newitem: *mut CJson,
) -> CJsonBool {
    if which < 0 || newitem.is_null() {
        return 0;
    }

    let after_inserted = get_array_item(array, which as usize);
    if after_inserted.is_null() {
        return add_item_to_array(array, newitem);
    }

    if after_inserted != (*array).child && (*after_inserted).prev.is_null() {
        // return false if after_inserted is a corrupted array item
        return 0;
    }

    (*newitem).next = after_inserted;
    (*newitem).prev = (*after_inserted).prev;
    (*after_inserted).prev = newitem;
    if after_inserted == (*array).child {
        (*array).child = newitem;
    } else {
        (*(*newitem).prev).next = newitem;
    }
    1
}

/// `cJSON_ReplaceItemViaPointer`.
pub unsafe fn cjson_replace_item_via_pointer(
    parent: *mut CJson,
    item: *mut CJson,
    replacement: *mut CJson,
) -> CJsonBool {
    if parent.is_null() || (*parent).child.is_null() || replacement.is_null() || item.is_null() {
        return 0;
    }

    if replacement == item {
        return 1;
    }

    (*replacement).next = (*item).next;
    (*replacement).prev = (*item).prev;

    if !(*replacement).next.is_null() {
        (*(*replacement).next).prev = replacement;
    }
    if (*parent).child == item {
        if (*(*parent).child).prev == (*parent).child {
            (*replacement).prev = replacement;
        }
        (*parent).child = replacement;
    } else {
        if !(*replacement).prev.is_null() {
            (*(*replacement).prev).next = replacement;
        }
        if (*replacement).next.is_null() {
            (*(*parent).child).prev = replacement;
        }
    }

    (*item).next = ptr::null_mut();
    (*item).prev = ptr::null_mut();
    cjson_delete(item);

    1
}

/// `cJSON_ReplaceItemInArray`.
pub unsafe fn cjson_replace_item_in_array(
    array: *mut CJson,
    which: c_int,
    newitem: *mut CJson,
) -> CJsonBool {
    if which < 0 {
        return 0;
    }
    cjson_replace_item_via_pointer(array, get_array_item(array, which as usize), newitem)
}

/// `replace_item_in_object`.
unsafe fn replace_item_in_object(
    object: *mut CJson,
    string: *const c_char,
    replacement: *mut CJson,
    case_sensitive: CJsonBool,
) -> CJsonBool {
    if replacement.is_null() || string.is_null() {
        return 0;
    }

    // replace the name in the replacement
    if (*replacement).type_ & CJSON_STRING_IS_CONST == 0 && !(*replacement).string.is_null() {
        let hooks = current_hooks();
        cjson_free(&hooks, (*replacement).string as *mut c_void);
    }
    let hooks = current_hooks();
    (*replacement).string = cjson_strdup(&hooks, string as *const u8);
    if (*replacement).string.is_null() {
        return 0;
    }

    (*replacement).type_ &= !CJSON_STRING_IS_CONST;

    cjson_replace_item_via_pointer(
        object,
        get_object_item(object, string, case_sensitive),
        replacement,
    )
}

/// `cJSON_ReplaceItemInObject`.
pub unsafe fn cjson_replace_item_in_object(
    object: *mut CJson,
    string: *const c_char,
    newitem: *mut CJson,
) -> CJsonBool {
    replace_item_in_object(object, string, newitem, 0)
}

/// `cJSON_ReplaceItemInObjectCaseSensitive`.
pub unsafe fn cjson_replace_item_in_object_case_sensitive(
    object: *mut CJson,
    string: *const c_char,
    newitem: *mut CJson,
) -> CJsonBool {
    replace_item_in_object(object, string, newitem, 1)
}

// ---- Create basic types ----------------------------------------------------

/// `cJSON_CreateNull`.
pub unsafe fn cjson_create_null() -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_NULL;
    }
    item
}

/// `cJSON_CreateTrue`.
pub unsafe fn cjson_create_true() -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_TRUE;
    }
    item
}

/// `cJSON_CreateFalse`.
pub unsafe fn cjson_create_false() -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_FALSE;
    }
    item
}

/// `cJSON_CreateBool`.
pub unsafe fn cjson_create_bool(boolean: CJsonBool) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = if boolean != 0 {
            CJSON_TRUE
        } else {
            CJSON_FALSE
        };
    }
    item
}

/// `cJSON_CreateNumber`.
pub unsafe fn cjson_create_number(num: c_double) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_NUMBER;
        (*item).valuedouble = num;

        // use saturation in case of overflow
        if num >= i32::MAX as c_double {
            (*item).valueint = i32::MAX;
        } else if num <= i32::MIN as c_double {
            (*item).valueint = i32::MIN;
        } else {
            (*item).valueint = num as c_int;
        }
    }
    item
}

/// `cJSON_CreateString`.
pub unsafe fn cjson_create_string(string: *const c_char) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_STRING;
        (*item).valuestring = cjson_strdup(&hooks, string as *const u8);
        if (*item).valuestring.is_null() {
            cjson_delete(item);
            return ptr::null_mut();
        }
    }
    item
}

/// `cJSON_CreateStringReference`.
pub unsafe fn cjson_create_string_reference(string: *const c_char) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_STRING | CJSON_IS_REFERENCE;
        (*item).valuestring = string as *mut c_char;
    }
    item
}

/// `cJSON_CreateObjectReference`.
pub unsafe fn cjson_create_object_reference(child: *const CJson) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_OBJECT | CJSON_IS_REFERENCE;
        (*item).child = child as *mut CJson;
    }
    item
}

/// `cJSON_CreateArrayReference`.
pub unsafe fn cjson_create_array_reference(child: *const CJson) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_ARRAY | CJSON_IS_REFERENCE;
        (*item).child = child as *mut CJson;
    }
    item
}

/// `cJSON_CreateRaw`.
pub unsafe fn cjson_create_raw(raw: *const c_char) -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_RAW;
        (*item).valuestring = cjson_strdup(&hooks, raw as *const u8);
        if (*item).valuestring.is_null() {
            cjson_delete(item);
            return ptr::null_mut();
        }
    }
    item
}

/// `cJSON_CreateArray`.
pub unsafe fn cjson_create_array() -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_ARRAY;
    }
    item
}

/// `cJSON_CreateObject`.
pub unsafe fn cjson_create_object() -> *mut CJson {
    let hooks = current_hooks();
    let item = new_item(&hooks);
    if !item.is_null() {
        (*item).type_ = CJSON_OBJECT;
    }
    item
}

// ---- Create arrays ---------------------------------------------------------

/// Build a cJSON array out of the `numbers` array (C `cJSON_Create*Array` body).
unsafe fn create_number_array(
    count: c_int,
    element: impl FnMut(usize) -> *mut CJson,
) -> *mut CJson {
    if count < 0 {
        return ptr::null_mut();
    }
    let a = cjson_create_array();
    let mut n: *mut CJson = ptr::null_mut();
    let mut p: *mut CJson = ptr::null_mut();
    let mut i: usize = 0;
    let mut element = element;
    while !a.is_null() && i < count as usize {
        n = element(i);
        if n.is_null() {
            cjson_delete(a);
            return ptr::null_mut();
        }
        if i == 0 {
            (*a).child = n;
        } else {
            suffix_object(p, n);
        }
        p = n;
        i += 1;
    }
    if !a.is_null() && !(*a).child.is_null() {
        (*(*a).child).prev = n;
    }
    a
}

/// `cJSON_CreateIntArray`.
pub unsafe fn cjson_create_int_array(numbers: *const c_int, count: c_int) -> *mut CJson {
    if count < 0 || numbers.is_null() {
        return ptr::null_mut();
    }
    create_number_array(count, |i| cjson_create_number(*numbers.add(i) as c_double))
}

/// `cJSON_CreateFloatArray`.
pub unsafe fn cjson_create_float_array(numbers: *const f32, count: c_int) -> *mut CJson {
    if count < 0 || numbers.is_null() {
        return ptr::null_mut();
    }
    create_number_array(count, |i| cjson_create_number(*numbers.add(i) as c_double))
}

/// `cJSON_CreateDoubleArray`.
pub unsafe fn cjson_create_double_array(numbers: *const c_double, count: c_int) -> *mut CJson {
    if count < 0 || numbers.is_null() {
        return ptr::null_mut();
    }
    create_number_array(count, |i| cjson_create_number(*numbers.add(i)))
}

/// `cJSON_CreateStringArray`.
pub unsafe fn cjson_create_string_array(strings: *const *const c_char, count: c_int) -> *mut CJson {
    if count < 0 || strings.is_null() {
        return ptr::null_mut();
    }
    create_number_array(count, |i| cjson_create_string(*strings.add(i)))
}

// ---- Duplication -----------------------------------------------------------

unsafe fn duplicate_rec(item: *const CJson, depth: usize, recurse: CJsonBool) -> *mut CJson {
    let mut newitem: *mut CJson;
    let mut child: *const CJson;
    let mut next: *mut CJson = ptr::null_mut();
    let mut newchild: *mut CJson = ptr::null_mut();

    // Bail on bad ptr
    if item.is_null() {
        return ptr::null_mut();
    }
    // Create new item
    let hooks = current_hooks();
    newitem = new_item(&hooks);
    if newitem.is_null() {
        return ptr::null_mut();
    }
    // Copy over all vars
    (*newitem).type_ = (*item).type_ & !CJSON_IS_REFERENCE;
    (*newitem).valueint = (*item).valueint;
    (*newitem).valuedouble = (*item).valuedouble;
    if !(*item).valuestring.is_null() {
        (*newitem).valuestring = cjson_strdup(&hooks, (*item).valuestring as *const u8);
        if (*newitem).valuestring.is_null() {
            goto_fail(&mut newitem);
            return ptr::null_mut();
        }
    }
    if !(*item).string.is_null() {
        (*newitem).string = if (*item).type_ & CJSON_STRING_IS_CONST != 0 {
            (*item).string
        } else {
            cjson_strdup(&hooks, (*item).string as *const u8)
        };
        if (*newitem).string.is_null() {
            goto_fail(&mut newitem);
            return ptr::null_mut();
        }
    }
    // If non-recursive, then we're done!
    if recurse == 0 {
        return newitem;
    }
    // Walk the ->next chain for the child.
    child = (*item).child;
    while !child.is_null() {
        if depth >= CJSON_CIRCULAR_LIMIT {
            goto_fail(&mut newitem);
            return ptr::null_mut();
        }
        newchild = duplicate_rec(child, depth + 1, 1); // Duplicate each item in the ->next chain
        if newchild.is_null() {
            goto_fail(&mut newitem);
            return ptr::null_mut();
        }
        if !next.is_null() {
            // If newitem->child already set, then crosswire ->prev and ->next and move on
            (*next).next = newchild;
            (*newchild).prev = next;
            next = newchild;
        } else {
            // Set newitem->child and move to it
            (*newitem).child = newchild;
            next = newchild;
        }
        child = (*child).next;
    }
    if !newitem.is_null() && !(*newitem).child.is_null() {
        (*newitem).child = {
            let c = (*newitem).child;
            (*c).prev = newchild;
            c
        };
    }

    newitem
}

unsafe fn goto_fail(newitem: &mut *mut CJson) {
    if !(*newitem).is_null() {
        cjson_delete(*newitem);
    }
    *newitem = ptr::null_mut();
}

/// `cJSON_Duplicate`.
pub unsafe fn cjson_duplicate(item: *const CJson, recurse: CJsonBool) -> *mut CJson {
    duplicate_rec(item, 0, recurse)
}

// ---- Minify -----------------------------------------------------------------

unsafe fn skip_oneline_comment(input: &mut *mut u8) {
    *input = (*input).add(2);
    while *(*input) != 0 {
        if *(*input) == b'\n' {
            *input = (*input).add(1);
            return;
        }
        *input = (*input).add(1);
    }
}

unsafe fn skip_multiline_comment(input: &mut *mut u8) {
    *input = (*input).add(2);
    while *(*input) != 0 {
        if *(*input) == b'*' && *(*input).add(1) == b'/' {
            *input = (*input).add(2);
            return;
        }
        *input = (*input).add(1);
    }
}

unsafe fn minify_string(input: &mut *mut u8, output: &mut *mut u8) {
    *(*output) = *(*input);
    *input = (*input).add(1);
    *output = (*output).add(1);

    while *(*input) != 0 {
        *(*output) = *(*input);

        if *(*input) == b'"' {
            *input = (*input).add(1);
            *output = (*output).add(1);
            return;
        } else if *(*input) == b'\\' && *(*input).add(1) == b'"' {
            *(*output).add(1) = *(*input).add(1);
            *input = (*input).add(1);
            *output = (*output).add(1);
        }
        *input = (*input).add(1);
        *output = (*output).add(1);
    }
}

/// `cJSON_Minify`.
pub unsafe fn cjson_minify(json: *mut c_char) {
    if json.is_null() {
        return;
    }
    let mut json_ptr = json as *mut u8;
    let mut into = json as *mut u8;

    while *json_ptr != 0 {
        match *json_ptr {
            b' ' | b'\t' | b'\r' | b'\n' => {
                json_ptr = json_ptr.add(1);
            }
            b'/' => {
                if *json_ptr.add(1) == b'/' {
                    skip_oneline_comment(&mut json_ptr);
                } else if *json_ptr.add(1) == b'*' {
                    skip_multiline_comment(&mut json_ptr);
                } else {
                    json_ptr = json_ptr.add(1);
                }
            }
            b'"' => {
                minify_string(&mut json_ptr, &mut into);
            }
            _ => {
                *into = *json_ptr;
                json_ptr = json_ptr.add(1);
                into = into.add(1);
            }
        }
    }

    // and null-terminate
    *into = 0;
}

// ---- Type checks and comparison ---------------------------------------------

/// `cJSON_IsInvalid`.
pub unsafe fn cjson_is_invalid(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_INVALID) as CJsonBool
}

/// `cJSON_IsFalse`.
pub unsafe fn cjson_is_false(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_FALSE) as CJsonBool
}

/// `cJSON_IsTrue`.
pub unsafe fn cjson_is_true(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_TRUE) as CJsonBool
}

/// `cJSON_IsBool`.
pub unsafe fn cjson_is_bool(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & (CJSON_TRUE | CJSON_FALSE) != 0) as CJsonBool
}

/// `cJSON_IsNull`.
pub unsafe fn cjson_is_null(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_NULL) as CJsonBool
}

/// `cJSON_IsNumber`.
pub unsafe fn cjson_is_number(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_NUMBER) as CJsonBool
}

/// `cJSON_IsString`.
pub unsafe fn cjson_is_string(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_STRING) as CJsonBool
}

/// `cJSON_IsArray`.
pub unsafe fn cjson_is_array(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_ARRAY) as CJsonBool
}

/// `cJSON_IsObject`.
pub unsafe fn cjson_is_object(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_OBJECT) as CJsonBool
}

/// `cJSON_IsRaw`.
pub unsafe fn cjson_is_raw(item: *const CJson) -> CJsonBool {
    if item.is_null() {
        return 0;
    }
    ((*item).type_ & 0xFF == CJSON_RAW) as CJsonBool
}

/// `cJSON_Compare`.
pub unsafe fn cjson_compare(
    a: *const CJson,
    b: *const CJson,
    case_sensitive: CJsonBool,
) -> CJsonBool {
    if a.is_null() || b.is_null() || (*a).type_ & 0xFF != (*b).type_ & 0xFF {
        return 0;
    }

    // check if type is valid
    match (*a).type_ & 0xFF {
        CJSON_FALSE | CJSON_TRUE | CJSON_NULL | CJSON_NUMBER | CJSON_STRING | CJSON_RAW
        | CJSON_ARRAY | CJSON_OBJECT => {}
        _ => return 0,
    }

    // identical objects are equal
    if a == b {
        return 1;
    }

    match (*a).type_ & 0xFF {
        CJSON_FALSE | CJSON_TRUE | CJSON_NULL => 1,
        CJSON_NUMBER => {
            if compare_double((*a).valuedouble, (*b).valuedouble) {
                1
            } else {
                0
            }
        }
        CJSON_STRING | CJSON_RAW => {
            if (*a).valuestring.is_null() || (*b).valuestring.is_null() {
                return 0;
            }
            (cstr_cmp((*a).valuestring as *const u8, (*b).valuestring as *const u8) == 0)
                as CJsonBool
        }
        CJSON_ARRAY => {
            let mut a_element = (*a).child;
            let mut b_element = (*b).child;
            loop {
                if a_element.is_null() || b_element.is_null() {
                    break;
                }
                if cjson_compare(a_element, b_element, case_sensitive) == 0 {
                    return 0;
                }
                a_element = (*a_element).next;
                b_element = (*b_element).next;
            }
            // one of the arrays is longer than the other
            if a_element != b_element {
                return 0;
            }
            1
        }
        CJSON_OBJECT => {
            // TODO This has O(n^2) runtime, which is horrible!
            let mut a_element = (*a).child;
            while !a_element.is_null() {
                let b_element = get_object_item(b, (*a_element).string, case_sensitive);
                if b_element.is_null() {
                    return 0;
                }
                if cjson_compare(a_element, b_element, case_sensitive) == 0 {
                    return 0;
                }
                a_element = (*a_element).next;
            }

            // doing this twice, once on a and b to prevent true comparison if a subset of b
            let mut b_element = (*b).child;
            while !b_element.is_null() {
                let a_element = get_object_item(a, (*b_element).string, case_sensitive);
                if a_element.is_null() {
                    return 0;
                }
                if cjson_compare(b_element, a_element, case_sensitive) == 0 {
                    return 0;
                }
                b_element = (*b_element).next;
            }

            1
        }
        _ => 0,
    }
}

/// `cJSON_malloc`.
pub unsafe fn cjson_malloc(size: usize) -> *mut c_void {
    let hooks = current_hooks();
    cjson_alloc(&hooks, size)
}

/// `cJSON_free`.
pub unsafe fn cjson_free_public(ptr: *mut c_void) {
    let hooks = current_hooks();
    cjson_dealloc(&hooks, ptr);
}
