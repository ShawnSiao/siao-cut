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
const WHISPER_RUNTIME_SOURCE_CONTRACT: &str =
    include_str!("../release/whisper-runtime-source.json");
const FORMAL_WHISPER_VERSION: &str = "1.9.1-siao.1";

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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VadTimelineCapability {
    pub verified: bool,
    pub status: String,
    pub backend: String,
    pub time_domain: Option<String>,
    pub executable_sha256: Option<String>,
    pub reason_code: Option<String>,
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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn verified_selected_runtime() -> Result<Option<RuntimeSelection>> {
    let Some(selection) = load()? else {
        return Ok(None);
    };
    match verify_selection(&selection) {
        Ok(_) => Ok(Some(selection)),
        Err(original_error) => {
            let Ok(Some(reconciled)) = reconcile_selection(&selection) else {
                return Err(original_error);
            };
            persist_selection(&reconciled)?;
            verify_selection(&reconciled)?;
            Ok(Some(reconciled))
        }
    }
}

#[allow(dead_code)]
pub fn verified_selected_whisper_path() -> Result<Option<PathBuf>> {
    verified_selected_runtime()?
        .map(|selection| verify_selection(&selection))
        .transpose()
}

fn required_string<'a>(
    value: &'a serde_json::Value,
    pointer: &str,
    label: &str,
) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("vad_metadata_invalid: 缺少或无效的 {label}"))
}

fn verify_vad_timeline_capability(
    whisper: &Path,
    expected_backend: &str,
) -> Result<(String, String)> {
    if !["cpu", "cuda", "vulkan"].contains(&expected_backend) {
        bail!("vad_backend_invalid: 未知的 ASR 后端 {expected_backend}")
    }
    if !whisper.is_file() {
        bail!(
            "vad_runtime_unresolved: whisper.cpp 运行时不是可验证文件：{}",
            whisper.display()
        )
    }
    let runtime_directory = whisper
        .parent()
        .ok_or_else(|| anyhow!("vad_runtime_unresolved: whisper.cpp 运行时没有父目录"))?;
    let metadata_path = runtime_directory.join("runtime-metadata.json");
    if !metadata_path.is_file() {
        bail!("vad_metadata_missing: 运行时没有 VAD 时间轴能力元数据")
    }
    let metadata: serde_json::Value = serde_json::from_slice(&fs::read(&metadata_path)?)
        .map_err(|_| anyhow!("vad_metadata_invalid: 无法解析 VAD 时间轴能力元数据"))?;
    let contract: serde_json::Value = serde_json::from_str(WHISPER_RUNTIME_SOURCE_CONTRACT)
        .map_err(|_| anyhow!("vad_contract_invalid: 内置运行时来源契约无效"))?;

    if metadata
        .pointer("/schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        bail!("vad_metadata_invalid: 不支持的运行时元数据版本")
    }
    let backend = required_string(&metadata, "/backend", "backend")?;
    if backend != expected_backend {
        bail!("vad_backend_mismatch: 运行时声明为 {backend}，当前后端为 {expected_backend}")
    }

    let executable_sha256 = hash_file(whisper)?;
    if !required_string(&metadata, "/executableSha256", "executableSha256")?
        .eq_ignore_ascii_case(&executable_sha256)
    {
        bail!("vad_executable_hash_mismatch: 运行时文件与能力元数据不一致")
    }

    let metadata_version = required_string(&metadata, "/version", "version")?;
    let contract_version = required_string(&contract, "/version", "version")?;
    if metadata_version != contract_version && metadata_version != FORMAL_WHISPER_VERSION {
        bail!("vad_source_contract_mismatch: 运行时 version 与正式 Whisper 身份不一致")
    }
    for (metadata_pointer, contract_pointer, label) in [
        ("/source", "/source", "source"),
        ("/sourceCommit", "/sourceCommit", "sourceCommit"),
        (
            "/upstreamTokenMappingFixCommit",
            "/upstreamTokenMappingFix/commit",
            "upstreamTokenMappingFixCommit",
        ),
        ("/patchPath", "/patch/path", "patchPath"),
        ("/patchSha256", "/patch/sha256", "patchSha256"),
    ] {
        if required_string(&metadata, metadata_pointer, label)?
            != required_string(&contract, contract_pointer, label)?
        {
            bail!("vad_source_contract_mismatch: 运行时 {label} 与内置来源契约不一致")
        }
    }
    for capability in [
        "segmentTimestampDomain",
        "tokenApiTimestampDomain",
        "cliJsonTokenTimestampDomain",
    ] {
        let pointer = format!("/sourceCapabilities/{capability}");
        if required_string(&metadata, &pointer, capability)? != "original_media" {
            bail!("vad_source_contract_mismatch: {capability} 不是原始媒体时间域")
        }
    }

    let files = metadata
        .pointer("/files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("vad_metadata_invalid: 缺少运行时文件清单"))?;
    let executable_entry = files
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("whisper-cli.exe")
        })
        .ok_or_else(|| anyhow!("vad_metadata_invalid: 文件清单缺少 whisper-cli.exe"))?;
    if !required_string(executable_entry, "/sha256", "files.whisper-cli.exe.sha256")?
        .eq_ignore_ascii_case(&executable_sha256)
    {
        bail!("vad_executable_hash_mismatch: 文件清单中的运行时哈希不一致")
    }

    let verification = metadata
        .pointer("/vadTimelineVerification")
        .ok_or_else(|| anyhow!("vad_verification_missing: 缺少 VAD 时间轴验收状态"))?;
    if required_string(verification, "/status", "vadTimelineVerification.status")? != "verified"
        || required_string(
            verification,
            "/timeDomain",
            "vadTimelineVerification.timeDomain",
        )? != "original_media"
    {
        bail!("vad_verification_not_verified: 运行时尚未通过原始媒体时间轴验收")
    }
    if required_string(
        verification,
        "/fixtureSha256",
        "vadTimelineVerification.fixtureSha256",
    )? != required_string(
        &contract,
        "/verification/fixtureSha256",
        "verification.fixtureSha256",
    )? || required_string(
        verification,
        "/verifier",
        "vadTimelineVerification.verifier",
    )? != required_string(&contract, "/verification/verifier", "verification.verifier")?
    {
        bail!("vad_verification_contract_mismatch: 运行时使用了未知的时间轴验收入口或样例")
    }

    let evidence_path = runtime_directory.join("vad-timeline-evidence.json");
    if !evidence_path.is_file() {
        bail!("vad_evidence_missing: 缺少 VAD 时间轴验收证据")
    }
    let evidence_sha256 = hash_file(&evidence_path)?;
    if !required_string(
        verification,
        "/evidenceSha256",
        "vadTimelineVerification.evidenceSha256",
    )?
    .eq_ignore_ascii_case(&evidence_sha256)
    {
        bail!("vad_evidence_hash_mismatch: VAD 时间轴验收证据已发生变化")
    }
    let evidence_entry = files
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str)
                == Some("vad-timeline-evidence.json")
        })
        .ok_or_else(|| anyhow!("vad_metadata_invalid: 文件清单缺少时间轴验收证据"))?;
    if !required_string(
        evidence_entry,
        "/sha256",
        "files.vad-timeline-evidence.sha256",
    )?
    .eq_ignore_ascii_case(&evidence_sha256)
    {
        bail!("vad_evidence_hash_mismatch: 文件清单中的时间轴证据哈希不一致")
    }

    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(&evidence_path)?)
        .map_err(|_| anyhow!("vad_evidence_invalid: 无法解析 VAD 时间轴验收证据"))?;
    let evidence_schema = evidence
        .pointer("/schemaVersion")
        .and_then(serde_json::Value::as_u64);
    let expected_schema = contract
        .pointer("/verification/schemaVersion")
        .and_then(serde_json::Value::as_u64);
    if evidence_schema != expected_schema
        || required_string(&evidence, "/status", "evidence.status")? != "passed"
        || required_string(&evidence, "/fixture/sha256", "evidence.fixture.sha256")?
            != required_string(
                &contract,
                "/verification/fixtureSha256",
                "verification.fixtureSha256",
            )?
        || required_string(&evidence, "/patched/backend", "evidence.patched.backend")?
            != expected_backend
        || required_string(
            &evidence,
            "/patched/timeDomain",
            "evidence.patched.timeDomain",
        )? != "original_media"
        || !required_string(
            &evidence,
            "/patched/executableSha256",
            "evidence.patched.executableSha256",
        )?
        .eq_ignore_ascii_case(&executable_sha256)
        || evidence
            .pointer("/patched/outsideParentTokenCount")
            .and_then(serde_json::Value::as_u64)
            != Some(0)
        || evidence
            .pointer("/segmentCount")
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|count| count < 2)
        || evidence
            .pointer("/spokenTokenCount")
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|count| count == 0)
    {
        bail!("vad_evidence_invalid: 时间轴验收证据不满足安全约束")
    }

    Ok((backend.to_owned(), executable_sha256))
}

pub fn vad_timeline_capability(whisper: &Path, expected_backend: &str) -> VadTimelineCapability {
    match verify_vad_timeline_capability(whisper, expected_backend) {
        Ok((backend, executable_sha256)) => VadTimelineCapability {
            verified: true,
            status: "verified".into(),
            backend,
            time_domain: Some("original_media".into()),
            executable_sha256: Some(executable_sha256),
            reason_code: None,
        },
        Err(error) => {
            let message = error.to_string();
            let reason_code = message
                .split_once(':')
                .map_or("vad_verification_failed", |(code, _)| code);
            VadTimelineCapability {
                verified: false,
                status: "safe_fallback".into(),
                backend: expected_backend.to_owned(),
                time_domain: None,
                executable_sha256: None,
                reason_code: Some(reason_code.into()),
            }
        }
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

    fn write_verified_vad_runtime(directory: &Path, backend: &str) -> PathBuf {
        let executable = directory.join("whisper-cli.exe");
        fs::write(&executable, b"verified whisper runtime").unwrap();
        let executable_sha256 = hash_file(&executable).unwrap();
        let contract: serde_json::Value =
            serde_json::from_str(WHISPER_RUNTIME_SOURCE_CONTRACT).unwrap();
        let fixture_sha256 = contract["verification"]["fixtureSha256"].as_str().unwrap();
        let evidence_path = directory.join("vad-timeline-evidence.json");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "status": "passed",
                "fixture": {
                    "sha256": fixture_sha256,
                    "durationSeconds": 26
                },
                "patched": {
                    "backend": backend,
                    "backendProbe": format!("{backend} backend loaded"),
                    "timeDomain": "original_media",
                    "executableSha256": executable_sha256,
                    "outsideParentTokenCount": 0
                },
                "segmentCount": 2,
                "spokenTokenCount": 44
            }))
            .unwrap(),
        )
        .unwrap();
        let evidence_sha256 = hash_file(&evidence_path).unwrap();
        let metadata = serde_json::json!({
            "schemaVersion": 1,
            "runtimeId": format!("siaocut-whisper-{backend}"),
            "version": contract["version"],
            "backend": backend,
            "source": contract["source"],
            "sourceCommit": contract["sourceCommit"],
            "upstreamTokenMappingFixCommit": contract["upstreamTokenMappingFix"]["commit"],
            "patchPath": contract["patch"]["path"],
            "patchSha256": contract["patch"]["sha256"],
            "sourceCapabilities": contract["sourceCapabilities"],
            "executableSha256": executable_sha256,
            "vadTimelineVerification": {
                "status": "verified",
                "timeDomain": "original_media",
                "evidenceSha256": evidence_sha256,
                "fixtureSha256": fixture_sha256,
                "verifier": contract["verification"]["verifier"]
            },
            "files": [
                {
                    "name": "whisper-cli.exe",
                    "sha256": executable_sha256
                },
                {
                    "name": "vad-timeline-evidence.json",
                    "sha256": evidence_sha256
                }
            ]
        });
        fs::write(
            directory.join("runtime-metadata.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
        executable
    }

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

    #[test]
    fn accepts_a_runtime_with_bound_original_timeline_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let executable = write_verified_vad_runtime(temp.path(), "cpu");

        let capability = vad_timeline_capability(&executable, "cpu");

        assert!(capability.verified);
        assert_eq!(capability.status, "verified");
        assert_eq!(capability.time_domain.as_deref(), Some("original_media"));
        assert_eq!(capability.reason_code, None);
        assert_eq!(
            capability.executable_sha256.as_deref(),
            Some(hash_file(&executable).unwrap().as_str())
        );
    }

    #[test]
    fn missing_metadata_falls_back_without_enabling_vad() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("whisper-cli.exe");
        fs::write(&executable, b"unknown runtime").unwrap();

        let capability = vad_timeline_capability(&executable, "cpu");

        assert!(!capability.verified);
        assert_eq!(capability.status, "safe_fallback");
        assert_eq!(
            capability.reason_code.as_deref(),
            Some("vad_metadata_missing")
        );
    }

    #[test]
    fn changed_executable_or_evidence_disables_vad() {
        let executable_temp = tempfile::tempdir().unwrap();
        let executable = write_verified_vad_runtime(executable_temp.path(), "cpu");
        fs::write(&executable, b"changed after verification").unwrap();
        assert_eq!(
            vad_timeline_capability(&executable, "cpu")
                .reason_code
                .as_deref(),
            Some("vad_executable_hash_mismatch")
        );

        let evidence_temp = tempfile::tempdir().unwrap();
        let executable = write_verified_vad_runtime(evidence_temp.path(), "cpu");
        fs::write(
            evidence_temp.path().join("vad-timeline-evidence.json"),
            b"changed evidence",
        )
        .unwrap();
        assert_eq!(
            vad_timeline_capability(&executable, "cpu")
                .reason_code
                .as_deref(),
            Some("vad_evidence_hash_mismatch")
        );
    }

    #[test]
    fn unknown_source_backend_or_verification_state_disables_vad() {
        for (field, value, reason) in [
            (
                "sourceCommit",
                serde_json::Value::String("unknown".into()),
                "vad_source_contract_mismatch",
            ),
            (
                "backend",
                serde_json::Value::String("vulkan".into()),
                "vad_backend_mismatch",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let executable = write_verified_vad_runtime(temp.path(), "cpu");
            let metadata_path = temp.path().join("runtime-metadata.json");
            let mut metadata: serde_json::Value =
                serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
            metadata[field] = value;
            fs::write(
                &metadata_path,
                serde_json::to_vec_pretty(&metadata).unwrap(),
            )
            .unwrap();
            assert_eq!(
                vad_timeline_capability(&executable, "cpu")
                    .reason_code
                    .as_deref(),
                Some(reason)
            );
        }

        let temp = tempfile::tempdir().unwrap();
        let executable = write_verified_vad_runtime(temp.path(), "cpu");
        let metadata_path = temp.path().join("runtime-metadata.json");
        let mut metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
        metadata["vadTimelineVerification"]["status"] = "not_run".into();
        fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
        assert_eq!(
            vad_timeline_capability(&executable, "cpu")
                .reason_code
                .as_deref(),
            Some("vad_verification_not_verified")
        );
    }
}
