use crate::{
    db,
    media::hash_file,
    util::{hidden_command, now},
};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const MANAGED_VULKAN_PATH_ENV: &str = "SIAOCUT_MANAGED_WHISPER_VULKAN_CLI";
const MANAGED_VULKAN_HASH_ENV: &str = "SIAOCUT_MANAGED_WHISPER_VULKAN_SHA256";
const MANAGED_VULKAN_SOURCE_ENV: &str = "SIAOCUT_MANAGED_WHISPER_VULKAN_SOURCE";
const MANAGED_VULKAN_VERSION_ENV: &str = "SIAOCUT_MANAGED_WHISPER_VULKAN_VERSION";

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

#[derive(Clone, Debug)]
struct ManagedRuntime {
    backend: String,
    whisper_path: PathBuf,
    executable_sha256: String,
    source: String,
    version: String,
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

fn persist_selection(selection: &RuntimeSelection) -> Result<()> {
    fs::create_dir_all(db::home_dir())?;
    let path = selection_path();
    let partial = path.with_extension("json.part");
    fs::write(&partial, serde_json::to_vec_pretty(selection)?)?;
    fs::rename(partial, path)?;
    Ok(())
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

fn managed_runtime_from_env() -> Result<Option<ManagedRuntime>> {
    let values = [
        env::var_os(MANAGED_VULKAN_PATH_ENV),
        env::var_os(MANAGED_VULKAN_HASH_ENV),
        env::var_os(MANAGED_VULKAN_SOURCE_ENV),
        env::var_os(MANAGED_VULKAN_VERSION_ENV),
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let [Some(path), Some(hash), Some(source), Some(version)] = values else {
        bail!("受管 Vulkan 运行时元数据不完整")
    };
    let executable_sha256 = hash.to_string_lossy().to_ascii_lowercase();
    if executable_sha256.len() != 64
        || !executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("受管 Vulkan 运行时哈希无效")
    }
    Ok(Some(ManagedRuntime {
        backend: "vulkan".into(),
        whisper_path: PathBuf::from(path),
        executable_sha256,
        source: source.to_string_lossy().into_owned(),
        version: version.to_string_lossy().into_owned(),
    }))
}

fn reconcile_managed_selection(
    selection: &RuntimeSelection,
    managed: &ManagedRuntime,
) -> Result<Option<RuntimeSelection>> {
    if selection.backend != managed.backend || selection.source != managed.source {
        return Ok(None);
    }
    if !managed.whisper_path.is_file() {
        bail!("受管 whisper.cpp 运行时不存在")
    }
    let actual = hash_file(&managed.whisper_path)?;
    if !actual.eq_ignore_ascii_case(&managed.executable_sha256) {
        bail!("受管 whisper.cpp 运行时未通过安装包清单校验")
    }
    let mut reconciled = selection.clone();
    reconciled.whisper_path = display_path(&managed.whisper_path)?;
    reconciled.executable_sha256 = actual;
    reconciled.version = managed.version.clone();
    Ok(Some(reconciled))
}

fn reconcile_selection(selection: &RuntimeSelection) -> Result<Option<RuntimeSelection>> {
    let Some(managed) = managed_runtime_from_env()? else {
        return Ok(None);
    };
    reconcile_managed_selection(selection, &managed)
}

pub fn verified_selected_whisper_path() -> Result<Option<PathBuf>> {
    let Some(selection) = load()? else {
        return Ok(None);
    };
    match verify_selection(&selection) {
        Ok(path) => Ok(Some(path)),
        Err(original_error) => {
            let Ok(Some(reconciled)) = reconcile_selection(&selection) else {
                return Err(original_error);
            };
            persist_selection(&reconciled)?;
            verify_selection(&reconciled).map(Some)
        }
    }
}

pub fn status() -> Result<serde_json::Value> {
    let selection = load()?;
    Ok(match selection {
        Some(selection) => {
            let selection = if verify_selection(&selection).is_err() {
                reconcile_selection(&selection)
                    .ok()
                    .flatten()
                    .filter(|reconciled| persist_selection(reconciled).is_ok())
                    .unwrap_or(selection)
            } else {
                selection
            };
            status_for_selection(selection)
        }
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
    persist_selection(&selection)?;
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

    #[test]
    fn migrates_a_trusted_bundled_runtime_after_upgrade() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("whisper-cli.exe");
        fs::write(&executable, b"upgraded bundled runtime").unwrap();
        let upgraded_hash = hash_file(&executable).unwrap();
        let selection = RuntimeSelection {
            backend: "vulkan".into(),
            whisper_path: executable.to_string_lossy().into_owned(),
            executable_sha256: hash_file(Path::new(file!())).unwrap(),
            source: "https://github.com/ggml-org/whisper.cpp".into(),
            version: "1.9.0-vulkan".into(),
            archive_sha256: None,
            device: Some("test gpu".into()),
            selected_at: "original selection time".into(),
        };
        let managed = ManagedRuntime {
            backend: "vulkan".into(),
            whisper_path: executable.clone(),
            executable_sha256: upgraded_hash.clone(),
            source: selection.source.clone(),
            version: "1.9.1-vulkan".into(),
        };

        let reconciled = reconcile_managed_selection(&selection, &managed)
            .unwrap()
            .unwrap();

        assert_eq!(reconciled.executable_sha256, upgraded_hash);
        assert_eq!(reconciled.whisper_path, display_path(&executable).unwrap());
        assert_eq!(reconciled.version, "1.9.1-vulkan");
        assert_eq!(reconciled.selected_at, "original selection time");
    }

    #[test]
    fn refuses_to_migrate_a_bundled_runtime_that_fails_manifest_hash() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("whisper-cli.exe");
        fs::write(&executable, b"tampered bundled runtime").unwrap();
        let selection = RuntimeSelection {
            backend: "vulkan".into(),
            whisper_path: executable.to_string_lossy().into_owned(),
            executable_sha256: "0".repeat(64),
            source: "https://github.com/ggml-org/whisper.cpp".into(),
            version: "1.9.0-vulkan".into(),
            archive_sha256: None,
            device: None,
            selected_at: "test".into(),
        };
        let managed = ManagedRuntime {
            backend: "vulkan".into(),
            whisper_path: executable,
            executable_sha256: "1".repeat(64),
            source: selection.source.clone(),
            version: "1.9.1-vulkan".into(),
        };

        let error = reconcile_managed_selection(&selection, &managed)
            .unwrap_err()
            .to_string();

        assert!(error.contains("未通过安装包清单校验"));
    }
}
