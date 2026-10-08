//! Approved platform tools, run with argument arrays, a clean environment, bounded output,
//! a deadline and cooperative cancellation.
use crate::{model::Result, policy, ports::*};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub const APPROVED: &[&str] = &["/usr/bin/git", "/usr/bin/xcrun", "/bin/launchctl"];
/// Container command-line tools, where their installers put them.
pub const CONTAINER_TOOLS: &[&str] = &[
    "/usr/local/bin/docker",
    "/opt/homebrew/bin/docker",
    "/Applications/Docker.app/Contents/Resources/bin/docker",
    "/Applications/OrbStack.app/Contents/MacOS/xbin/docker",
    "/usr/local/bin/podman",
    "/opt/homebrew/bin/podman",
    "/opt/podman/bin/podman",
];
/// The same, relative to the home folder.
pub const HOME_CONTAINER_TOOLS: &[&str] = &[
    ".orbstack/bin/docker",
    ".docker/bin/docker",
    ".rd/bin/docker",
];
/// Whether an executable is one this runner may start.
pub fn approved(executable: &str) -> bool {
    APPROVED.contains(&executable)
        || CONTAINER_TOOLS.contains(&executable)
        || HOME_CONTAINER_TOOLS
            .iter()
            .any(|t| executable == format!("{}/{t}", policy::home()))
}
const LIMIT: usize = 4_194_304;

pub struct SystemRunner;
impl CommandRunner for SystemRunner {
    fn run(
        &self,
        executable: &str,
        args: &[String],
        timeout: Duration,
        control: &ScanControl,
    ) -> Result<Output> {
        run(executable, args, timeout, control)
    }
}
fn run(
    executable: &str,
    args: &[String],
    timeout: Duration,
    control: &ScanControl,
) -> Result<Output> {
    if !approved(executable) {
        return Err("Unapproved executable".into());
    }
    // A tool finds its own helpers (credential helpers, plugins) beside it.
    let folder = std::path::Path::new(executable)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("/usr/bin");
    control.check()?;
    let mut child = Command::new(executable)
        .args(args)
        .env_clear()
        .env("HOME", policy::home())
        .env("PATH", format!("{folder}:/usr/bin:/bin:/usr/sbin:/sbin"))
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A private process group lets timeout and cancellation stop descendants too.
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Missing stdout pipe")?;
    let stderr = child.stderr.take().ok_or("Missing stderr pipe")?;
    let (sender, receiver) = mpsc::channel();
    for (index, pipe) in [
        Box::new(stdout) as Box<dyn Read + Send>,
        Box::new(stderr) as Box<dyn Read + Send>,
    ]
    .into_iter()
    .enumerate()
    {
        let sender = sender.clone();
        thread::spawn(move || {
            let _ = sender.send((index, read_bounded(pipe)));
        });
    }
    drop(sender);
    let deadline = Instant::now() + timeout;
    let stopped = |control: &ScanControl| {
        if control.check().is_err() {
            "Command cancelled"
        } else {
            "Command timed out"
        }
    };
    let status = loop {
        if control.check().is_err() || Instant::now() >= deadline {
            stop(&mut child);
            return Err(stopped(control).into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                stop(&mut child);
                return Err(e.to_string());
            }
        }
    };
    // The child has exited, but a descendant can still hold its pipes open. Readers are
    // bounded by the same deadline and are left to finish on their own if it passes.
    let mut outputs: [Option<Vec<u8>>; 2] = [None, None];
    while outputs.iter().any(Option::is_none) {
        if control.check().is_err() || Instant::now() >= deadline {
            // Descendants still hold the pipes, so the group still exists and its ID cannot
            // have been reused; stop them with it.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            return Err(format!(
                "{}; a descendant process kept its output open",
                stopped(control)
            ));
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok((index, data)) => outputs[index] = Some(data?),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("Command reader failed".into()),
        }
    }
    let [data, error] = outputs.map(Option::unwrap_or_default);
    Ok(Output {
        data,
        error: String::from_utf8_lossy(&error).into(),
        status,
    })
}
fn stop(child: &mut Child) {
    // The unreaped child keeps its process-group ID reserved, so this cannot reach an unrelated group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
fn read_bounded(mut pipe: impl Read) -> Result<Vec<u8>> {
    let mut output = vec![];
    let mut buffer = [0u8; 16_384];
    let mut overflow = false;
    loop {
        let count = pipe.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if output.len() + count <= LIMIT {
            output.extend_from_slice(&buffer[..count]);
        } else {
            overflow = true;
        }
    }
    if overflow {
        Err("Command output exceeded the 4 MB limit".into())
    } else {
        Ok(output)
    }
}
