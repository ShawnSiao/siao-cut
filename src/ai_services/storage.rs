use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

use serde::{Serialize, de::DeserializeOwned};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, io::Error> {
    match fs::read(path) {
        Ok(contents) => serde_json::from_slice(&contents)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let part_path = path.with_extension("json.part");
    let backup_path = path.with_extension("json.backup");
    let mut file = File::create(&part_path)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()?;

    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }
    if path.exists() {
        fs::rename(path, &backup_path)?;
    }
    if let Err(error) = fs::rename(&part_path, path) {
        if backup_path.exists() {
            let _ = fs::rename(&backup_path, path);
        }
        return Err(error);
    }
    if backup_path.exists() {
        fs::remove_file(backup_path)?;
    }
    Ok(())
}
