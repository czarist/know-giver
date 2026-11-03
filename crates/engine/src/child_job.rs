//! Ties helper processes to the app's lifetime on Windows.
//!
//! Windows keeps a child running when its parent ends. Each helper the app
//! starts (the local OCR server) joins one job object whose last handle
//! belongs to this process; when the app ends for any reason, even a crash
//! or a forced close, Windows closes that handle and ends the job's processes.
//! Elsewhere this does nothing: the server is stopped when the engine drops.
#![allow(unsafe_code)]

use std::process::Child;

/// Puts `child` in the app's job. A failure is logged and otherwise ignored:
/// the server still stops when the engine drops.
pub(crate) fn tie_to_app(child: &Child) {
    #[cfg(windows)]
    windows::tie(child);
    #[cfg(not(windows))]
    let _ = child;
}

#[cfg(windows)]
mod windows {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::sync::OnceLock;

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    /// The job's handle as an address; never closed, so it lives exactly as
    /// long as the process.
    static JOB: OnceLock<Option<usize>> = OnceLock::new();

    fn job() -> Option<usize> {
        *JOB.get_or_init(|| {
            // SAFETY: no attributes and no name are valid arguments; a null
            // result is handled below.
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                tracing::warn!(error = %std::io::Error::last_os_error(), "could not create a job for helper processes");
                return None;
            }
            // SAFETY: an all-zero structure is a valid "no limits" value.
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).ok()?;
            // SAFETY: `job` is a live job handle and `info` outlives the call
            // with the size passed.
            let set = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast(),
                    size,
                )
            };
            if set == 0 {
                tracing::warn!(error = %std::io::Error::last_os_error(), "could not set the helper job's limits");
                return None;
            }
            Some(job as usize)
        })
    }

    pub(super) fn tie(child: &Child) {
        let Some(job) = job() else {
            return;
        };
        // SAFETY: both handles are live: the job is never closed and `child`
        // owns its process handle for the duration of the call.
        let assigned =
            unsafe { AssignProcessToJobObject(job as *mut _, child.as_raw_handle().cast()) };
        if assigned == 0 {
            tracing::warn!(error = %std::io::Error::last_os_error(), pid = child.id(), "could not tie a helper process to the app");
        }
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const PARENT: &str = "KG_CHILD_JOB_PARENT";

    fn running(pid: u32) -> bool {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }

    /// Runs itself as a parent that starts a helper, ties it and exits at once,
    /// without stopping it; the helper must not outlive it.
    #[test]
    fn a_helper_ends_with_the_app_even_without_cleanup() {
        if std::env::var_os(PARENT).is_some() {
            let child = Command::new("ping")
                .args(["-n", "60", "127.0.0.1"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            super::tie_to_app(&child);
            println!("helper-pid={}", child.id());
            std::process::exit(0);
        }
        // Read only the pid line: the helper inherits the pipe, so waiting for
        // the end of the output would wait for the helper too.
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "child_job::tests::a_helper_ends_with_the_app_even_without_cleanup",
                "--nocapture",
            ])
            .env(PARENT, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut lines =
            std::io::BufRead::lines(std::io::BufReader::new(parent.stdout.take().unwrap()));
        let pid: u32 = lines
            .find_map(|line| {
                line.ok()?
                    .strip_prefix("helper-pid=")
                    .and_then(|pid| pid.trim().parse().ok())
            })
            .expect("the parent starts a helper");
        parent.wait().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while running(pid) {
            assert!(Instant::now() < deadline, "helper {pid} outlived the app");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
