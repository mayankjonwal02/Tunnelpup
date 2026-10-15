//! Keeps TunnelPup's connector from outliving TunnelPup.
//!
//! If TunnelPup is killed rather than quit (a crash, Task Manager, `tauri dev` restarting it),
//! the connector would keep running on its own: the user's sites stay online with TunnelPup
//! gone, and the running binary can't be replaced by an upgrade or a rebuild.
//!
//! - Windows: the connector joins a job object that the OS ends together with TunnelPup,
//!   however TunnelPup exits.
//! - Everywhere: the connector's PID is recorded while it runs. If TunnelPup didn't shut down
//!   cleanly, the next start ends the leftover connector.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// The bundled connector's name (`externalBin` in tauri.conf.json). It isn't plain
/// "cloudflared" so it can't clash with a cloudflared the user installed themselves.
pub const SIDECAR: &str = "tunnelpup-cloudflared";
const PID_FILE: &str = "connector.pid";

/// Have the OS end process `pid` when TunnelPup exits. Windows only; elsewhere `kill_leftover` covers it.
pub fn tie_to_app(pid: u32) {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        // Never closed by TunnelPup itself: the OS closes it when TunnelPup's process ends.
        static JOB: OnceLock<Option<win::KillOnClose>> = OnceLock::new();
        if let Some(job) = JOB.get_or_init(win::KillOnClose::new) {
            job.add(pid);
        }
    }
    #[cfg(not(windows))]
    let _ = pid;
}

/// Record the running connector, so a later start can end it if TunnelPup dies.
pub fn remember(app: &AppHandle, pid: u32) {
    if let Some(path) = pid_file(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, pid.to_string());
    }
}

/// The connector was stopped on purpose, so there's nothing to clean up next time.
pub fn forget(app: &AppHandle) {
    if let Some(path) = pid_file(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// End a connector left running by an earlier TunnelPup that didn't shut down cleanly.
pub fn kill_leftover(app: &AppHandle) {
    let Some(path) = pid_file(app) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let _ = std::fs::remove_file(&path);
    let Ok(pid) = text.trim().parse::<u32>() else { return };
    // The PID may belong to another program by now; only end it if it's still our connector.
    if executable(pid).is_some_and(|exe| is_connector(&exe)) {
        terminate(pid);
    }
}

fn pid_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(PID_FILE))
}

/// True if `exe` is TunnelPup's connector, `tunnelpup-cloudflared[.exe]`.
fn is_connector(exe: &Path) -> bool {
    exe.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case(SIDECAR))
}

#[cfg(windows)]
fn executable(pid: u32) -> Option<PathBuf> {
    win::executable(pid)
}

#[cfg(windows)]
fn terminate(pid: u32) {
    win::terminate(pid);
}

#[cfg(target_os = "linux")]
fn executable(pid: u32) -> Option<PathBuf> {
    let path = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    // A binary replaced by an upgrade shows up as "<path> (deleted)".
    let path = path.to_string_lossy();
    Some(PathBuf::from(path.strip_suffix(" (deleted)").unwrap_or(&path)))
}

#[cfg(target_os = "macos")]
fn executable(pid: u32) -> Option<PathBuf> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let len = unsafe { libc::proc_pidpath(pid as libc::c_int, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if len <= 0 {
        return None;
    }
    buf.truncate(len as usize);
    Some(PathBuf::from(String::from_utf8_lossy(&buf).into_owned()))
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn executable(_pid: u32) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
fn terminate(pid: u32) {
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
}

#[cfg(not(any(windows, unix)))]
fn terminate(_pid: u32) {}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        QueryFullProcessImageNameW, TerminateProcess,
    };

    /// A job object whose processes are all ended when it's closed, including when the OS
    /// closes it because the process holding it exited.
    pub struct KillOnClose(HANDLE);

    // The handle is only passed to kernel calls, which are safe from any thread.
    unsafe impl Send for KillOnClose {}
    unsafe impl Sync for KillOnClose {}

    impl KillOnClose {
        pub fn new() -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let job = Self(handle); // closed again by Drop if setup fails
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                );
                (ok != 0).then_some(job)
            }
        }

        pub fn add(&self, pid: u32) -> bool {
            unsafe {
                let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if process.is_null() {
                    return false;
                }
                let ok = AssignProcessToJobObject(self.0, process) != 0;
                CloseHandle(process);
                ok
            }
        }
    }

    impl Drop for KillOnClose {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn executable(pid: u32) -> Option<PathBuf> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
        }
    }

    pub fn terminate(pid: u32) {
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !process.is_null() {
                TerminateProcess(process, 1);
                CloseHandle(process);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        #[test]
        fn closing_the_job_ends_its_processes() {
            // Something that would otherwise run for about a minute.
            let mut child = Command::new("ping").args(["-n", "60", "127.0.0.1"]).stdout(Stdio::null()).spawn().unwrap();
            let job = KillOnClose::new().expect("job object");
            assert!(job.add(child.id()));
            assert!(child.try_wait().unwrap().is_none(), "still running while the job is open");
            drop(job);
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline, "process outlived its job");
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        #[test]
        fn reads_a_process_executable() {
            let mine = executable(std::process::id()).expect("own executable");
            assert_eq!(mine, std::env::current_exe().unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_only_tunnelpups_connector() {
        for yes in ["tunnelpup-cloudflared.exe", "C:/Program Files/TunnelPup/tunnelpup-cloudflared.exe", "/usr/bin/tunnelpup-cloudflared"] {
            assert!(is_connector(Path::new(yes)), "{yes}");
        }
        for no in ["cloudflared.exe", "/usr/bin/cloudflared", "tunnelpup.exe", "tunnelpup-cloudflared-helper"] {
            assert!(!is_connector(Path::new(no)), "{no}");
        }
    }
}
