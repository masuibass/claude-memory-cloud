use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Logs/memory-cloud")
    } else {
        dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("memory-cloud/logs")
    }
}

pub fn log_path() -> PathBuf {
    log_dir().join("cli.log")
}

pub fn init() {
    LOG_PATH.get_or_init(|| {
        let dir = log_dir();
        fs::create_dir_all(&dir).ok()?;
        Some(dir.join("cli.log"))
    });
}

fn write_log(level: &str, msg: &str) {
    let Some(Some(path)) = LOG_PATH.get() else {
        return;
    };
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let _ = writeln!(f, "{ts} [{level}] {msg}");
}

pub fn info(msg: &str) {
    write_log("INFO", msg);
}

pub fn error(msg: &str) {
    write_log("ERROR", msg);
}
