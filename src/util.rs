use chrono::Utc;
use std::{
    env,
    ffi::OsStr,
    io, mem,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::Path,
    process::{Child, Command},
    ptr,
};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{CloseHandle, STILL_ACTIVE},
    System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CreateProcessW, GetExitCodeProcess,
        OpenProcess, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
    },
};

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..8])
}

/// Create a console application without allocating or inheriting a visible
/// Windows console. stdout and stderr remain available to `output`/`spawn`.
pub fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Start another Core role without inheriting the caller's redirected handles.
///
/// `std::process::Command` can keep a Windows launcher or Tauri output pipe
/// alive until a detached descendant exits. Using `bInheritHandles = FALSE`
/// keeps CLI responses independent from the service and long-running workers.
pub fn spawn_detached_current(arguments: &[&str]) -> io::Result<()> {
    if arguments.iter().any(|argument| {
        argument.contains('\0')
            || argument.contains('"')
            || argument.chars().any(char::is_whitespace)
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "detached Core arguments must not contain whitespace or quotes",
        ));
    }
    let executable = env::current_exe()?;
    let mut application = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut command_line = format!("\"{}\"", executable.display());
    for argument in arguments {
        command_line.push(' ');
        command_line.push_str(argument);
    }
    let mut command_line = command_line
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut startup: STARTUPINFOW = unsafe { mem::zeroed() };
    startup.cb = mem::size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            application.as_mut_ptr(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
            ptr::null(),
            ptr::null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    Ok(())
}

pub fn process_is_active(process_id: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0_u32;
    let read = unsafe { GetExitCodeProcess(handle, &mut exit_code) } != 0;
    unsafe { CloseHandle(handle) };
    read && exit_code == STILL_ACTIVE as u32
}

pub fn terminate_process_tree(child: &mut Child) {
    let terminated = terminate_process_tree_by_id(child.id());
    if !terminated {
        let _ = child.kill();
    }
    let _ = child.wait();
}

pub fn terminate_process_tree_by_id(process_id: u32) -> bool {
    let process_id = process_id.to_string();
    hidden_command("taskkill.exe")
        .args(["/PID", &process_id, "/T", "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn available_space(path: &Path) -> io::Result<u64> {
    if let Some(value) = env::var("SIAOCUT_TEST_AVAILABLE_SPACE_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Ok(value);
    }
    fs2::available_space(path)
}
