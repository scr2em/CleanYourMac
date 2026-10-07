use crate::{files::ScanControl, model::Result, policy};
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Output {
    pub data: Vec<u8>,
    pub error: String,
    pub status: i32,
}
impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).into()
    }
}
pub fn run(
    executable: &str,
    args: &[String],
    timeout: Duration,
    control: &ScanControl,
) -> Result<Output> {
    if !["/usr/bin/git", "/usr/bin/xcrun", "/bin/launchctl"].contains(&executable) {
        return Err("Unapproved executable".into());
    }
    control.check()?;
    let mut child = Command::new(executable)
        .args(args)
        .env_clear()
        .env("HOME", policy::home())
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Missing stdout pipe")?;
    let stderr = child.stderr.take().ok_or("Missing stderr pipe")?;
    // Pipe readers cannot grow without bound. Timeout/cancellation kills the owned child and closes its pipe writers.
    let out = thread::spawn(move || read_bounded(stdout));
    let err = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        if control.check().is_err() || started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            return Err(if control.check().is_err() {
                "Scan cancelled"
            } else {
                "Command timed out"
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    };
    let data = out.join().map_err(|_| "Command reader failed")??;
    let error = err.join().map_err(|_| "Command reader failed")??;
    Ok(Output {
        data,
        error: String::from_utf8_lossy(&error).into(),
        status,
    })
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
        if output.len() + count <= 4_194_304 {
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
