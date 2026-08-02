cmake_minimum_required(VERSION 3.5)

project(cJSON
    VERSION 1.7.19
    LANGUAGES C)

# The cJSON implementation is the Rust port. CJSON_LIB must be the target name
# `cjson` because tests/CMakeLists.txt links `${CJSON_LIB}`.
set(CJSON_LIB cjson)

# Build the Rust port (release; debug info enabled, unwinding kept intact so
# the C harness can detect panics at the ABI boundary) and import it as a
# static lib.
set(CJSON_RUST_SRC "${CMAKE_CURRENT_SOURCE_DIR}/..")
set(CJSON_RUST_LIB "${CJSON_RUST_SRC}/target/release/libcjson.a")

# Re-run `cargo build --release` whenever the Rust sources change.
file(GLOB CJSON_RUST_SOURCES
    "${CJSON_RUST_SRC}/src/*.rs"
    "${CJSON_RUST_SRC}/Cargo.toml")

add_custom_command(
    OUTPUT "${CJSON_RUST_LIB}"
    COMMAND "$ENV{HOME}/.cargo/bin/cargo" build --release
    WORKING_DIRECTORY "${CJSON_RUST_SRC}"
    COMMENT "Building the Rust cJSON port (cargo build --release)"
    DEPENDS ${CJSON_RUST_SOURCES}
    VERBATIM)
add_custom_target(cjson_rust DEPENDS "${CJSON_RUST_LIB}")

add_library("${CJSON_LIB}" STATIC IMPORTED)
set_target_properties("${CJSON_LIB}" PROPERTIES IMPORTED_LOCATION "${CJSON_RUST_LIB}")
add_dependencies("${CJSON_LIB}" cjson_rust)

if(APPLE)
    # Rust std runtime dependencies for linking against the staticlib.
    target_link_libraries("${CJSON_LIB}" INTERFACE
        "-framework Security" "-framework CoreFoundation" m pthread dl)
else()
    target_link_libraries("${CJSON_LIB}" INTERFACE m pthread dl)
endif()

option(ENABLE_CJSON_TEST "Enable building cJSON test" ON)
enable_testing()

# cJSON_Utils is part of the Rust port; when enabled, point the utils tests at
# the same imported staticlib (they link `${CJSON_LIB}` and `${CJSON_UTILS_LIB}`).
option(ENABLE_CJSON_UTILS "Enable building cJSON_Utils tests" OFF)
if(ENABLE_CJSON_UTILS)
    set(CJSON_UTILS_LIB "${CJSON_LIB}")
endif()

# Original top-level smoke test (test.c), unchanged.
set(TEST_CJSON cJSON_test)
add_executable("${TEST_CJSON}" test.c)
target_link_libraries("${TEST_CJSON}" "${CJSON_LIB}")
add_test(NAME ${TEST_CJSON} COMMAND "${CMAKE_CURRENT_BINARY_DIR}/${TEST_CJSON}")

# "check" target that builds everything and runs the tests (referenced by
# tests/CMakeLists.txt via add_dependencies).
add_custom_target(check
    COMMAND ${CMAKE_CTEST_COMMAND} --output-on-failure
    DEPENDS ${TEST_CJSON})

add_subdirectory(tests)
