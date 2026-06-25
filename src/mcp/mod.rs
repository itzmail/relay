pub mod cli;
pub mod daemon;
pub mod db;
pub mod installer;
pub mod server;
pub mod status;
pub mod tools;

use std::path::PathBuf;

pub struct RelayPaths {
    pub dir: PathBuf,
    pub pid: PathBuf,
    pub port: PathBuf,
    pub log: PathBuf,
    pub db: PathBuf,
}

pub fn paths() -> RelayPaths {
    let dir = dirs::home_dir()
        .expect("cannot resolve home dir")
        .join(".relay");
    RelayPaths {
        pid: dir.join("daemon.pid"),
        port: dir.join("daemon.port"),
        log: dir.join("daemon.log"),
        db: dir.join("relay.db"),
        dir,
    }
}
