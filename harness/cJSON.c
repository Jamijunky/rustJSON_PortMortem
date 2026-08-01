/*
  cJSON - Rust port C shim

  This file is a drop-in replacement for the reference cJSON.c. The original
  test suite includes it directly (`tests/common.h` does `#include
  "../cJSON.c"`), so it must provide every type and declaration the tests rely
  on. All implementation lives in the Rust port, whose `#[no_mangle]` symbols
  this translation unit links against; there are no definitions here, only the
  struct layouts and the extern declarations.
*/

#include "cJSON.h"

#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <math.h>
#include <stdlib.h>
#include <limits.h>
#include <ctype.h>
#include <float.h>

#ifdef ENABLE_LOCALES
#include <locale.h>
#endif

/* cJSON_bool is int; provide the same true/false shorthands as cJSON.c. */
#ifdef true
#undef true
#endif
#define true ((cJSON_bool)1)

#ifdef false
#undef false
#endif
#define false ((cJSON_bool)0)

/* Private allocation hooks (cJSON.c). */
typedef struct internal_hooks
{
    void *(CJSON_CDECL *allocate)(size_t size);
    void (CJSON_CDECL *deallocate)(void *pointer);
    void *(CJSON_CDECL *reallocate)(void *pointer, size_t size);
} internal_hooks;

/* Internal parse state (cJSON.c). */
typedef struct parse_buffer
{
    const unsigned char *content;
    size_t length;
    size_t offset;
    size_t depth;
    internal_hooks hooks;
} parse_buffer;

/* Internal print state (cJSON.c). */
typedef struct printbuffer
{
    char *buffer;
    size_t length;
    size_t offset;
    size_t depth;
    cJSON_bool noalloc;
    cJSON_bool format;
    internal_hooks hooks;
} printbuffer;

/* Single global hook set, owned by the Rust port. */
extern internal_hooks global_hooks;

/* ---- internals (implemented by the Rust port) ---- */

extern unsigned parse_hex4(const unsigned char * const input);
extern cJSON_bool parse_number(cJSON * const item, parse_buffer * const input_buffer);
extern cJSON_bool parse_string(cJSON * const item, parse_buffer * const input_buffer);
extern cJSON_bool parse_array(cJSON * const item, parse_buffer * const input_buffer);
extern cJSON_bool parse_object(cJSON * const item, parse_buffer * const input_buffer);
extern cJSON_bool parse_value(cJSON * const item, parse_buffer * const input_buffer);
extern parse_buffer *skip_utf8_bom(parse_buffer * const buffer);
extern unsigned char *ensure(printbuffer * const p, size_t needed);
extern cJSON_bool print_string(const cJSON * const item, printbuffer * const output_buffer);
extern cJSON_bool print_array(const cJSON * const item, printbuffer * const output_buffer);
extern cJSON_bool print_object(const cJSON * const item, printbuffer * const output_buffer);
extern cJSON_bool print_number(const cJSON * const item, printbuffer * const output_buffer);
extern cJSON_bool print_value(const cJSON * const item, printbuffer * const output_buffer);
extern cJSON_bool print_string_ptr(const unsigned char * const input, printbuffer * const output_buffer);
extern cJSON_bool compare_double(double a, double b);
extern unsigned char get_decimal_point(void);
extern int case_insensitive_strcmp(const unsigned char *string1, const unsigned char *string2);
extern void *cast_away_const(const void *string);
extern cJSON *get_array_item(const cJSON *array, size_t index);
extern cJSON *get_object_item(const cJSON * const object, const char * const name, const cJSON_bool case_sensitive);
extern void suffix_object(cJSON *prev, cJSON *item);
extern cJSON *create_reference(const cJSON *item, const internal_hooks * const hooks);
extern cJSON_bool add_item_to_array(cJSON *array, cJSON *item);
extern cJSON_bool add_item_to_object(cJSON * const object, const char * const string, cJSON * const item, const internal_hooks * const hooks, const cJSON_bool constant_key);
extern unsigned char *cJSON_strdup(const unsigned char *string, const internal_hooks * const hooks);
