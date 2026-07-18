use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::Mutex,
};
use tauri::{AppHandle, State, ipc::Channel};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATE_ENDPOINT: Option<&str> = option_env!("SIAOCUT_UPDATE_ENDPOINT");
const UPDATE_PUBKEY: Option<&str> = option_env!("SIAOCUT_UPDATER_PUBKEY");
const UPDATE_ENABLED: Option<&str> = option_env!("SIAOCUT_UPDATER_ENABLED");

pub struct PendingUpdate(Mutex<Option<Update>>);

impl Default for PendingUpdate {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePolicy {
    current_version: String,
    enabled: bool,
    automatic_check_interval_hours: u8,
    disabled_reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    version: String,
    current_version: String,
    notes: Option<String>,
    published_at: Option<String>,
    size_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
    Verifying,
}

fn release_configured() -> bool {
    !cfg!(debug_assertions)
        && UPDATE_ENABLED == Some("1")
        && UPDATE_ENDPOINT.is_some_and(|value| !value.trim().is_empty())
        && UPDATE_PUBKEY.is_some_and(|value| !value.trim().is_empty())
}

fn release_enabled() -> bool {
    release_configured()
        && std::env::current_exe()
            .ok()
            .and_then(|path| authenticode_status(&path).ok())
            .as_deref()
            == Some("Valid")
}

fn disabled_reason() -> Option<String> {
    if release_enabled() {
        None
    } else if cfg!(debug_assertions) {
        Some("开发构建不连接更新源。".to_owned())
    } else if release_configured() {
        Some("当前应用未通过 Authenticode 校验，自动更新已禁用。".to_owned())
    } else {
        Some("此构建未配置签名更新；请从正式发布页获取新版。".to_owned())
    }
}

#[tauri::command]
pub fn update_policy() -> UpdatePolicy {
    UpdatePolicy {
        current_version: env!("CARGO_PKG_VERSION").to_owned(),
        enabled: release_enabled(),
        automatic_check_interval_hours: 24,
        disabled_reason: disabled_reason(),
    }
}

fn manifest_platform(update: &Update) -> Result<&serde_json::Value, String> {
    let platforms = update
        .raw_json
        .get("platforms")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "更新清单缺少 platforms。".to_owned())?;
    platforms
        .get(&update.target)
        .or_else(|| {
            platforms.values().find(|platform| {
                platform.get("url").and_then(serde_json::Value::as_str)
                    == Some(update.download_url.as_str())
            })
        })
        .ok_or_else(|| format!("更新清单缺少当前平台 {}。", update.target))
}

fn manifest_integrity(update: &Update) -> Result<(u64, String), String> {
    let platform = manifest_platform(update)?;
    let size = platform
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "更新清单缺少安装包大小。".to_owned())?;
    let sha256 = platform
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| "更新清单缺少有效的 SHA-256。".to_owned())?;
    Ok((size, sha256.to_ascii_lowercase()))
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> Result<Option<UpdateMetadata>, String> {
    if !release_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "更新不可用。".to_owned()));
    }
    let endpoint = UPDATE_ENDPOINT
        .expect("release_enabled requires endpoint")
        .parse()
        .map_err(|error| format!("更新地址无效：{error}"))?;
    let update = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| format!("无法配置更新地址：{error}"))?
        .pubkey(UPDATE_PUBKEY.expect("release_enabled requires public key"))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("无法初始化更新器：{error}"))?
        .check()
        .await
        .map_err(|error| format!("检查更新失败：{error}"))?;

    let metadata = update
        .as_ref()
        .map(|candidate| {
            let (size_bytes, _) = manifest_integrity(candidate)?;
            Ok::<UpdateMetadata, String>(UpdateMetadata {
                version: candidate.version.clone(),
                current_version: candidate.current_version.clone(),
                notes: candidate.body.clone(),
                published_at: candidate.date.map(|value| value.to_string()),
                size_bytes,
            })
        })
        .transpose()?;
    *pending_update
        .0
        .lock()
        .map_err(|_| "更新状态锁已损坏。".to_owned())? = update;
    Ok(metadata)
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected {
        Ok(())
    } else {
        Err("更新安装包的 SHA-256 与发布清单不一致。".to_owned())
    }
}

async fn download_verified<C, D>(
    update: &Update,
    on_chunk: C,
    on_download_finish: D,
) -> Result<Vec<u8>, String>
where
    C: FnMut(usize, Option<u64>),
    D: FnOnce(),
{
    let (expected_size, expected_sha256) = manifest_integrity(update)?;
    let bytes = update
        .download(on_chunk, on_download_finish)
        .await
        .map_err(|error| format!("更新签名校验或下载失败：{error}"))?;
    if bytes.len() as u64 != expected_size {
        return Err("更新安装包大小与发布清单不一致。".to_owned());
    }
    verify_sha256(&bytes, &expected_sha256)?;
    Ok(bytes)
}

fn authenticode_status(path: &Path) -> Result<String, String> {
    let script = "Import-Module Microsoft.PowerShell.Security -ErrorAction Stop; $signature = Get-AuthenticodeSignature -LiteralPath $env:SIAOCUT_UPDATE_VERIFY_PATH; Write-Output $signature.Status";
    let windows_root = std::path::PathBuf::from(
        std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
    );
    let windows_powershell = windows_root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let windows_module_path = windows_root.join("System32/WindowsPowerShell/v1.0/Modules");
    let output = Command::new(windows_powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("SIAOCUT_UPDATE_VERIFY_PATH", path)
        .env("PSModulePath", windows_module_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("无法执行 Authenticode 校验：{error}"))?;
    let status = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() && !status.is_empty() {
        Ok(status)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn verify_authenticode(bytes: &[u8]) -> Result<(), String> {
    let mut file = tempfile::Builder::new()
        .prefix("siaocut-update-")
        .suffix(".exe")
        .tempfile()
        .map_err(|error| format!("无法创建更新校验文件：{error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .map_err(|error| format!("无法写入更新校验文件：{error}"))?;
    file.as_file()
        .sync_all()
        .map_err(|error| format!("无法同步更新校验文件：{error}"))?;
    let file_path = file.into_temp_path();
    let status = authenticode_status(&file_path)
        .map_err(|error| format!("无法执行 Authenticode 校验：{error}"))?;
    if status == "Valid" {
        Ok(())
    } else {
        Err(format!("更新安装包未通过 Authenticode 校验（{status}）。"))
    }
}

#[tauri::command]
pub async fn install_update(
    pending_update: State<'_, PendingUpdate>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    if !release_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "更新不可用。".to_owned()));
    }
    let update = pending_update
        .0
        .lock()
        .map_err(|_| "更新状态锁已损坏。".to_owned())?
        .take()
        .ok_or_else(|| "没有待安装的更新；请重新检查。".to_owned())?;
    let mut started = false;
    let bytes = download_verified(
        &update,
        |chunk_length, content_length| {
            if !started {
                let _ = on_event.send(DownloadEvent::Started { content_length });
                started = true;
            }
            let _ = on_event.send(DownloadEvent::Progress { chunk_length });
        },
        || {
            let _ = on_event.send(DownloadEvent::Finished);
        },
    )
    .await?;
    let _ = on_event.send(DownloadEvent::Verifying);
    verify_authenticode(&bytes)?;
    update
        .install(&bytes)
        .map_err(|error| format!("安装更新失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_build_disables_updater() {
        assert!(!release_enabled());
        assert!(disabled_reason().is_some());
    }

    #[test]
    fn rejects_tampered_sha256() {
        assert!(verify_sha256(b"tampered", &"0".repeat(64)).is_err());
    }

    #[test]
    fn accepts_matching_sha256() {
        let expected = format!("{:x}", Sha256::digest(b"release"));
        assert!(verify_sha256(b"release", &expected).is_ok());
    }

    #[test]
    fn rejects_unsigned_authenticode_payload() {
        assert!(verify_authenticode(b"not a signed executable").is_err());
    }

    #[test]
    fn accepts_trusted_windows_executable() {
        let located = Command::new("where.exe").arg("node.exe").output().unwrap();
        assert!(located.status.success());
        let executable = String::from_utf8(located.stdout)
            .unwrap()
            .lines()
            .next()
            .map(std::path::PathBuf::from)
            .unwrap();
        let bytes = std::fs::read(executable).unwrap();
        let result = verify_authenticode(&bytes);
        assert!(result.is_ok(), "{result:?}");
    }
}
