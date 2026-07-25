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
    verified_selected_whisper_path().ok().flatten()
}

fn verify_selection(selection: &RuntimeSelection) -> Result<PathBuf> {
    let path = PathBuf::from(&selection.whisper_path);
    if !path.is_file() {
        bail!(
            "runtime_executable_missing: 已选择的 whisper.cpp 运行时不存在：{}",
            path.display()
        )
    }
    let actual = hash_file(&path)?;
    if !actual.eq_ignore_ascii_case(&selection.executable_sha256) {
        bail!(
            "runtime_hash_mismatch: whisper.cpp 运行时在选择后发生变化；需要 {}，实际为 {}",
            selection.executable_sha256,
            actual
        )
    }
    Ok(path)
}

pub fn verified_selected_whisper_path() -> Result<Option<PathBuf>> {
    load()?.as_ref().map(verify_selection).transpose()
}

pub fn status() -> Result<serde_json::Value> {
    let selection = load()?;
    Ok(match selection {
        Some(selection) => status_for_selection(selection),
        None => serde_json::json!({
            "backend": "cpu",
            "selected": false,
            "available": true
        }),
    })
}

fn status_for_selection(selection: RuntimeSelection) -> serde_json::Value {
    match verify_selection(&selection) {
        Ok(_) => serde_json::json!({
            "backend": selection.backend,
            "selected": true,
            "available": true,
            "verificationStatus": "verified",
            "selection": selection
        }),
        Err(error) => {
            let message = error.to_string();
            let code = message
                .split_once(':')
                .map(|(code, _)| code)
                .unwrap_or("runtime_verification_failed");
            serde_json::json!({
                "backend": selection.backend,
                "selected": true,
                "available": false,
                "verificationStatus": "failed",
                "errorCode": code,
                "errorMessage": message,
                "selection": selection
            })
        }
    }
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

    #[test]
    fn rejects_a_selected_runtime_that_changed_after_selection() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("whisper-cli.exe");
        fs::write(&executable, b"selected runtime").unwrap();
        let selected_hash = hash_file(&executable).unwrap();
        fs::write(&executable, b"tampered runtime").unwrap();
        let selection = RuntimeSelection {
            backend: "cpu".into(),
            whisper_path: executable.to_string_lossy().into_owned(),
            executable_sha256: selected_hash,
            source: "test".into(),
            version: "test".into(),
            archive_sha256: None,
            device: None,
            selected_at: "test".into(),
        };

        let error = verify_selection(&selection).unwrap_err().to_string();

        assert!(error.contains("runtime_hash_mismatch"));
        let status = status_for_selection(selection);
        assert_eq!(status["available"], false);
        assert_eq!(status["verificationStatus"], "failed");
        assert_eq!(status["errorCode"], "runtime_hash_mismatch");
    }
}
