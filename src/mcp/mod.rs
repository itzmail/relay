pub mod cli;
pub mod daemon;
pub mod db;
pub mod installer;
pub mod jobs;
pub mod server;
pub mod spawn;
pub mod status;
pub mod tools;

use std::path::PathBuf;

pub struct RelayPaths {
    pub dir: PathBuf,
    pub pid: PathBuf,
    pub port: PathBuf,
    pub log: PathBuf,
    pub db: PathBuf,
    pub jobs_dir: PathBuf,
}

pub fn paths() -> RelayPaths {
    let dir = dirs::home_dir()
        .expect("cannot resolve home dir")
        .join(".relay");
    let jobs_dir = dir.join("jobs");
    RelayPaths {
        pid: dir.join("daemon.pid"),
        port: dir.join("daemon.port"),
        log: dir.join("daemon.log"),
        db: dir.join("relay.db"),
        jobs_dir,
        dir,
    }
}
