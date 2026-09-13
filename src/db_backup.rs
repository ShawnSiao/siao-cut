//! Preserve the existing database before applying migrations.
use super::*;

pub(super) fn backup_before_upgrade(path: &Path, wait: Duration) -> Result<Option<PathBuf>> {
    if !path.is_file() {
        return Ok(None);
    }
    let source = Connection::open(path).context("无法读取待升级的 SiaoCut 数据库")?;
    source.busy_timeout(wait)?;
    let has_migrations: bool = source.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !has_migrations {
        return Ok(None);
    }
    let installed: i64 = source.query_row(
        "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if installed <= 0 || installed >= CURRENT_SCHEMA_VERSION {
        return Ok(None);
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("siaocut.db");
    let backup_path = path.with_file_name(format!("{file_name}.schema-{installed}.bak"));
    if backup_path.exists() {
        return Ok(Some(backup_path));
    }
    let partial_path =
        backup_path.with_file_name(format!("{file_name}.schema-{installed}.bak.partial"));
    if partial_path.is_file() {
        fs::remove_file(&partial_path).context("无法清理未完成的数据库备份")?;
    }
    let backup_result = (|| -> Result<()> {
        let mut destination =
            Connection::open(&partial_path).context("无法创建数据库升级前备份")?;
        let backup = rusqlite::backup::Backup::new(&source, &mut destination)
            .context("无法初始化数据库升级前备份")?;
        backup
            .run_to_completion(128, Duration::from_millis(10), None)
            .context("数据库升级前备份失败")?;
        Ok(())
    })();
    if let Err(error) = backup_result {
        let _ = fs::remove_file(&partial_path);
        return Err(error);
    }
    fs::rename(&partial_path, &backup_path).context("无法完成数据库升级前备份")?;
    Ok(Some(backup_path))
}
