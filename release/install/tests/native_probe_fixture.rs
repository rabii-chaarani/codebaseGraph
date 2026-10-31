#![no_std]
#![no_main]

use core::{ffi::c_void, panic::PanicInfo};

const EXIT_UNEXPECTED_ARGUMENTS: u32 = 64;
const EXIT_FIXTURE_ERROR: u32 = 65;
const TRACE_VERSION: u32 = 1;
const ROLE_GRAPH: u32 = 1;
const ROLE_WIKI: u32 = 2;

const FILE_APPEND_DATA: u32 = 0x0004;
const FILE_SHARE_READ: u32 = 0x0001;
const FILE_SHARE_WRITE: u32 = 0x0002;
const OPEN_ALWAYS: u32 = 4;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x0080;

#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn GetCommandLineW() -> *const u16;
    fn GetEnvironmentStringsW() -> *mut u16;
    fn FreeEnvironmentStringsW(environment: *mut u16) -> i32;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
    fn CreateFileW(
        path: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *const c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: *mut c_void,
    ) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        buffer: *const c_void,
        bytes_to_write: u32,
        bytes_written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn FlushFileBuffers(file: *mut c_void) -> i32;
    fn CloseHandle(object: *mut c_void) -> i32;
    fn ExitProcess(exit_code: u32) -> !;
}

#[link(name = "shell32", kind = "raw-dylib")]
unsafe extern "system" {
    fn CommandLineToArgvW(command_line: *const u16, argc: *mut i32) -> *mut *mut u16;
}

struct EnvironmentConfig {
    graph_exit: u32,
    wiki_exit: u32,
    trace_path: *const u16,
}

#[no_mangle]
pub extern "system" fn mainCRTStartup() -> ! {
    let exit_code = unsafe { run() };
    unsafe { ExitProcess(exit_code) }
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { ExitProcess(EXIT_FIXTURE_ERROR) }
}

unsafe fn run() -> u32 {
    let environment = GetEnvironmentStringsW();
    if environment.is_null() {
        return EXIT_FIXTURE_ERROR;
    }

    let config = match parse_environment(environment) {
        Some(config) => config,
        None => {
            let _ = FreeEnvironmentStringsW(environment);
            return EXIT_FIXTURE_ERROR;
        }
    };

    let command_line = GetCommandLineW();
    if command_line.is_null() {
        let _ = FreeEnvironmentStringsW(environment);
        return EXIT_FIXTURE_ERROR;
    }

    let mut argc = 0i32;
    let arguments = CommandLineToArgvW(command_line, &mut argc);
    if arguments.is_null() {
        let _ = FreeEnvironmentStringsW(environment);
        return EXIT_FIXTURE_ERROR;
    }
    if argc < 1 {
        let _ = release_inputs(arguments, environment);
        return EXIT_FIXTURE_ERROR;
    }

    let executable = *arguments;
    if executable.is_null() {
        let _ = release_inputs(arguments, environment);
        return EXIT_FIXTURE_ERROR;
    }
    let role = match role_from_executable(executable) {
        Some(role) => role,
        None => {
            let _ = release_inputs(arguments, environment);
            return EXIT_FIXTURE_ERROR;
        }
    };

    let mut index = 1i32;
    while index < argc {
        if (*arguments.add(index as usize)).is_null() {
            let _ = release_inputs(arguments, environment);
            return EXIT_FIXTURE_ERROR;
        }
        index += 1;
    }

    let trace = CreateFileW(
        config.trace_path,
        FILE_APPEND_DATA,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        core::ptr::null(),
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        core::ptr::null_mut(),
    );
    if trace as isize == -1 {
        let _ = release_inputs(arguments, environment);
        return EXIT_FIXTURE_ERROR;
    }

    if FreeEnvironmentStringsW(environment) == 0 {
        let _ = CloseHandle(trace);
        let _ = LocalFree(arguments.cast());
        return EXIT_FIXTURE_ERROR;
    }

    let actual_argc = (argc - 1) as u32;
    let expected_call = actual_argc == 1
        && if role == ROLE_GRAPH {
            equals_ascii(*arguments.add(1), b"--help")
        } else {
            equals_ascii(*arguments.add(1), b"--version")
        };
    let selected_exit = if expected_call {
        if role == ROLE_GRAPH {
            config.graph_exit
        } else {
            config.wiki_exit
        }
    } else {
        EXIT_UNEXPECTED_ARGUMENTS
    };

    let record_written = write_record(trace, arguments, argc, role, selected_exit);
    let trace_flushed = FlushFileBuffers(trace) != 0;
    let trace_closed = CloseHandle(trace) != 0;
    let arguments_released = LocalFree(arguments.cast()).is_null();

    if record_written && trace_flushed && trace_closed && arguments_released {
        selected_exit
    } else {
        EXIT_FIXTURE_ERROR
    }
}

unsafe fn release_inputs(arguments: *mut *mut u16, environment: *mut u16) -> bool {
    let arguments_released = arguments.is_null() || LocalFree(arguments.cast()).is_null();
    let environment_released = environment.is_null() || FreeEnvironmentStringsW(environment) != 0;
    arguments_released && environment_released
}

unsafe fn parse_environment(block: *const u16) -> Option<EnvironmentConfig> {
    let mut graph_exit = None;
    let mut wiki_exit = None;
    let mut trace_path: *const u16 = core::ptr::null();
    let mut cursor = block;

    loop {
        if *cursor == 0 {
            break;
        }

        let mut separator = cursor;
        let mut key_length = 0usize;
        while *separator != 0 && *separator != b'=' as u16 {
            separator = separator.add(1);
            key_length = key_length.checked_add(1)?;
        }

        if *separator == b'=' as u16 {
            let value = separator.add(1);
            if equals_key(cursor, key_length, b"CBG_A09_GRAPH_EXIT") {
                if graph_exit.is_some() {
                    return None;
                }
                graph_exit = Some(parse_exit_value(value, b'3' as u16, 23)?);
            } else if equals_key(cursor, key_length, b"CBG_A09_WIKI_EXIT") {
                if wiki_exit.is_some() {
                    return None;
                }
                wiki_exit = Some(parse_exit_value(value, b'9' as u16, 29)?);
            } else if equals_key(cursor, key_length, b"CBG_A09_TRACE") {
                if !trace_path.is_null() {
                    return None;
                }
                if !is_absolute_path(value) {
                    return None;
                }
                trace_path = value;
            }
        }

        while *cursor != 0 {
            cursor = cursor.add(1);
        }
        cursor = cursor.add(1);
    }

    Some(EnvironmentConfig {
        graph_exit: graph_exit?,
        wiki_exit: wiki_exit?,
        trace_path: if trace_path.is_null() {
            return None;
        } else {
            trace_path
        },
    })
}

unsafe fn parse_exit_value(
    value: *const u16,
    nonzero_digit: u16,
    nonzero_exit: u32,
) -> Option<u32> {
    let first = *value;
    if first == b'0' as u16 && *value.add(1) == 0 {
        return Some(0);
    }
    if first == b'2' as u16 && *value.add(1) == nonzero_digit && *value.add(2) == 0 {
        return Some(nonzero_exit);
    }
    None
}

unsafe fn equals_key(value: *const u16, length: usize, expected: &[u8]) -> bool {
    if length != expected.len() {
        return false;
    }
    let mut index = 0usize;
    while index < length {
        if *value.add(index) != *expected.as_ptr().add(index) as u16 {
            return false;
        }
        index += 1;
    }
    true
}

unsafe fn is_absolute_path(path: *const u16) -> bool {
    let length = match wide_length(path) {
        Some(length) => length,
        None => return false,
    };
    if length >= 3
        && is_ascii_letter(*path)
        && *path.add(1) == b':' as u16
        && is_separator(*path.add(2))
    {
        return true;
    }
    length >= 2 && is_separator(*path) && is_separator(*path.add(1))
}

fn is_ascii_letter(value: u16) -> bool {
    (value >= b'A' as u16 && value <= b'Z' as u16) || (value >= b'a' as u16 && value <= b'z' as u16)
}

fn is_separator(value: u16) -> bool {
    value == b'\\' as u16 || value == b'/' as u16
}

unsafe fn role_from_executable(executable: *const u16) -> Option<u32> {
    let length = wide_length(executable)?;
    let mut basename = executable;
    let mut index = 0usize;
    while index < length {
        if is_separator(*executable.add(index)) {
            basename = executable.add(index + 1);
        }
        index += 1;
    }

    if equals_ascii_case_insensitive(basename, b"codebase-graph.exe") {
        Some(ROLE_GRAPH)
    } else if equals_ascii_case_insensitive(basename, b"k-wiki.exe") {
        Some(ROLE_WIKI)
    } else {
        None
    }
}

unsafe fn equals_ascii_case_insensitive(value: *const u16, expected: &[u8]) -> bool {
    let length = match wide_length(value) {
        Some(length) => length,
        None => return false,
    };
    if length != expected.len() {
        return false;
    }

    let mut index = 0usize;
    while index < length {
        let actual = *value.add(index);
        let expected = *expected.as_ptr().add(index) as u16;
        if fold_ascii(actual) != fold_ascii(expected) {
            return false;
        }
        index += 1;
    }
    true
}

fn fold_ascii(value: u16) -> u16 {
    if value >= b'A' as u16 && value <= b'Z' as u16 {
        value + (b'a' as u16 - b'A' as u16)
    } else {
        value
    }
}

unsafe fn equals_ascii(value: *const u16, expected: &[u8]) -> bool {
    let length = match wide_length(value) {
        Some(length) => length,
        None => return false,
    };
    if length != expected.len() {
        return false;
    }

    let mut index = 0usize;
    while index < length {
        if *value.add(index) != *expected.as_ptr().add(index) as u16 {
            return false;
        }
        index += 1;
    }
    true
}

unsafe fn wide_length(value: *const u16) -> Option<usize> {
    if value.is_null() {
        return None;
    }
    let mut length = 0usize;
    while *value.add(length) != 0 {
        length = length.checked_add(1)?;
    }
    Some(length)
}

unsafe fn write_record(
    file: *mut c_void,
    arguments: *mut *mut u16,
    argc: i32,
    role: u32,
    selected_exit: u32,
) -> bool {
    let Some(argument_count) = u32::try_from(argc - 1).ok() else {
        return false;
    };

    if !write_u32(file, TRACE_VERSION) || !write_u32(file, role) || !write_u32(file, argument_count)
    {
        return false;
    }

    let mut index = 1i32;
    while index < argc {
        let argument = *arguments.add(index as usize);
        let length = match wide_length(argument) {
            Some(length) if length <= u32::MAX as usize => length,
            _ => return false,
        };
        let length_u32 = length as u32;
        if !write_u32(file, length_u32) {
            return false;
        }
        if length > 0 {
            let byte_length = match length_u32.checked_mul(2) {
                Some(byte_length) => byte_length,
                None => return false,
            };
            if !write_all(file, argument.cast::<u8>(), byte_length) {
                return false;
            }
        }
        index += 1;
    }

    write_u32(file, selected_exit)
}

unsafe fn write_u32(file: *mut c_void, value: u32) -> bool {
    let bytes = value.to_le_bytes();
    write_all(file, bytes.as_ptr(), bytes.len() as u32)
}

unsafe fn write_all(file: *mut c_void, buffer: *const u8, length: u32) -> bool {
    let mut written_total = 0u32;
    while written_total < length {
        let mut written = 0u32;
        let remaining = length - written_total;
        let result = WriteFile(
            file,
            buffer.add(written_total as usize).cast(),
            remaining,
            &mut written,
            core::ptr::null_mut(),
        );
        if result == 0 || written == 0 || written > remaining {
            return false;
        }
        written_total += written;
    }
    true
}
