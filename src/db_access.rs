//! Bounded lock acquisition for interactive requests and background workers.
use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use std::{
    fs::{self, File},
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub const INTERACTIVE_WAIT: Duration = Duration::from_secs(2);
pub const BACKGROUND_WAIT: Duration = Duration::from_secs(120);

pub fn lock_migration(file: &File, wait: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        match fs2::FileExt::try_lock_exclusive(file) {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                if started.elapsed() >= wait {
                    bail!("database_busy: 数据库暂时被占用，请稍后重试")
                }
                thread::sleep(
                    Duration::from_millis(20).min(wait.saturating_sub(started.elapsed())),
                );
            }
            Err(error) => return Err(error).context("无法取得数据库升级锁"),
        }
    }
}

pub(super) fn open_at_with_timeout(path: &Path, wait: Duration) -> Result<Connection> {
    // Serialize backup and migration together across Core workers and app windows.
    // The file is scoped to this database and is released when initialization ends.
    let lock_path = path.with_file_name(format!(
        "{}.migration.lock",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let migration_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    lock_migration(&migration_lock, wait)?;
    super::backup_before_upgrade(path, wait)?;
    let mut db = Connection::open(path).context("无法打开 SiaoCut SQLite 数据库")?;
    db.busy_timeout(wait)?;
    db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    super::migrate(&mut db)?;
    Ok(db)
}
