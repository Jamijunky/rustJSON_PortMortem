//! The `extern "C"` surface of the port.
//!
//! Every symbol here has the exact name and signature of its `cJSON.h` /
//! `cJSON.c` counterpart so that the original C test suite (and the C shim)
//! can link against the Rust port unmodified.

use core::ffi::{c_char, c_double, c_int, c_void};

use crate::alloc::cjson_init_hooks;
use crate::manip::*;
use crate::model::*;
use crate::parse::*;
use crate::print::*;

// ---- public API ------------------------------------------------------------

/// `cJSON_InitHooks`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_InitHooks(hooks: *mut CJsonHooks) {
    cjson_init_hooks(hooks)
}

/// `cJSON_Delete`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Delete(item: *mut CJson) {
    cjson_delete(item)
}

/// `cJSON_GetErrorPtr`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetErrorPtr() -> *const c_char {
    get_error_ptr()
}

/// `cJSON_Version`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Version() -> *const c_char {
    cjson_version()
}

/// `cJSON_GetStringValue`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetStringValue(item: *const CJson) -> *mut c_char {
    cjson_get_string_value(item)
}

/// `cJSON_GetNumberValue`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetNumberValue(item: *const CJson) -> c_double {
    cjson_get_number_value(item)
}

/// `cJSON_SetNumberHelper`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_SetNumberHelper(object: *mut CJson, number: c_double) -> c_double {
    cjson_set_number_helper(object, number)
}

/// `cJSON_SetValuestring`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_SetValuestring(
    object: *mut CJson,
    valuestring: *const c_char,
) -> *mut c_char {
    cjson_set_valuestring(object, valuestring)
}

// ---- parsing ----------------------------------------------------------------

/// `cJSON_ParseWithOpts`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ParseWithOpts(
    value: *const c_char,
    return_parse_end: *mut *const c_char,
    require_null_terminated: CJsonBool,
) -> *mut CJson {
    cjson_parse_with_opts(value, return_parse_end, require_null_terminated)
}

/// `cJSON_ParseWithLengthOpts`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ParseWithLengthOpts(
    value: *const c_char,
    buffer_length: usize,
    return_parse_end: *mut *const c_char,
    require_null_terminated: CJsonBool,
) -> *mut CJson {
    cjson_parse_with_length_opts(value, buffer_length, return_parse_end, require_null_terminated)
}

/// `cJSON_Parse`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Parse(value: *const c_char) -> *mut CJson {
    cjson_parse(value)
}

/// `cJSON_ParseWithLength`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ParseWithLength(
    value: *const c_char,
    buffer_length: usize,
) -> *mut CJson {
    cjson_parse_with_length(value, buffer_length)
}

// ---- printing ----------------------------------------------------------------

/// `cJSON_Print`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Print(item: *const CJson) -> *mut c_char {
    cjson_print(item)
}

/// `cJSON_PrintUnformatted`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_PrintUnformatted(item: *const CJson) -> *mut c_char {
    cjson_print_unformatted(item)
}

/// `cJSON_PrintBuffered`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_PrintBuffered(
    item: *const CJson,
    prebuffer: c_int,
    fmt: CJsonBool,
) -> *mut c_char {
    cjson_print_buffered(item, prebuffer, fmt)
}

/// `cJSON_PrintPreallocated`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_PrintPreallocated(
    item: *mut CJson,
    buffer: *mut c_char,
    length: c_int,
    format: CJsonBool,
) -> CJsonBool {
    cjson_print_preallocated(item, buffer, length, format)
}

// ---- accessors ---------------------------------------------------------------

/// `cJSON_GetArraySize`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetArraySize(array: *const CJson) -> c_int {
    cjson_get_array_size(array)
}

/// `cJSON_GetArrayItem`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetArrayItem(array: *const CJson, index: c_int) -> *mut CJson {
    cjson_get_array_item(array, index)
}

/// `cJSON_GetObjectItem`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetObjectItem(
    object: *const CJson,
    string: *const c_char,
) -> *mut CJson {
    cjson_get_object_item(object, string)
}

/// `cJSON_GetObjectItemCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_GetObjectItemCaseSensitive(
    object: *const CJson,
    string: *const c_char,
) -> *mut CJson {
    cjson_get_object_item_case_sensitive(object, string)
}

/// `cJSON_HasObjectItem`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_HasObjectItem(
    object: *const CJson,
    string: *const c_char,
) -> CJsonBool {
    cjson_has_object_item(object, string)
}

// ---- adding items ------------------------------------------------------------

/// `cJSON_AddItemToArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemToArray(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    cjson_add_item_to_array(array, item)
}

/// `cJSON_AddItemToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemToObject(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
) -> CJsonBool {
    cjson_add_item_to_object(object, string, item)
}

/// `cJSON_AddItemToObjectCS`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemToObjectCS(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
) -> CJsonBool {
    cjson_add_item_to_object_cs(object, string, item)
}

/// `cJSON_AddItemReferenceToArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemReferenceToArray(
    array: *mut CJson,
    item: *mut CJson,
) -> CJsonBool {
    cjson_add_item_reference_to_array(array, item)
}

/// `cJSON_AddItemReferenceToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemReferenceToObject(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
) -> CJsonBool {
    cjson_add_item_reference_to_object(object, string, item)
}

/// `cJSON_AddNullToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddNullToObject(
    object: *mut CJson,
    name: *const c_char,
) -> *mut CJson {
    cjson_add_null_to_object(object, name)
}

/// `cJSON_AddTrueToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddTrueToObject(
    object: *mut CJson,
    name: *const c_char,
) -> *mut CJson {
    cjson_add_true_to_object(object, name)
}

/// `cJSON_AddFalseToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddFalseToObject(
    object: *mut CJson,
    name: *const c_char,
) -> *mut CJson {
    cjson_add_false_to_object(object, name)
}

/// `cJSON_AddBoolToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddBoolToObject(
    object: *mut CJson,
    name: *const c_char,
    boolean: CJsonBool,
) -> *mut CJson {
    cjson_add_bool_to_object(object, name, boolean)
}

/// `cJSON_AddNumberToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddNumberToObject(
    object: *mut CJson,
    name: *const c_char,
    number: c_double,
) -> *mut CJson {
    cjson_add_number_to_object(object, name, number)
}

/// `cJSON_AddStringToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddStringToObject(
    object: *mut CJson,
    name: *const c_char,
    string: *const c_char,
) -> *mut CJson {
    cjson_add_string_to_object(object, name, string)
}

/// `cJSON_AddRawToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddRawToObject(
    object: *mut CJson,
    name: *const c_char,
    raw: *const c_char,
) -> *mut CJson {
    cjson_add_raw_to_object(object, name, raw)
}

/// `cJSON_AddObjectToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddObjectToObject(
    object: *mut CJson,
    name: *const c_char,
) -> *mut CJson {
    cjson_add_object_to_object(object, name)
}

/// `cJSON_AddArrayToObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddArrayToObject(
    object: *mut CJson,
    name: *const c_char,
) -> *mut CJson {
    cjson_add_array_to_object(object, name)
}

// ---- detaching / removing -----------------------------------------------------

/// `cJSON_DetachItemViaPointer`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemViaPointer(
    parent: *mut CJson,
    item: *mut CJson,
) -> *mut CJson {
    cjson_detach_item_via_pointer(parent, item)
}

/// `cJSON_DetachItemFromArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemFromArray(
    array: *mut CJson,
    which: c_int,
) -> *mut CJson {
    cjson_detach_item_from_array(array, which)
}

/// `cJSON_DeleteItemFromArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DeleteItemFromArray(array: *mut CJson, which: c_int) {
    cjson_delete_item_from_array(array, which)
}

/// `cJSON_DetachItemFromObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemFromObject(
    object: *mut CJson,
    string: *const c_char,
) -> *mut CJson {
    cjson_detach_item_from_object(object, string)
}

/// `cJSON_DetachItemFromObjectCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemFromObjectCaseSensitive(
    object: *mut CJson,
    string: *const c_char,
) -> *mut CJson {
    cjson_detach_item_from_object_case_sensitive(object, string)
}

/// `cJSON_DeleteItemFromObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DeleteItemFromObject(object: *mut CJson, string: *const c_char) {
    cjson_delete_item_from_object(object, string)
}

/// `cJSON_DeleteItemFromObjectCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_DeleteItemFromObjectCaseSensitive(
    object: *mut CJson,
    string: *const c_char,
) {
    cjson_delete_item_from_object_case_sensitive(object, string)
}

/// `cJSON_InsertItemInArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_InsertItemInArray(
    array: *mut CJson,
    which: c_int,
    newitem: *mut CJson,
) -> CJsonBool {
    cjson_insert_item_in_array(array, which, newitem)
}

/// `cJSON_ReplaceItemViaPointer`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemViaPointer(
    parent: *mut CJson,
    item: *mut CJson,
    replacement: *mut CJson,
) -> CJsonBool {
    cjson_replace_item_via_pointer(parent, item, replacement)
}

/// `cJSON_ReplaceItemInArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemInArray(
    array: *mut CJson,
    which: c_int,
    newitem: *mut CJson,
) -> CJsonBool {
    cjson_replace_item_in_array(array, which, newitem)
}

/// `cJSON_ReplaceItemInObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemInObject(
    object: *mut CJson,
    string: *const c_char,
    newitem: *mut CJson,
) -> CJsonBool {
    cjson_replace_item_in_object(object, string, newitem)
}

/// `cJSON_ReplaceItemInObjectCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemInObjectCaseSensitive(
    object: *mut CJson,
    string: *const c_char,
    newitem: *mut CJson,
) -> CJsonBool {
    cjson_replace_item_in_object_case_sensitive(object, string, newitem)
}

// ---- creation ----------------------------------------------------------------

/// `cJSON_CreateNull`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateNull() -> *mut CJson {
    cjson_create_null()
}

/// `cJSON_CreateTrue`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateTrue() -> *mut CJson {
    cjson_create_true()
}

/// `cJSON_CreateFalse`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateFalse() -> *mut CJson {
    cjson_create_false()
}

/// `cJSON_CreateBool`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateBool(boolean: CJsonBool) -> *mut CJson {
    cjson_create_bool(boolean)
}

/// `cJSON_CreateNumber`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateNumber(num: c_double) -> *mut CJson {
    cjson_create_number(num)
}

/// `cJSON_CreateString`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateString(string: *const c_char) -> *mut CJson {
    cjson_create_string(string)
}

/// `cJSON_CreateStringReference`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateStringReference(string: *const c_char) -> *mut CJson {
    cjson_create_string_reference(string)
}

/// `cJSON_CreateObjectReference`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateObjectReference(child: *const CJson) -> *mut CJson {
    cjson_create_object_reference(child)
}

/// `cJSON_CreateArrayReference`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateArrayReference(child: *const CJson) -> *mut CJson {
    cjson_create_array_reference(child)
}

/// `cJSON_CreateRaw`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateRaw(raw: *const c_char) -> *mut CJson {
    cjson_create_raw(raw)
}

/// `cJSON_CreateArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateArray() -> *mut CJson {
    cjson_create_array()
}

/// `cJSON_CreateObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateObject() -> *mut CJson {
    cjson_create_object()
}

/// `cJSON_CreateIntArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateIntArray(numbers: *const c_int, count: c_int) -> *mut CJson {
    cjson_create_int_array(numbers, count)
}

/// `cJSON_CreateFloatArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateFloatArray(numbers: *const f32, count: c_int) -> *mut CJson {
    cjson_create_float_array(numbers, count)
}

/// `cJSON_CreateDoubleArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateDoubleArray(
    numbers: *const c_double,
    count: c_int,
) -> *mut CJson {
    cjson_create_double_array(numbers, count)
}

/// `cJSON_CreateStringArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateStringArray(
    strings: *const *const c_char,
    count: c_int,
) -> *mut CJson {
    cjson_create_string_array(strings, count)
}

/// `cJSON_Duplicate`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Duplicate(item: *const CJson, recurse: CJsonBool) -> *mut CJson {
    cjson_duplicate(item, recurse)
}

/// `cJSON_Minify`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Minify(json: *mut c_char) {
    cjson_minify(json)
}

// ---- type predicates -----------------------------------------------------------

/// `cJSON_IsInvalid`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsInvalid(item: *const CJson) -> CJsonBool {
    cjson_is_invalid(item)
}

/// `cJSON_IsFalse`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsFalse(item: *const CJson) -> CJsonBool {
    cjson_is_false(item)
}

/// `cJSON_IsTrue`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsTrue(item: *const CJson) -> CJsonBool {
    cjson_is_true(item)
}

/// `cJSON_IsBool`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsBool(item: *const CJson) -> CJsonBool {
    cjson_is_bool(item)
}

/// `cJSON_IsNull`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsNull(item: *const CJson) -> CJsonBool {
    cjson_is_null(item)
}

/// `cJSON_IsNumber`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsNumber(item: *const CJson) -> CJsonBool {
    cjson_is_number(item)
}

/// `cJSON_IsString`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsString(item: *const CJson) -> CJsonBool {
    cjson_is_string(item)
}

/// `cJSON_IsArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsArray(item: *const CJson) -> CJsonBool {
    cjson_is_array(item)
}

/// `cJSON_IsObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsObject(item: *const CJson) -> CJsonBool {
    cjson_is_object(item)
}

/// `cJSON_IsRaw`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsRaw(item: *const CJson) -> CJsonBool {
    cjson_is_raw(item)
}

/// `cJSON_Compare`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_Compare(
    a: *const CJson,
    b: *const CJson,
    case_sensitive: CJsonBool,
) -> CJsonBool {
    cjson_compare(a, b, case_sensitive)
}

/// `cJSON_malloc`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_malloc(size: usize) -> *mut c_void {
    cjson_malloc(size)
}

/// `cJSON_free`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_free(ptr: *mut c_void) {
    cjson_free_public(ptr)
}

// ---- internals (used by the C shim and the original test suite) ----------------

/// `parse_hex4`.
#[no_mangle]
pub unsafe extern "C" fn parse_hex4(input: *const u8) -> u32 {
    parse_hex4_impl(input)
}

/// `parse_string`.
#[no_mangle]
pub unsafe extern "C" fn parse_string(
    item: *mut CJson,
    input_buffer: *mut ParseBuffer,
) -> CJsonBool {
    parse_string_impl(item, input_buffer)
}

/// `parse_number`.
#[no_mangle]
pub unsafe extern "C" fn parse_number(
    item: *mut CJson,
    input_buffer: *mut ParseBuffer,
) -> CJsonBool {
    parse_number_impl(item, input_buffer)
}

/// `parse_array`.
#[no_mangle]
pub unsafe extern "C" fn parse_array(
    item: *mut CJson,
    input_buffer: *mut ParseBuffer,
) -> CJsonBool {
    parse_array_impl(item, input_buffer)
}

/// `parse_object`.
#[no_mangle]
pub unsafe extern "C" fn parse_object(
    item: *mut CJson,
    input_buffer: *mut ParseBuffer,
) -> CJsonBool {
    parse_object_impl(item, input_buffer)
}

/// `parse_value`.
#[no_mangle]
pub unsafe extern "C" fn parse_value(
    item: *mut CJson,
    input_buffer: *mut ParseBuffer,
) -> CJsonBool {
    parse_value_impl(item, input_buffer)
}

/// `skip_utf8_bom`.
#[no_mangle]
pub unsafe extern "C" fn skip_utf8_bom(buffer: *mut ParseBuffer) -> *mut ParseBuffer {
    skip_utf8_bom_impl(buffer)
}

/// `ensure`.
#[no_mangle]
pub unsafe extern "C" fn ensure(p: *mut PrintBuffer, needed: usize) -> *mut u8 {
    ensure_impl(p, needed)
}

/// `print_string`.
#[no_mangle]
pub unsafe extern "C" fn print_string(
    item: *const CJson,
    output_buffer: *mut PrintBuffer,
) -> CJsonBool {
    print_string_impl(item, output_buffer)
}

/// `print_array`.
#[no_mangle]
pub unsafe extern "C" fn print_array(
    item: *const CJson,
    output_buffer: *mut PrintBuffer,
) -> CJsonBool {
    print_array_impl(item, output_buffer)
}

/// `print_object`.
#[no_mangle]
pub unsafe extern "C" fn print_object(
    item: *const CJson,
    output_buffer: *mut PrintBuffer,
) -> CJsonBool {
    print_object_impl(item, output_buffer)
}

/// `print_number`.
#[no_mangle]
pub unsafe extern "C" fn print_number(
    item: *const CJson,
    output_buffer: *mut PrintBuffer,
) -> CJsonBool {
    print_number_impl(item, output_buffer)
}

/// `print_value`.
#[no_mangle]
pub unsafe extern "C" fn print_value(
    item: *const CJson,
    output_buffer: *mut PrintBuffer,
) -> CJsonBool {
    print_value_impl(item, output_buffer)
}

/// `print_string_ptr`.
#[no_mangle]
pub unsafe extern "C" fn print_string_ptr(
    input: *const u8,
    output_buffer: *mut PrintBuffer,
) -> CJsonBool {
    print_string_ptr_impl(input, output_buffer)
}

/// `compare_double`.
#[no_mangle]
pub unsafe extern "C" fn compare_double(a: c_double, b: c_double) -> CJsonBool {
    compare_double_impl(a, b)
}

/// `get_decimal_point`.
#[no_mangle]
pub unsafe extern "C" fn get_decimal_point() -> u8 {
    get_decimal_point_impl()
}

/// `case_insensitive_strcmp`.
#[no_mangle]
pub unsafe extern "C" fn case_insensitive_strcmp(
    string1: *const u8,
    string2: *const u8,
) -> c_int {
    case_insensitive_strcmp_impl(string1, string2)
}

/// `cast_away_const`.
#[no_mangle]
pub unsafe extern "C" fn cast_away_const(string: *const c_void) -> *mut c_void {
    cast_away_const_impl(string)
}

/// `get_array_item`.
#[no_mangle]
pub unsafe extern "C" fn get_array_item(array: *const CJson, index: usize) -> *mut CJson {
    get_array_item_impl(array, index)
}

/// `get_object_item`.
#[no_mangle]
pub unsafe extern "C" fn get_object_item(
    object: *const CJson,
    name: *const c_char,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    get_object_item_impl(object, name, case_sensitive)
}

/// `suffix_object`.
#[no_mangle]
pub unsafe extern "C" fn suffix_object(prev: *mut CJson, item: *mut CJson) {
    suffix_object_impl(prev, item)
}

/// `create_reference`.
#[no_mangle]
pub unsafe extern "C" fn create_reference(
    item: *const CJson,
    hooks: *const InternalHooks,
) -> *mut CJson {
    create_reference_impl(item, hooks)
}

/// `add_item_to_array`.
#[no_mangle]
pub unsafe extern "C" fn add_item_to_array(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    add_item_to_array_impl(array, item)
}

/// `add_item_to_object`.
#[no_mangle]
pub unsafe extern "C" fn add_item_to_object(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
    hooks: *const InternalHooks,
    constant_key: CJsonBool,
) -> CJsonBool {
    add_item_to_object_impl(object, string, item, hooks, constant_key)
}

/// `cJSON_strdup`.
#[no_mangle]
pub unsafe extern "C" fn cJSON_strdup(string: *const u8, hooks: *const InternalHooks) -> *mut u8 {
    cjson_strdup_impl(string, hooks)
}

// ---- internal impl re-exports (thin wrappers around the port) -----------------

unsafe fn parse_hex4_impl(input: *const u8) -> u32 {
    crate::parse::parse_hex4(input)
}
unsafe fn parse_string_impl(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    crate::parse::parse_string(item, input_buffer)
}
unsafe fn parse_number_impl(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    crate::parse::parse_number(item, input_buffer)
}
unsafe fn parse_array_impl(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    crate::parse::parse_array(item, input_buffer)
}
unsafe fn parse_object_impl(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    crate::parse::parse_object(item, input_buffer)
}
unsafe fn parse_value_impl(item: *mut CJson, input_buffer: *mut ParseBuffer) -> CJsonBool {
    crate::parse::parse_value(item, input_buffer)
}
unsafe fn skip_utf8_bom_impl(buffer: *mut ParseBuffer) -> *mut ParseBuffer {
    crate::parse::skip_utf8_bom(buffer)
}
unsafe fn ensure_impl(p: *mut PrintBuffer, needed: usize) -> *mut u8 {
    crate::print::ensure(p, needed)
}
unsafe fn print_string_impl(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    crate::print::print_string(item, output_buffer)
}
unsafe fn print_array_impl(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    crate::print::print_array(item, output_buffer)
}
unsafe fn print_object_impl(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    crate::print::print_object(item, output_buffer)
}
unsafe fn print_number_impl(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    crate::print::print_number(item, output_buffer)
}
unsafe fn print_value_impl(item: *const CJson, output_buffer: *mut PrintBuffer) -> CJsonBool {
    crate::print::print_value(item, output_buffer)
}
unsafe fn print_string_ptr_impl(input: *const u8, output_buffer: *mut PrintBuffer) -> CJsonBool {
    crate::print::print_string_ptr(input, output_buffer)
}
unsafe fn compare_double_impl(a: c_double, b: c_double) -> CJsonBool {
    crate::print::compare_double(a, b) as CJsonBool
}
unsafe fn get_decimal_point_impl() -> u8 {
    crate::print::get_decimal_point()
}
unsafe fn case_insensitive_strcmp_impl(string1: *const u8, string2: *const u8) -> c_int {
    crate::manip::case_insensitive_strcmp(string1, string2)
}
unsafe fn cast_away_const_impl(string: *const c_void) -> *mut c_void {
    string as *mut c_void
}
unsafe fn get_array_item_impl(array: *const CJson, index: usize) -> *mut CJson {
    crate::manip::get_array_item_pub(array, index)
}
unsafe fn get_object_item_impl(
    object: *const CJson,
    name: *const c_char,
    case_sensitive: CJsonBool,
) -> *mut CJson {
    crate::manip::get_object_item_pub(object, name, case_sensitive)
}
unsafe fn suffix_object_impl(prev: *mut CJson, item: *mut CJson) {
    crate::manip::suffix_object_pub(prev, item)
}
unsafe fn create_reference_impl(item: *const CJson, hooks: *const InternalHooks) -> *mut CJson {
    crate::manip::create_reference_pub(item, hooks)
}
unsafe fn add_item_to_array_impl(array: *mut CJson, item: *mut CJson) -> CJsonBool {
    crate::manip::add_item_to_array_pub(array, item)
}
unsafe fn add_item_to_object_impl(
    object: *mut CJson,
    string: *const c_char,
    item: *mut CJson,
    hooks: *const InternalHooks,
    constant_key: CJsonBool,
) -> CJsonBool {
    crate::manip::add_item_to_object_pub(object, string, item, hooks, constant_key)
}
unsafe fn cjson_strdup_impl(string: *const u8, hooks: *const InternalHooks) -> *mut u8 {
    crate::alloc::cjson_strdup(&*hooks, string) as *mut u8
}

// ---- cJSON_Utils API ---------------------------------------------------------

/// `cJSONUtils_GetPointer`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_GetPointer(object: *mut CJson, pointer: *const c_char) -> *mut CJson {
    crate::utils::cjson_utils_get_pointer(object, pointer as *const u8)
}

/// `cJSONUtils_GetPointerCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_GetPointerCaseSensitive(object: *mut CJson, pointer: *const c_char) -> *mut CJson {
    crate::utils::cjson_utils_get_pointer_case_sensitive(object, pointer as *const u8)
}

/// `cJSONUtils_GeneratePatches`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_GeneratePatches(from: *mut CJson, to: *mut CJson) -> *mut CJson {
    crate::utils::cjson_utils_generate_patches(from, to)
}

/// `cJSONUtils_GeneratePatchesCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_GeneratePatchesCaseSensitive(from: *mut CJson, to: *mut CJson) -> *mut CJson {
    crate::utils::cjson_utils_generate_patches_case_sensitive(from, to)
}

/// `cJSONUtils_AddPatchToArray`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_AddPatchToArray(
    array: *mut CJson,
    operation: *const c_char,
    path: *const c_char,
    value: *const CJson,
) {
    crate::utils::cjson_utils_add_patch_to_array(array, operation, path, value)
}

/// `cJSONUtils_ApplyPatches`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_ApplyPatches(object: *mut CJson, patches: *const CJson) -> c_int {
    crate::utils::cjson_utils_apply_patches(object, patches)
}

/// `cJSONUtils_ApplyPatchesCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_ApplyPatchesCaseSensitive(object: *mut CJson, patches: *const CJson) -> c_int {
    crate::utils::cjson_utils_apply_patches_case_sensitive(object, patches)
}

/// `cJSONUtils_MergePatch`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_MergePatch(target: *mut CJson, patch: *const CJson) -> *mut CJson {
    crate::utils::cjson_utils_merge_patch(target, patch)
}

/// `cJSONUtils_MergePatchCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_MergePatchCaseSensitive(target: *mut CJson, patch: *const CJson) -> *mut CJson {
    crate::utils::cjson_utils_merge_patch_case_sensitive(target, patch)
}

/// `cJSONUtils_GenerateMergePatch`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_GenerateMergePatch(from: *mut CJson, to: *mut CJson) -> *mut CJson {
    crate::utils::cjson_utils_generate_merge_patch(from, to)
}

/// `cJSONUtils_GenerateMergePatchCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_GenerateMergePatchCaseSensitive(from: *mut CJson, to: *mut CJson) -> *mut CJson {
    crate::utils::cjson_utils_generate_merge_patch_case_sensitive(from, to)
}

/// `cJSONUtils_FindPointerFromObjectTo`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_FindPointerFromObjectTo(object: *const CJson, target: *const CJson) -> *mut c_char {
    crate::utils::cjson_utils_find_pointer_from_object_to(object, target)
}

/// `cJSONUtils_SortObject`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_SortObject(object: *mut CJson) {
    crate::utils::cjson_utils_sort_object(object)
}

/// `cJSONUtils_SortObjectCaseSensitive`.
#[no_mangle]
pub unsafe extern "C" fn cJSONUtils_SortObjectCaseSensitive(object: *mut CJson) {
    crate::utils::cjson_utils_sort_object_case_sensitive(object)
}
