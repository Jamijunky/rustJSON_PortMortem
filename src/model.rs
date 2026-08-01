//! C ABI type definitions mirroring cJSON's public and internal struct layouts.
//!
//! The struct field order and widths are load-bearing: these types are passed
//! back and forth to C test code through the FFI layer, so they must match the
//! original `cJSON.h` and `cJSON.c` definitions byte for byte.

use core::ffi::{c_char, c_double, c_int, c_void};

/// Type flags, matching `cJSON.h`.
pub const CJSON_INVALID: c_int = 0;
pub const CJSON_FALSE: c_int = 1 << 0;
pub const CJSON_TRUE: c_int = 1 << 1;
pub const CJSON_NULL: c_int = 1 << 2;
pub const CJSON_NUMBER: c_int = 1 << 3;
pub const CJSON_STRING: c_int = 1 << 4;
pub const CJSON_ARRAY: c_int = 1 << 5;
pub const CJSON_OBJECT: c_int = 1 << 6;
pub const CJSON_RAW: c_int = 1 << 7;

pub const CJSON_IS_REFERENCE: c_int = 256;
pub const CJSON_STRING_IS_CONST: c_int = 512;

/// Limits how deeply nested arrays/objects can be before cJSON rejects parsing.
pub const CJSON_NESTING_LIMIT: usize = 1000;
/// Limits the length of circular references before cJSON rejects duplication.
pub const CJSON_CIRCULAR_LIMIT: usize = 10000;

/// cJSON's `int` boolean type.
pub type CJsonBool = c_int;

/// C's `void *(CJSON_CDECL *)(size_t)` hook type.
pub type AllocateFn = unsafe extern "C" fn(usize) -> *mut c_void;
/// C's `void (CJSON_CDECL *)(void *)` hook type.
pub type DeallocateFn = unsafe extern "C" fn(*mut c_void);
/// C's `void *(CJSON_CDECL *)(void *, size_t)` hook type.
pub type ReallocateFn = unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void;

/// Private allocation hooks used inside `cJSON.c` (NOT the public `cJSON_Hooks`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InternalHooks {
    pub allocate: Option<AllocateFn>,
    pub deallocate: Option<DeallocateFn>,
    pub reallocate: Option<ReallocateFn>,
}

/// The public `cJSON_Hooks` struct from `cJSON.h`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CJsonHooks {
    pub malloc_fn: Option<AllocateFn>,
    pub free_fn: Option<DeallocateFn>,
}

/// The public `cJSON` node struct from `cJSON.h`.
#[repr(C)]
pub struct CJson {
    pub next: *mut CJson,
    pub prev: *mut CJson,
    pub child: *mut CJson,
    pub type_: c_int,
    pub valuestring: *mut c_char,
    pub valueint: c_int,
    pub valuedouble: c_double,
    pub string: *mut c_char,
}

/// Internal `parse_buffer` from `cJSON.c`.
#[repr(C)]
pub struct ParseBuffer {
    pub content: *const u8,
    pub length: usize,
    pub offset: usize,
    pub depth: usize,
    pub hooks: InternalHooks,
}

/// Internal `printbuffer` from `cJSON.c`.
#[repr(C)]
pub struct PrintBuffer {
    pub buffer: *mut u8,
    pub length: usize,
    pub offset: usize,
    pub depth: usize,
    pub noalloc: CJsonBool,
    pub format: CJsonBool,
    pub hooks: InternalHooks,
}

/// Internal `error` global (`global_error`) from `cJSON.c`.
#[repr(C)]
pub struct GlobalError {
    pub json: *const u8,
    pub position: usize,
}

/// The version reported by `cJSON_Version()`.
pub const CJSON_VERSION_MAJOR: c_int = 1;
pub const CJSON_VERSION_MINOR: c_int = 7;
pub const CJSON_VERSION_PATCH: c_int = 19;

impl CJson {
    /// Base type flags (low byte), ignoring reference/const modifiers.
    #[inline]
    pub fn base_type(&self) -> c_int {
        self.type_ & 0xFF
    }

    #[inline]
    pub fn is_reference(&self) -> bool {
        self.type_ & CJSON_IS_REFERENCE != 0
    }

    #[inline]
    pub fn is_string_is_const(&self) -> bool {
        self.type_ & CJSON_STRING_IS_CONST != 0
    }
}
