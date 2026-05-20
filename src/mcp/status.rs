use std::fs;
use std::path::Path;
use sysinfo::{Pid, System};

pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub uptime_secs: Option<u64>,
    pub log_path: String,
}

pub fn get_status(pid_path: &Path, port_path: &Path, log_path: &Path) -> DaemonStatus {
    let pid = fs::read_to_string(pid_path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    let Some(pid) = pid else {
        return DaemonStatus {
            running: false,
            pid: None,
            port: None,
            uptime_secs: None,
            log_path: log_path.display().to_string(),
        };
    };

    let mut sys = System::new();
    sys.refresh_processes();
    let process = sys.process(Pid::from_u32(pid));

    if process.is_none() {
        return DaemonStatus {
            running: false,
            pid: Some(pid),
            port: None,
            uptime_secs: None,
            log_path: log_path.display().to_string(),
        };
    }

    let uptime_secs = process.map(|p| {
        let started = p.start_time();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(started)
    });

    let port = fs::read_to_string(port_path)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok());

    DaemonStatus {
        running: true,
        pid: Some(pid),
        port,
        uptime_secs,
        log_path: log_path.display().to_string(),
    }
}

pub fn format_status(s: &DaemonStatus) -> String {
    if !s.running {
        return "Status: stopped".to_string();
    }

    let uptime = s.uptime_secs.map(format_uptime).unwrap_or_default();
    let port = s
        .port
        .map(|p| p.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        "Status: running\nPID:    {}\nPort:   {}\nUptime: {}\nLog:    {}",
        s.pid.unwrap_or(0),
        port,
        uptime,
        s.log_path,
    )
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
