//! Hook-based memory allocation.
//!
//! Every byte of memory that can cross the FFI boundary must be allocated
//! through the *current* `global_hooks`, so that C code (including the
//! original test suite) can free it with the same hooks. `global_hooks` is a
//! real C-visible symbol: the C shim declares `extern internal_hooks
//! global_hooks;` and the original test files read it directly.

use core::ffi::{c_char, c_void};

use crate::model::{AllocateFn, CJsonHooks, DeallocateFn, InternalHooks};

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
}

/// The single source of truth for allocation hooks, exported to C under the
/// symbol name `global_hooks` (matching `cJSON.c`).
#[no_mangle]
pub static mut global_hooks: InternalHooks = InternalHooks {
    allocate: Some(malloc),
    deallocate: Some(free),
    reallocate: Some(realloc),
};

/// Read the current hooks (a copy, safe to hand to functions).
#[inline]
pub unsafe fn current_hooks() -> InternalHooks {
    (&raw const global_hooks).read()
}

/// Mutable access to the current hooks.
#[inline]
pub unsafe fn hooks_mut() -> &'static mut InternalHooks {
    &mut *(&raw mut global_hooks)
}

/// True when the given allocate hook is the default `malloc`.
#[inline]
pub fn is_default_allocator(allocate: Option<AllocateFn>) -> bool {
    matches!(allocate, Some(f) if core::ptr::fn_addr_eq(f, malloc as AllocateFn))
}

/// True when the given deallocate hook is the default `free`.
#[inline]
pub fn is_default_deallocator(deallocate: Option<DeallocateFn>) -> bool {
    matches!(deallocate, Some(f) if core::ptr::fn_addr_eq(f, free as DeallocateFn))
}

/// `cJSON_InitHooks`: install (or reset, when `hooks` is null) the global hooks.
pub unsafe fn cjson_init_hooks(hooks: *mut CJsonHooks) {
    let h = hooks_mut();
    if hooks.is_null() {
        *h = InternalHooks {
            allocate: Some(malloc),
            deallocate: Some(free),
            reallocate: Some(realloc),
        };
        return;
    }
    let given = &*hooks;
    h.allocate = Some(malloc);
    if let Some(malloc_fn) = given.malloc_fn {
        h.allocate = Some(malloc_fn);
    }
    h.deallocate = Some(free);
    if let Some(free_fn) = given.free_fn {
        h.deallocate = Some(free_fn);
    }
    h.reallocate = None;
    if is_default_allocator(h.allocate) && is_default_deallocator(h.deallocate) {
        h.reallocate = Some(realloc);
    }
}

/// Allocate `size` bytes through `hooks`, returning a null pointer on failure.
#[inline]
pub unsafe fn cjson_alloc(hooks: &InternalHooks, size: usize) -> *mut c_void {
    match hooks.allocate {
        Some(allocate) => allocate(size),
        None => core::ptr::null_mut(),
    }
}

/// Free `ptr` through `hooks` (no-op for null).
#[inline]
pub unsafe fn cjson_dealloc(hooks: &InternalHooks, ptr: *mut c_void) {
    if !ptr.is_null() {
        if let Some(deallocate) = hooks.deallocate {
            deallocate(ptr);
        }
    }
}

/// Reallocate `ptr` to `size` bytes through `hooks`. C semantics: if no
/// `reallocate` hook is installed the caller must fall back to manual
/// alloc/copy/dealloc; that fallback is implemented by callers that need it.
#[inline]
pub unsafe fn cjson_realloc(hooks: &InternalHooks, ptr: *mut c_void, size: usize) -> *mut c_void {
    match hooks.reallocate {
        Some(reallocate) => reallocate(ptr, size),
        None => core::ptr::null_mut(),
    }
}

/// Allocate and zero a `cJSON` node (cJSON_New_Item).
#[inline]
pub unsafe fn cjson_new_item(hooks: &InternalHooks) -> *mut crate::model::CJson {
    let node = cjson_alloc(hooks, core::mem::size_of::<crate::model::CJson>()) as *mut crate::model::CJson;
    if !node.is_null() {
        core::ptr::write_bytes(node as *mut u8, 0, core::mem::size_of::<crate::model::CJson>());
    }
    node
}

/// Duplicate a nul-terminated byte string through `hooks` (cJSON_strdup).
pub unsafe fn cjson_strdup(hooks: &InternalHooks, string: *const u8) -> *mut c_char {
    if string.is_null() {
        return core::ptr::null_mut();
    }
    let length = cstr_len(string);
    let copy = cjson_alloc(hooks, length + 1) as *mut u8;
    if copy.is_null() {
        return core::ptr::null_mut();
    }
    core::ptr::copy_nonoverlapping(string, copy, length + 1);
    copy as *mut c_char
}

/// Length of a NUL-terminated byte string in bytes.
pub unsafe fn cstr_len(mut ptr: *const u8) -> usize {
    let mut len = 0usize;
    while *ptr != 0 {
        ptr = ptr.add(1);
        len += 1;
    }
    len
}

/// Free a string (or node) allocated through `hooks` via `cJSON_free`.
#[inline]
pub unsafe fn cjson_free(hooks: &InternalHooks, ptr: *mut c_void) {
    cjson_dealloc(hooks, ptr);
}
