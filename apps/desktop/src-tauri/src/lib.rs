mod app_updates;
mod diagnostics;

use diagnostics::Diagnostics;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};
use tauri::Manager;
use tokio::io::AsyncWriteExt;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    core_path: String,
    core_api_version: String,
    ffmpeg_configured: bool,
    asr_configured: bool,
    vad_configured: bool,
    vad_timeline_verified: bool,
    vad_status: String,
    vad_reason_code: Option<String>,
    yt_dlp_configured: bool,
    asr_backend: String,
    asr_device: Option<String>,
    available_asr_backends: Vec<String>,
    ffmpeg_path: Option<String>,
    whisper_path: Option<String>,
    yt_dlp_path: Option<String>,
    runtime_manifest_path: Option<String>,
    default_model_path: String,
    default_model_available: bool,
    log_directory: Option<String>,
    diagnostics_available: bool,
}

#[derive(Clone, Debug)]
struct RuntimePaths {
    core: PathBuf,
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
    whisper: Option<PathBuf>,
    whisper_vad_model: Option<PathBuf>,
    whisper_vulkan: Option<PathBuf>,
    yt_dlp: Option<PathBuf>,
    default_model: Option<PathBuf>,
    manifest: Option<PathBuf>,
    managed_whisper_vulkan: Option<ManagedWhisperRuntime>,
}

#[derive(Clone, Debug)]
struct RuntimeState {
    resource_dir: Option<PathBuf>,
}

impl RuntimeState {
    fn paths(&self) -> Result<RuntimePaths, String> {
        discover_runtime(self.resource_dir.as_deref())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedResourceConfig {
    root: PathBuf,
    #[serde(default)]
    active_entrypoints: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
struct ManagedWhisperRuntime {
    path: PathBuf,
    executable_sha256: String,
    source: String,
    version: String,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("src-tauri must live under apps/desktop")
        .to_path_buf()
}

fn core_candidates() -> Vec<PathBuf> {
    let executable = if cfg!(windows) {
        "siaocut-core.exe"
    } else {
        "siaocut-core"
    };
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("SIAOCUT_CORE_BIN") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(current) = env::current_exe()
        && let Some(parent) = current.parent()
    {
        candidates.push(parent.join(executable));
    }
    candidates.push(
        repository_root()
            .join("target")
            .join("debug")
            .join(executable),
    );
    candidates.push(
        repository_root()
            .join("target")
            .join("release")
            .join(executable),
    );
    candidates
}

fn core_path() -> Result<PathBuf, String> {
    core_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "未找到 siaocut-core。请先在仓库根目录运行 cargo build。".to_owned())
}

fn first_file(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| path.is_file())
}

fn local_resource_config_path() -> PathBuf {
    env::var_os("SIAOCUT_RESOURCE_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| env::current_dir().unwrap_or_default())
                .join("SiaoCut")
                .join("config")
        })
        .join("local-resources.json")
}

fn managed_entrypoint(config: &ManagedResourceConfig, key: &str) -> Option<PathBuf> {
    let relative = Path::new(config.active_entrypoints.get(key)?);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(config.root.join(relative))
}

fn managed_resource_config(path: &Path) -> Option<ManagedResourceConfig> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn managed_whisper_vulkan(
    manifest: Option<&Path>,
    whisper_vulkan: Option<&Path>,
) -> Option<ManagedWhisperRuntime> {
    let path = whisper_vulkan?.to_path_buf();
    let manifest: Value = serde_json::from_slice(&fs::read(manifest?).ok()?).ok()?;
    let component = manifest
        .get("components")?
        .as_array()?
        .iter()
        .find(|component| component.get("id").and_then(Value::as_str) == Some("whisper-vulkan"))?;
    let executable_sha256 = component
        .get("executableSha256")?
        .as_str()?
        .to_ascii_lowercase();
    if executable_sha256.len() != 64
        || !executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(ManagedWhisperRuntime {
        path,
        executable_sha256,
        source: component.get("source")?.as_str()?.to_owned(),
        version: format!("{}-vulkan", component.get("version")?.as_str()?),
    })
}

fn discover_runtime_with_config(
    resource_dir: Option<&Path>,
    config_path: &Path,
) -> Result<RuntimePaths, String> {
    let managed = managed_resource_config(config_path);
    let managed_path = |key: &str| {
        managed
            .as_ref()
            .and_then(|config| managed_entrypoint(config, key))
    };
    let ffmpeg = first_file(
        env::var_os("SIAOCUT_FFMPEG")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("ffmpeg")),
    );
    let ffprobe = first_file(
        env::var_os("SIAOCUT_FFPROBE")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("ffprobe")),
    );
    let whisper = first_file(
        env::var_os("SIAOCUT_WHISPER_CLI")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("whisper")),
    );
    let whisper_vulkan = first_file(
        env::var_os("SIAOCUT_WHISPER_VULKAN_CLI")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("whisper_vulkan")),
    );
    let whisper_vad_model = first_file(
        env::var_os("SIAOCUT_WHISPER_VAD_MODEL")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("whisper_vad_model")),
    );
    let yt_dlp = first_file(
        env::var_os("SIAOCUT_YTDLP")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("yt_dlp")),
    );
    let default_model = first_file(
        env::var_os("SIAOCUT_DEFAULT_MODEL")
            .map(PathBuf::from)
            .into_iter()
            .chain(managed_path("default_model")),
    );
    let manifest = first_file(
        resource_dir
            .into_iter()
            .map(|root| root.join("notices/runtime-manifest.json")),
    );
    let managed_whisper_vulkan =
        managed_whisper_vulkan(manifest.as_deref(), whisper_vulkan.as_deref());
    Ok(RuntimePaths {
        core: core_path()?,
        ffmpeg,
        ffprobe,
        whisper,
        whisper_vad_model,
        whisper_vulkan,
        yt_dlp,
        default_model,
        manifest,
        managed_whisper_vulkan,
    })
}

fn discover_runtime(resource_dir: Option<&Path>) -> Result<RuntimePaths, String> {
    discover_runtime_with_config(resource_dir, &local_resource_config_path())
}

fn configure_command(command: &mut tokio::process::Command, runtime: &RuntimePaths) {
    command.creation_flags(CREATE_NO_WINDOW);
    if let Some(path) = &runtime.ffmpeg {
        command.env("SIAOCUT_FFMPEG", path);
    }
    if let Some(path) = &runtime.ffprobe {
        command.env("SIAOCUT_FFPROBE", path);
    }
    if let Some(path) = &runtime.whisper {
        command.env("SIAOCUT_WHISPER_CLI", path);
    }
    if let Some(path) = &runtime.whisper_vad_model {
        command.env("SIAOCUT_WHISPER_VAD_MODEL", path);
    }
    if let Some(path) = &runtime.yt_dlp {
        command.env("SIAOCUT_YTDLP", path);
    }
    if let Some(managed) = &runtime.managed_whisper_vulkan {
        command
            .env("SIAOCUT_MANAGED_WHISPER_VULKAN_CLI", &managed.path)
            .env(
                "SIAOCUT_MANAGED_WHISPER_VULKAN_SHA256",
                &managed.executable_sha256,
            )
            .env("SIAOCUT_MANAGED_WHISPER_VULKAN_SOURCE", &managed.source)
            .env("SIAOCUT_MANAGED_WHISPER_VULKAN_VERSION", &managed.version);
    }
}

#[cfg(test)]
fn configure_sync_command(command: &mut Command, runtime: &RuntimePaths) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW);
    if let Some(path) = &runtime.ffmpeg {
        command.env("SIAOCUT_FFMPEG", path);
    }
    if let Some(path) = &runtime.ffprobe {
        command.env("SIAOCUT_FFPROBE", path);
    }
    if let Some(path) = &runtime.whisper {
        command.env("SIAOCUT_WHISPER_CLI", path);
    }
    if let Some(path) = &runtime.whisper_vad_model {
        command.env("SIAOCUT_WHISPER_VAD_MODEL", path);
    }
    if let Some(path) = &runtime.yt_dlp {
        command.env("SIAOCUT_YTDLP", path);
    }
    if let Some(managed) = &runtime.managed_whisper_vulkan {
        command
            .env("SIAOCUT_MANAGED_WHISPER_VULKAN_CLI", &managed.path)
            .env(
                "SIAOCUT_MANAGED_WHISPER_VULKAN_SHA256",
                &managed.executable_sha256,
            )
            .env("SIAOCUT_MANAGED_WHISPER_VULKAN_SOURCE", &managed.source)
            .env("SIAOCUT_MANAGED_WHISPER_VULKAN_VERSION", &managed.version);
    }
}

fn validate_core_args_with_limit(args: &[String], default_max_args: usize) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "health",
        "import",
        "project",
        "glossary",
        "transcript",
        "task",
        "agent",
        "workflow",
        "cut",
        "canvas",
        "media",
        "speech",
        "speaker",
        "video",
        "model",
        "runtime",
        "resources",
        "source",
        "auto",
        "audit",
        "transcribe",
        "transcription",
        "desktop-request",
    ];
    if args.is_empty() || !ALLOWED.contains(&args[0].as_str()) {
        return Err("桌面应用拒绝了未知 Core 命令。".to_owned());
    }
    let max_args = if args.first().is_some_and(|command| command == "glossary") {
        410
    } else {
        default_max_args
    };
    if args.len() > max_args {
        return Err("Core 命令参数过多。".to_owned());
    }
    Ok(())
}

#[cfg(test)]
fn validate_core_args(args: &[String]) -> Result<(), String> {
    validate_core_args_with_limit(args, 32)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum StructuredCoreRequest {
    #[serde(rename = "transcript_offset")]
    TranscriptOffset {
        #[serde(rename = "projectId")]
        project_id: String,
        #[serde(rename = "segmentIds")]
        segment_ids: Vec<String>,
        delta: f64,
    },
    #[serde(rename = "transcription_start")]
    TranscriptionStart {
        #[serde(rename = "projectId")]
        project_id: String,
        language: String,
        prompt: Option<String>,
        hotwords: Vec<String>,
    },
}

fn validate_structured_text(label: &str, value: &str, max_chars: usize) -> Result<(), String> {
    let length = value.chars().count();
    if value.trim().is_empty() || length > max_chars || value.contains('\0') {
        return Err(format!("structured_core_request_invalid: invalid {label}"));
    }
    Ok(())
}

fn validate_structured_core_request(payload: &str) -> Result<(), String> {
    const MAX_STRUCTURED_PAYLOAD_BYTES: usize = 64 * 1024;
    if payload.len() > MAX_STRUCTURED_PAYLOAD_BYTES {
        return Err("structured_core_payload_too_large: request exceeds 64 KiB".to_owned());
    }
    let request: StructuredCoreRequest = serde_json::from_str(payload)
        .map_err(|error| format!("structured_core_payload_invalid: {error}"))?;
    match request {
        StructuredCoreRequest::TranscriptOffset {
            project_id,
            segment_ids,
            delta,
        } => {
            validate_structured_text("项目 ID", &project_id, 256)?;
            let unique = segment_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>();
            if segment_ids.is_empty()
                || segment_ids.len() > 1000
                || unique.len() != segment_ids.len()
                || !delta.is_finite()
            {
                return Err(
                    "structured_core_request_invalid: transcript offset request is invalid"
                        .to_owned(),
                );
            }
            for segment_id in &segment_ids {
                validate_structured_text("字幕段 ID", segment_id, 256)?;
            }
        }
        StructuredCoreRequest::TranscriptionStart {
            project_id,
            language,
            prompt,
            hotwords,
        } => {
            validate_structured_text("项目 ID", &project_id, 256)?;
            if !["auto", "en", "zh"].contains(&language.as_str()) || hotwords.len() > 512 {
                return Err(
                    "structured_core_request_invalid: transcription request is invalid".to_owned(),
                );
            }
            if let Some(value) = prompt.as_deref() {
                validate_structured_text("Prompt", value, 1200)?;
            }
            for hotword in &hotwords {
                validate_structured_text("热词", hotword, 200)?;
            }
        }
    }
    Ok(())
}

fn parse_core_response(stdout: &[u8], stderr: &[u8]) -> Result<Value, String> {
    let mut last_error = None;
    for candidate in [stdout, stderr] {
        if candidate.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<Value>(candidate) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
    }
    let detail = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "Core 未返回任何内容".to_owned());
    let stderr = String::from_utf8_lossy(stderr);
    Err(format!("Core 返回了无效 JSON：{detail}。{stderr}"))
}

#[cfg(test)]
fn execute_core(args: &[String]) -> Result<Value, String> {
    validate_core_args(args)?;
    let runtime = discover_runtime(None)?;
    let mut command = Command::new(&runtime.core);
    configure_sync_command(&mut command, &runtime);
    let output = command
        .arg("--json")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("无法启动 Core：{error}"))?;
    parse_core_response(&output.stdout, &output.stderr)
}

#[cfg(test)]
fn execute_core_over_named_pipe(args: &[String], home: &Path) -> Result<Value, String> {
    validate_core_args(args)?;
    let runtime = discover_runtime(None)?;
    let mut command = Command::new(&runtime.core);
    configure_sync_command(&mut command, &runtime);
    let output = command
        .env("SIAOCUT_HOME", home)
        .env("SIAOCUT_SERVICE_IDLE_MS", "100")
        .env_remove("SIAOCUT_DIRECT")
        .arg("--json")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("无法启动 Core：{error}"))?;
    parse_core_response(&output.stdout, &output.stderr)
}

async fn execute_core_async_with_limit(
    args: Vec<String>,
    runtime: &RuntimePaths,
    max_args: usize,
) -> Result<Value, String> {
    validate_core_args_with_limit(&args, max_args)?;
    let command_name = diagnostic_command_name(&args).to_owned();
    let started = Instant::now();
    log::info!("event=core_request command={command_name} status=started");
    let mut command = tokio::process::Command::new(&runtime.core);
    configure_command(&mut command, runtime);
    let output = command
        .arg("--json")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| {
            log::error!(
                "event=core_request command={command_name} status=spawn_failed duration_ms={} detail={}",
                started.elapsed().as_millis(),
                diagnostics::sanitize_detail(&error.to_string())
            );
            format!("无法启动 Core：{error}")
        })?;
    let response = parse_core_response(&output.stdout, &output.stderr).inspect_err(|error| {
        log::error!(
            "event=core_request command={command_name} status=invalid_response duration_ms={} detail={}",
            started.elapsed().as_millis(),
            diagnostics::sanitize_detail(error)
        );
    })?;
    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if status == "ok" {
        log::info!(
            "event=core_request command={command_name} status=ok duration_ms={}",
            started.elapsed().as_millis()
        );
    } else {
        let code = response
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        log::warn!(
            "event=core_request command={command_name} status=error code={code} duration_ms={}",
            started.elapsed().as_millis()
        );
    }
    Ok(response)
}

async fn execute_core_async(args: Vec<String>, runtime: &RuntimePaths) -> Result<Value, String> {
    execute_core_async_with_limit(args, runtime, 32).await
}

fn diagnostic_command_name(args: &[String]) -> &str {
    args.first().map(String::as_str).unwrap_or("unknown")
}

#[tauri::command]
async fn run_core(
    runtime: tauri::State<'_, RuntimeState>,
    args: Vec<String>,
) -> Result<Value, String> {
    let paths = runtime.paths()?;
    execute_core_async(args, &paths).await
}

#[tauri::command]
async fn run_core_structured(
    runtime: tauri::State<'_, RuntimeState>,
    payload: String,
) -> Result<Value, String> {
    validate_structured_core_request(&payload)?;
    let request_file = tempfile::Builder::new()
        .prefix("siaocut-desktop-request-")
        .suffix(".json")
        .tempfile()
        .map_err(|error| format!("structured_core_request_file_failed: {error}"))?
        .into_temp_path();
    fs::write(&request_file, payload.as_bytes())
        .map_err(|error| format!("structured_core_request_file_failed: {error}"))?;
    let paths = runtime.paths()?;
    execute_core_async(
        vec![
            "desktop-request".to_owned(),
            request_file.to_string_lossy().into_owned(),
        ],
        &paths,
    )
    .await
}

#[tauri::command]
async fn run_ai_request(
    runtime: tauri::State<'_, RuntimeState>,
    payload: String,
) -> Result<Value, String> {
    if payload.len() > 16 * 1024 || serde_json::from_str::<Value>(&payload).is_err() {
        return Err("ai_request_invalid: AI 服务请求格式无效。".to_owned());
    }
    let paths = runtime.paths()?;
    let mut command = tokio::process::Command::new(&paths.core);
    configure_command(&mut command, &paths);
    let mut child = command
        .env("SIAOCUT_DIRECT", "1")
        .arg("--json")
        .arg("ai-request")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("ai_request_start_failed: 无法启动 Core：{error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "ai_request_start_failed: 无法建立安全输入管道。".to_owned())?;
    stdin
        .write_all(payload.as_bytes())
        .await
        .map_err(|_| "ai_request_write_failed: 无法发送 AI 服务请求。".to_owned())?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| format!("ai_request_failed: Core 请求失败：{error}"))?;
    parse_core_response(&output.stdout, &output.stderr)
}

#[tauri::command]
fn local_file_available(path: String) -> bool {
    !path.trim().is_empty() && Path::new(&path).is_file()
}

#[tauri::command]
async fn runtime_info(
    runtime: tauri::State<'_, RuntimeState>,
    diagnostics: tauri::State<'_, Diagnostics>,
) -> Result<RuntimeInfo, String> {
    let paths = runtime.paths()?;
    runtime_info_for(&paths, &diagnostics).await
}

#[tauri::command]
async fn select_asr_backend(
    runtime: tauri::State<'_, RuntimeState>,
    diagnostics: tauri::State<'_, Diagnostics>,
    backend: String,
) -> Result<RuntimeInfo, String> {
    let paths = runtime.paths()?;
    let args = match backend.as_str() {
        "cpu" => vec!["runtime".into(), "reset".into()],
        "vulkan" => {
            let whisper = paths
                .whisper_vulkan
                .as_ref()
                .ok_or_else(|| "尚未配置 Vulkan 运行时；仍可继续使用 CPU。".to_owned())?;
            vec![
                "runtime".into(),
                "select".into(),
                "vulkan".into(),
                "--whisper".into(),
                whisper.display().to_string(),
                "--source".into(),
                "https://github.com/ggml-org/whisper.cpp".into(),
                "--version".into(),
                "1.9.1-vulkan".into(),
            ]
        }
        _ => return Err("桌面应用仅支持选择 CPU 或 Vulkan 后端。".to_owned()),
    };
    let response = execute_core_async(args, &paths).await?;
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("无法切换转录后端。")
            .to_owned());
    }
    runtime_info_for(&paths, &diagnostics).await
}

async fn runtime_info_for(
    runtime: &RuntimePaths,
    diagnostics: &Diagnostics,
) -> Result<RuntimeInfo, String> {
    let health = execute_core_async(vec!["health".into()], runtime).await?;
    if health.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(health
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("Core 健康检查未通过。")
            .to_owned());
    }
    let model = runtime.default_model.clone().unwrap_or_else(|| {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("SiaoCut")
            .join("models")
            .join("ggml-tiny.en.bin")
    });
    Ok(RuntimeInfo {
        core_path: runtime.core.display().to_string(),
        core_api_version: health
            .get("apiVersion")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        ffmpeg_configured: health.pointer("/engines/ffmpeg").and_then(Value::as_str)
            == Some("configured"),
        asr_configured: health.pointer("/engines/asr").and_then(Value::as_str)
            == Some("configured"),
        vad_configured: health.pointer("/engines/vadModel").and_then(Value::as_str)
            == Some("configured"),
        vad_timeline_verified: health
            .pointer("/vadTimeline/verified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        vad_status: health
            .pointer("/engines/vad")
            .and_then(Value::as_str)
            .unwrap_or("safe_fallback")
            .to_owned(),
        vad_reason_code: health
            .pointer("/vadTimeline/reasonCode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        yt_dlp_configured: runtime.yt_dlp.is_some(),
        asr_backend: health
            .pointer("/runtime/backend")
            .and_then(Value::as_str)
            .unwrap_or("cpu")
            .to_owned(),
        asr_device: health
            .pointer("/runtime/selection/device")
            .and_then(Value::as_str)
            .map(str::to_owned),
        available_asr_backends: if runtime.whisper_vulkan.is_some() {
            vec!["cpu".into(), "vulkan".into()]
        } else {
            vec!["cpu".into()]
        },
        ffmpeg_path: runtime
            .ffmpeg
            .as_ref()
            .map(|path| path.display().to_string()),
        whisper_path: runtime
            .whisper
            .as_ref()
            .map(|path| path.display().to_string()),
        yt_dlp_path: runtime
            .yt_dlp
            .as_ref()
            .map(|path| path.display().to_string()),
        runtime_manifest_path: runtime
            .manifest
            .as_ref()
            .map(|path| path.display().to_string()),
        default_model_available: model.is_file(),
        default_model_path: model.display().to_string(),
        log_directory: diagnostics
            .log_directory
            .as_ref()
            .map(|path| path.display().to_string()),
        diagnostics_available: diagnostics.initialization_error.is_none()
            && diagnostics.log_directory.is_some(),
    })
}

#[tauri::command]
fn open_log_directory(diagnostics: tauri::State<'_, Diagnostics>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let directory = diagnostics
        .log_directory
        .as_ref()
        .ok_or_else(|| "诊断日志不可用。".to_owned())?;
    Command::new("explorer.exe")
        .arg(directory)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开日志目录：{error}"))
}

#[tauri::command]
async fn authorize_media(app: tauri::AppHandle, project_id: String) -> Result<String, String> {
    let runtime = app.state::<RuntimeState>();
    let paths = runtime.paths()?;
    let response =
        execute_core_async(vec!["project".into(), "show".into(), project_id], &paths).await?;
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("无法读取项目")
            .to_owned());
    }
    let source = response
        .pointer("/project/media/sourcePath")
        .and_then(Value::as_str)
        .ok_or_else(|| "项目缺少媒体路径。".to_owned())?;
    let path = PathBuf::from(source)
        .canonicalize()
        .map_err(|error| format!("媒体文件不可用：{error}"))?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| format!("无法授权媒体预览：{error}"))?;
    Ok(path.display().to_string())
}

#[tauri::command]
async fn authorize_artifact(
    app: tauri::AppHandle,
    project_id: String,
    kind: String,
) -> Result<Option<String>, String> {
    let runtime = app.state::<RuntimeState>();
    let paths = runtime.paths()?;
    let response =
        execute_core_async(vec!["project".into(), "show".into(), project_id], &paths).await?;
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("无法读取项目")
            .to_owned());
    }
    let pointer = match kind.as_str() {
        "preview" => "/project/mediaArtifacts/proxyPath",
        "waveform" => "/project/mediaArtifacts/waveformPath",
        _ => return Err("未知预览资源类型。".to_owned()),
    };
    let Some(source) = response.pointer(pointer).and_then(Value::as_str) else {
        return Ok(None);
    };
    let path = PathBuf::from(source)
        .canonicalize()
        .map_err(|error| format!("预览资源不可用：{error}"))?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| format!("无法授权预览资源：{error}"))?;
    Ok(Some(path.display().to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let diagnostics = diagnostics::initialize();
    log::info!("event=desktop_start version={}", env!("CARGO_PKG_VERSION"));
    let builder = tauri::Builder::default()
        .manage(diagnostics)
        .manage(app_updates::PendingUpdate::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let resource_dir = app.path().resource_dir().ok();
            app.manage(RuntimeState { resource_dir });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            run_core,
            run_core_structured,
            run_ai_request,
            local_file_available,
            runtime_info,
            select_asr_backend,
            open_log_directory,
            authorize_media,
            authorize_artifact,
            app_updates::update_policy,
            app_updates::check_for_update,
            app_updates::install_update
        ]);
    builder
        .run(tauri::generate_context!())
        .expect("error while running SiaoCut");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_repository_root() {
        assert!(repository_root().join("Cargo.toml").is_file());
    }

    #[test]
    fn rejects_internal_service_command() {
        let error = execute_core(&["__service".into()]).unwrap_err();
        assert!(error.contains("未知 Core 命令"));
    }

    #[test]
    fn allows_public_auto_workflow_commands() {
        assert!(validate_core_args(&["auto".into(), "list".into()]).is_ok());
    }

    #[test]
    fn allows_public_canvas_commands() {
        assert!(validate_core_args(&["canvas".into(), "show".into(), "p1".into()]).is_ok());
        assert!(
            validate_core_args(&[
                "canvas".into(),
                "set".into(),
                "p1".into(),
                "--aspect-ratio".into(),
                "9:16".into(),
                "--framing".into(),
                "contain-blur".into(),
            ])
            .is_ok()
        );
    }

    #[test]
    fn allows_public_voice_intelligence_commands() {
        assert!(validate_core_args(&["speech".into(), "audio-latest".into()]).is_ok());
        assert!(validate_core_args(&["speaker".into(), "package".into()]).is_ok());
        assert!(validate_core_args(&["transcription".into(), "providers".into()]).is_ok());
        assert!(validate_core_args(&["agent".into(), "health".into()]).is_ok());
        assert!(validate_core_args(&["glossary".into(), "show".into(), "p1".into()]).is_ok());
        assert!(validate_core_args(&["resources".into(), "status".into()]).is_ok());
        assert!(
            validate_core_args(&["resources".into(), "install".into(), "url_import".into()])
                .is_ok()
        );
    }

    #[test]
    fn structured_offset_supports_one_thousand_segment_ids_without_argv_expansion() {
        let segment_ids = (0..1000)
            .map(|index| format!("segment-{index:028}"))
            .collect::<Vec<_>>();
        let payload = serde_json::json!({
            "kind": "transcript_offset",
            "projectId": "project-1",
            "segmentIds": segment_ids,
            "delta": 0.125
        })
        .to_string();
        assert!(payload.len() < 64 * 1024);
        validate_structured_core_request(&payload).unwrap();
        assert!(validate_core_args(&["desktop-request".into(), "request.json".into()]).is_ok());
    }

    #[test]
    fn structured_transcription_preserves_unicode_prompt_and_hotwords() {
        let payload = serde_json::json!({
            "kind": "transcription_start",
            "projectId": "project-中文",
            "language": "zh",
            "prompt": "区分「小雅」与 René",
            "hotwords": ["小雅", "SiaoCut", "café"]
        })
        .to_string();
        validate_structured_core_request(&payload).unwrap();
        let StructuredCoreRequest::TranscriptionStart {
            project_id,
            prompt,
            hotwords,
            ..
        } = serde_json::from_str(&payload).unwrap()
        else {
            panic!("expected transcription request")
        };
        assert_eq!(project_id, "project-中文");
        assert_eq!(prompt.as_deref(), Some("区分「小雅」与 René"));
        assert_eq!(hotwords, ["小雅", "SiaoCut", "café"]);
    }

    #[test]
    fn structured_command_rejects_malformed_json() {
        let error =
            validate_structured_core_request(r#"{"kind":"transcript_offset","projectId":"p1""#)
                .unwrap_err();
        assert!(error.starts_with("structured_core_payload_invalid:"));
    }

    #[test]
    fn structured_command_rejects_oversized_payload_before_parsing() {
        let error = validate_structured_core_request(&"x".repeat(64 * 1024 + 1)).unwrap_err();
        assert!(error.starts_with("structured_core_payload_too_large:"));
    }

    #[test]
    fn structured_command_rejects_more_than_one_thousand_segments() {
        let payload = serde_json::json!({
            "kind": "transcript_offset",
            "projectId": "project-1",
            "segmentIds": (0..1001).map(|index| format!("segment-{index}")).collect::<Vec<_>>(),
            "delta": 0.1
        })
        .to_string();
        let error = validate_structured_core_request(&payload).unwrap_err();
        assert!(error.starts_with("structured_core_request_invalid:"));
    }

    #[test]
    fn parses_structured_core_errors_written_to_stderr() {
        let response = parse_core_response(
            b"",
            br#"{"apiVersion":"0.1","status":"error","code":"caption-too-long","message":"caption too long"}"#,
        )
        .unwrap();

        assert_eq!(response["status"], "error");
        assert_eq!(response["code"], "caption-too-long");
    }

    #[test]
    fn still_rejects_non_json_core_output() {
        let error = parse_core_response(b"", b"plain failure").unwrap_err();
        assert!(error.contains("Core 返回了无效 JSON"));
    }

    #[test]
    fn diagnostic_label_never_contains_command_payload() {
        let args = vec![
            "transcript".to_owned(),
            "replace".to_owned(),
            "private subtitle text".to_owned(),
        ];
        assert_eq!(diagnostic_command_name(&args), "transcript");
    }

    #[test]
    fn reads_external_vulkan_integrity_from_a_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("runtime-manifest.json");
        let executable = temp.path().join("whisper-cli.exe");
        fs::write(&executable, b"runtime").unwrap();
        fs::write(
            &manifest,
            serde_json::json!({
                "components": [{
                    "id": "whisper-vulkan",
                    "version": "1.9.1",
                    "source": "https://github.com/ggml-org/whisper.cpp",
                    "executableSha256": "a".repeat(64)
                }]
            })
            .to_string(),
        )
        .unwrap();

        let managed = managed_whisper_vulkan(Some(&manifest), Some(&executable)).unwrap();

        assert_eq!(managed.path, executable);
        assert_eq!(managed.executable_sha256, "a".repeat(64));
        assert_eq!(managed.version, "1.9.1-vulkan");
    }

    #[test]
    fn refuses_to_manage_vulkan_without_a_valid_manifest_hash() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("runtime-manifest.json");
        let executable = temp.path().join("whisper-cli.exe");
        fs::write(&executable, b"runtime").unwrap();
        fs::write(
            &manifest,
            serde_json::json!({
                "components": [{
                    "id": "whisper-vulkan",
                    "version": "1.9.1",
                    "source": "https://github.com/ggml-org/whisper.cpp",
                    "executableSha256": "not-a-sha256"
                }]
            })
            .to_string(),
        )
        .unwrap();

        assert!(managed_whisper_vulkan(Some(&manifest), Some(&executable)).is_none());
    }

    #[test]
    fn ignores_runtime_files_under_resource_dir_for_app_only_packages() {
        let temp = tempfile::tempdir().unwrap();
        let bundled_ffmpeg = temp.path().join("runtime/ffmpeg/ffmpeg.exe");
        fs::create_dir_all(bundled_ffmpeg.parent().unwrap()).unwrap();
        fs::write(&bundled_ffmpeg, b"must not be discovered").unwrap();
        let manifest = temp.path().join("notices/runtime-manifest.json");
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(
            &manifest,
            br#"{"packageProfile":"app-only","components":[]}"#,
        )
        .unwrap();

        let paths = discover_runtime(Some(temp.path())).unwrap();

        assert_ne!(paths.ffmpeg.as_deref(), Some(bundled_ffmpeg.as_path()));
        assert_eq!(paths.manifest.as_deref(), Some(manifest.as_path()));
    }

    #[test]
    fn reads_only_relative_product_managed_entrypoints() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("LocalResources");
        let executable = root.join("packages/media/8.1/ffmpeg.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"runtime").unwrap();
        let config_path = temp.path().join("config/local-resources.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            serde_json::json!({
                "root": root,
                "activeEntrypoints": {
                    "ffmpeg": "packages/media/8.1/ffmpeg.exe",
                    "ffprobe": "../outside.exe"
                }
            })
            .to_string(),
        )
        .unwrap();

        let config = managed_resource_config(&config_path).unwrap();

        assert_eq!(managed_entrypoint(&config, "ffmpeg"), Some(executable));
        assert_eq!(managed_entrypoint(&config, "ffprobe"), None);
    }

    #[test]
    fn reloads_product_managed_entrypoints_after_activation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("LocalResources");
        let first = root.join("packages/media/1/ffmpeg.exe");
        let second = root.join("packages/media/2/ffmpeg.exe");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        let config_path = temp.path().join("config/local-resources.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        for (relative, expected) in [
            ("packages/media/1/ffmpeg.exe", &first),
            ("packages/media/2/ffmpeg.exe", &second),
        ] {
            fs::write(
                &config_path,
                serde_json::json!({
                    "root": root,
                    "activeEntrypoints": { "ffmpeg": relative }
                })
                .to_string(),
            )
            .unwrap();
            let config = managed_resource_config(&config_path).unwrap();
            assert_eq!(
                managed_entrypoint(&config, "ffmpeg").as_ref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn discovers_managed_cpu_transcription_without_a_vad_model() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("LocalResources");
        let ffmpeg = root.join("packages/ffmpeg-cpu/8.1/ffmpeg.exe");
        let ffprobe = root.join("packages/ffmpeg-cpu/8.1/ffprobe.exe");
        let whisper = root.join("packages/whisper-cpu-upstream/1.9.1/whisper-cli.exe");
        let model = root.join("models/transcription-model/base-test/ggml-base.bin");
        for path in [&ffmpeg, &ffprobe, &whisper, &model] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        let config_path = temp.path().join("config/local-resources.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            serde_json::json!({
                "root": root,
                "activeEntrypoints": {
                    "ffmpeg": "packages/ffmpeg-cpu/8.1/ffmpeg.exe",
                    "ffprobe": "packages/ffmpeg-cpu/8.1/ffprobe.exe",
                    "whisper": "packages/whisper-cpu-upstream/1.9.1/whisper-cli.exe",
                    "default_model": "models/transcription-model/base-test/ggml-base.bin"
                }
            })
            .to_string(),
        )
        .unwrap();

        let paths = discover_runtime_with_config(None, &config_path).unwrap();
        assert_eq!(paths.whisper.as_deref(), Some(whisper.as_path()));
        assert_eq!(paths.default_model.as_deref(), Some(model.as_path()));
        assert!(paths.whisper_vad_model.is_none());
        assert!(paths.whisper_vulkan.is_none());
    }

    #[test]
    fn reaches_core_health_contract_over_named_pipe() {
        let home = tempfile::tempdir().unwrap();
        let response = execute_core_over_named_pipe(&["health".into()], home.path()).unwrap();
        assert_eq!(response["status"], "ok");
        assert_eq!(response["apiVersion"], "0.1");
    }

    #[test]
    fn async_runtime_check_completes() {
        let paths = discover_runtime(None).unwrap();
        let diagnostics = Diagnostics {
            log_directory: Some(std::env::temp_dir()),
            initialization_error: None,
        };
        let info = tauri::async_runtime::block_on(runtime_info_for(&paths, &diagnostics)).unwrap();
        assert_eq!(info.core_api_version, "0.1");
        assert!(info.diagnostics_available);
    }
}
