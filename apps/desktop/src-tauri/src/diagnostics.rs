use chrono::Utc;
use log::{LevelFilter, Log, Metadata, Record};
use std::{
    env,
    ffi::OsString,
    fs,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const LOG_FILE_NAME: &str = "siaocut-desktop.log";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const HISTORY_FILES: usize = 3;
const MAX_DETAIL_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub struct Diagnostics {
    pub log_directory: Option<PathBuf>,
    pub initialization_error: Option<String>,
}

struct LoggerState {
    directory: PathBuf,
    file: Option<File>,
}

struct LocalLogger {
    state: Mutex<LoggerState>,
}

static LOGGER: OnceLock<LocalLogger> = OnceLock::new();

impl Log for LocalLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let detail = sanitize_detail(&record.args().to_string());
        let line = format!(
            "{} level={} target={} {}\n",
            Utc::now().to_rfc3339(),
            record.level(),
            record.target(),
            detail
        );
        if let Ok(mut state) = self.state.lock() {
            let _ = write_line(&mut state, line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut state) = self.state.lock()
            && let Some(file) = state.file.as_mut()
        {
            let _ = file.flush();
        }
    }
}

pub fn initialize() -> Diagnostics {
    let candidates = candidate_directories(env::var_os("LOCALAPPDATA"), env::temp_dir());

    let mut errors = Vec::new();
    for directory in candidates {
        match open_state(&directory) {
            Ok(state) => {
                if LOGGER
                    .set(LocalLogger {
                        state: Mutex::new(state),
                    })
                    .is_err()
                {
                    return Diagnostics {
                        log_directory: Some(directory),
                        initialization_error: Some("诊断日志已由其他组件初始化。".to_owned()),
                    };
                }
                if let Err(error) = log::set_logger(LOGGER.get().expect("logger initialized")) {
                    return Diagnostics {
                        log_directory: Some(directory),
                        initialization_error: Some(format!("无法启用诊断日志：{error}")),
                    };
                }
                log::set_max_level(LevelFilter::Info);
                install_panic_hook();
                log::info!("event=diagnostics_ready");
                return Diagnostics {
                    log_directory: Some(directory),
                    initialization_error: None,
                };
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    Diagnostics {
        log_directory: None,
        initialization_error: Some(errors.join("；")),
    }
}

fn candidate_directories(local_app_data: Option<OsString>, temp_dir: PathBuf) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local_app_data) = local_app_data {
        candidates.push(PathBuf::from(local_app_data).join("SiaoCut").join("logs"));
    }
    candidates.push(temp_dir.join("SiaoCut").join("logs"));
    candidates.dedup();
    candidates
}

pub fn sanitize_detail(value: &str) -> String {
    let flattened = value.replace("\r\n", "\\n").replace(['\r', '\n'], "\\n");
    if flattened.len() <= MAX_DETAIL_BYTES {
        return flattened;
    }
    let mut end = MAX_DETAIL_BYTES;
    while !flattened.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &flattened[..end])
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("event=panic detail={}", sanitize_detail(&info.to_string()));
        log::logger().flush();
        previous(info);
    }));
}

fn open_state(directory: &Path) -> std::io::Result<LoggerState> {
    fs::create_dir_all(directory)?;
    let current = directory.join(LOG_FILE_NAME);
    if current.metadata().map(|item| item.len()).unwrap_or(0) >= MAX_LOG_BYTES {
        rotate(directory)?;
    }
    let file = open_log_file(&current)?;
    Ok(LoggerState {
        directory: directory.to_path_buf(),
        file: Some(file),
    })
}

fn open_log_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn write_line(state: &mut LoggerState, line: &[u8]) -> std::io::Result<()> {
    let current_length = state
        .file
        .as_ref()
        .and_then(|file| file.metadata().ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if current_length.saturating_add(line.len() as u64) > MAX_LOG_BYTES {
        state.file.take();
        rotate(&state.directory)?;
        state.file = Some(open_log_file(&state.directory.join(LOG_FILE_NAME))?);
    }
    if let Some(file) = state.file.as_mut() {
        file.write_all(line)?;
        if line
            .windows(b"level=ERROR".len())
            .any(|window| window == b"level=ERROR")
        {
            file.flush()?;
        }
    }
    Ok(())
}

fn rotated_path(directory: &Path, index: usize) -> PathBuf {
    directory.join(format!("{LOG_FILE_NAME}.{index}"))
}

fn rotate(directory: &Path) -> std::io::Result<()> {
    let oldest = rotated_path(directory, HISTORY_FILES);
    if oldest.is_file() {
        fs::remove_file(&oldest)?;
    }
    for index in (2..=HISTORY_FILES).rev() {
        let source = rotated_path(directory, index - 1);
        if source.is_file() {
            fs::rename(source, rotated_path(directory, index))?;
        }
    }
    let current = directory.join(LOG_FILE_NAME);
    if current.is_file() {
        fs::rename(current, rotated_path(directory, 1))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rotates_at_the_size_limit_and_keeps_three_history_files() {
        let temp = tempdir().unwrap();
        let current = temp.path().join(LOG_FILE_NAME);
        fs::write(&current, vec![b'x'; MAX_LOG_BYTES as usize]).unwrap();
        for index in 1..=HISTORY_FILES {
            fs::write(rotated_path(temp.path(), index), format!("old-{index}")).unwrap();
        }

        let mut state = open_state(temp.path()).unwrap();
        write_line(&mut state, b"next\n").unwrap();

        assert_eq!(fs::read_to_string(&current).unwrap(), "next\n");
        assert_eq!(
            fs::metadata(rotated_path(temp.path(), 1)).unwrap().len(),
            MAX_LOG_BYTES
        );
        assert_eq!(
            fs::read_to_string(rotated_path(temp.path(), 2)).unwrap(),
            "old-1"
        );
        assert_eq!(
            fs::read_to_string(rotated_path(temp.path(), 3)).unwrap(),
            "old-2"
        );
    }

    #[test]
    fn flattens_and_truncates_diagnostic_details() {
        assert_eq!(sanitize_detail("first\r\nsecond"), "first\\nsecond");
        let truncated = sanitize_detail(&"中".repeat(MAX_DETAIL_BYTES));
        assert!(truncated.ends_with('…'));
        assert!(truncated.len() <= MAX_DETAIL_BYTES + '…'.len_utf8());
    }

    #[test]
    fn falls_back_to_the_temporary_directory() {
        let temp = PathBuf::from(r"C:\Temp");
        let candidates = candidate_directories(None, temp.clone());
        assert_eq!(candidates, vec![temp.join("SiaoCut").join("logs")]);
    }
}
