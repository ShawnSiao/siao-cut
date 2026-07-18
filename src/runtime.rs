use crate::{
    db,
    media::hash_file,
    util::{hidden_command, now},
};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSelection {
    pub backend: String,
    pub whisper_path: String,
    pub executable_sha256: String,
    pub source: String,
    pub version: String,
    pub archive_sha256: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    pub selected_at: String,
}

fn display_path(path: &Path) -> Result<String> {
    let path = path.canonicalize()?.to_string_lossy().to_string();
    Ok(path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned())
}

fn selection_path() -> PathBuf {
    db::home_dir().join("runtime-selection.json")
}

pub fn load() -> Result<Option<RuntimeSelection>> {
    let path = selection_path();
    if !path.is_file() {
        return Ok(None);
    }
    let selection: RuntimeSelection = serde_json::from_slice(&fs::read(&path)?)?;
    Ok(Some(selection))
}

pub fn selected_whisper_path() -> Option<PathBuf> {
    load()
        .ok()
        .flatten()
        .map(|selection| PathBuf::from(selection.whisper_path))
        .filter(|path| path.is_file())
}

pub fn status() -> Result<serde_json::Value> {
    let selection = load()?;
    Ok(match selection {
        Some(selection) => {
            let path = PathBuf::from(&selection.whisper_path);
            serde_json::json!({
                "backend": selection.backend,
                "selected": true,
                "available": path.is_file(),
                "selection": selection
            })
        }
        None => serde_json::json!({
            "backend": "cpu",
            "selected": false,
            "available": true
        }),
    })
}

pub fn select(
    backend: &str,
    whisper: &Path,
    source: Option<String>,
    version: Option<String>,
    archive_sha256: Option<String>,
) -> Result<RuntimeSelection> {
    if !["cpu", "cuda", "vulkan"].contains(&backend) {
        bail!("仅支持 cpu、cuda 或 vulkan 运行时")
    }
    if !whisper.is_file() {
        bail!("whisper.cpp 运行时不存在：{}", whisper.display())
    }
    let output = hidden_command(whisper)
        .arg("--version")
        .output()
        .map_err(|error| anyhow!("无法启动 whisper.cpp 运行时：{error}"))?;
    if !output.status.success() {
        bail!("所选 whisper.cpp 运行时无法通过健康检查")
    }
    let probe = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let device = match backend {
        "vulkan" => {
            if !probe.contains("ggml_vulkan: Found") || probe.contains("ggml_vulkan: Found 0") {
                bail!("未检测到可用的 Vulkan 显卡；请继续使用 CPU")
            }
            probe.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("ggml_vulkan: 0 = ")
                    .map(|value| value.split(" (").next().unwrap_or(value).to_owned())
            })
        }
        "cuda" => {
            let lower = probe.to_lowercase();
            if lower.contains("no gpu found") || !lower.contains("cuda") {
                bail!("未检测到可用的 CUDA 运行时；请继续使用 CPU 或 Vulkan")
            }
            Some("CUDA".to_owned())
        }
        _ => None,
    };
    let selection = RuntimeSelection {
        backend: backend.to_owned(),
        whisper_path: display_path(whisper)?,
        executable_sha256: hash_file(whisper)?,
        source: source.unwrap_or_else(|| "manual".into()),
        version: version.unwrap_or_else(|| "unknown".into()),
        archive_sha256,
        device,
        selected_at: now(),
    };
    fs::create_dir_all(db::home_dir())?;
    let path = selection_path();
    let partial = path.with_extension("json.part");
    fs::write(&partial, serde_json::to_vec_pretty(&selection)?)?;
    fs::rename(partial, path)?;
    Ok(selection)
}

pub fn reset() -> Result<()> {
    let path = selection_path();
    if path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_backend_before_touching_disk() {
        let error = select("magic", Path::new("missing.exe"), None, None, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("cpu、cuda 或 vulkan"));
    }
}
