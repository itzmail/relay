use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::Path;
use sysinfo::{Pid, System};

pub fn read_pid(pid_path: &Path) -> Option<u32> {
    fs::read_to_string(pid_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn is_pid_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_processes();
    sys.process(Pid::from_u32(pid)).is_some()
}

pub fn write_pid(pid_path: &Path, pid: u32) -> Result<()> {
    let mut f = fs::File::create(pid_path)?;
    write!(f, "{pid}")?;
    Ok(())
}

pub fn write_port(port_path: &Path, port: u16) -> Result<()> {
    let mut f = fs::File::create(port_path)?;
    write!(f, "{port}")?;
    Ok(())
}

pub fn cleanup(pid_path: &Path, port_path: &Path) {
    let _ = fs::remove_file(pid_path);
    let _ = fs::remove_file(port_path);
}

/// Spawn detached child process (re-exec with --foreground).
/// Called from the parent process — returns immediately after child starts.
#[cfg(unix)]
pub fn spawn_daemon(port: u16, log_path: &Path) -> Result<u32> {
    use std::process::{Command, Stdio};
    use std::os::unix::process::CommandExt;

    let exe = std::env::current_exe().context("cannot determine relay binary path")?;
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .context("cannot open daemon log file")?;
    let log_file2 = log_file.try_clone()?;

    let child = unsafe {
        Command::new(&exe)
            .args(["mcp", "start", "--foreground", "--port", &port.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file2))
            .pre_exec(|| {
                // setsid: detach from parent's process group
                nix::libc::setsid();
                Ok(())
            })
            .spawn()
            .context("failed to spawn relay daemon")?
    };

    Ok(child.id())
}

#[cfg(unix)]
pub fn stop_daemon(pid: u32) -> Result<()> {
    if !is_pid_alive(pid) {
        return Ok(());
    }
    kill_pid(pid)
}

/// SIGTERM → poll 5s → SIGKILL. Shared by daemon stop and spawn module.
#[cfg(unix)]
pub fn kill_pid(pid: u32) -> Result<()> {
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid as NixPid;
    use std::time::{Duration, Instant};

    let nix_pid = NixPid::from_raw(pid as i32);

    if signal::kill(nix_pid, Signal::SIGTERM).is_err() {
        bail!("Process {pid} not found or permission denied");
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        std::thread::sleep(Duration::from_millis(200));
        if !is_pid_alive(pid) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
    }

    let _ = signal::kill(nix_pid, Signal::SIGKILL);
    Ok(())
}

#[cfg(not(unix))]
pub fn kill_pid(_pid: u32) -> Result<()> {
    bail!("kill_pid not supported on Windows.");
}

#[cfg(not(unix))]
pub fn spawn_daemon(_port: u16, _log_path: &Path) -> Result<u32> {
    bail!("Daemon mode not supported on Windows. Use --foreground instead.");
}

#[cfg(not(unix))]
pub fn stop_daemon(_pid: u32) -> Result<()> {
    bail!("Stop not supported on Windows.");
}
