mod app_updates;
mod diagnostics;

use diagnostics::Diagnostics;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};
use tauri::Manager;
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
    component_store: Value,
}

#[derive(Clone, Debug)]
struct RuntimePaths {
    core: PathBuf,
    manifest: Option<PathBuf>,
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

fn discover_runtime(resource_dir: Option<&Path>) -> Result<RuntimePaths, String> {
    let manifest = first_file(
        resource_dir
            .into_iter()
            .map(|root| root.join("notices/runtime-manifest.json")),
    );
    Ok(RuntimePaths {
        core: core_path()?,
        manifest,
    })
}

fn configure_command(command: &mut tokio::process::Command, runtime: &RuntimePaths) {
    let _ = runtime;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(test)]
fn configure_sync_command(command: &mut Command, runtime: &RuntimePaths) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW);
    let _ = runtime;
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
        "source",
        "auto",
        "audit",
        "transcribe",
        "transcription",
        "component-store",
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
    runtime: tauri::State<'_, RuntimePaths>,
    args: Vec<String>,
) -> Result<Value, String> {
    execute_core_async(args, &runtime).await
}

#[tauri::command]
async fn run_core_structured(
    runtime: tauri::State<'_, RuntimePaths>,
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
    execute_core_async(
        vec![
            "desktop-request".to_owned(),
            request_file.to_string_lossy().into_owned(),
        ],
        &runtime,
    )
    .await
}

#[tauri::command]
fn local_file_available(path: String) -> bool {
    !path.trim().is_empty() && Path::new(&path).is_file()
}

#[tauri::command]
async fn runtime_info(
    runtime: tauri::State<'_, RuntimePaths>,
    diagnostics: tauri::State<'_, Diagnostics>,
) -> Result<RuntimeInfo, String> {
    runtime_info_for(&runtime, &diagnostics).await
}

#[tauri::command]
async fn select_asr_backend(
    runtime: tauri::State<'_, RuntimePaths>,
    diagnostics: tauri::State<'_, Diagnostics>,
    backend: String,
) -> Result<RuntimeInfo, String> {
    let component = match backend.as_str() {
        "cpu" => "whisper-cpu",
        "vulkan" => "whisper-vulkan",
        _ => return Err("桌面应用仅支持选择 CPU 或 Vulkan 后端。".to_owned()),
    };
    let args = vec!["component-store".into(), "select".into(), component.into()];
    let response = execute_core_async(args, &runtime).await?;
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("无法切换转录后端。")
            .to_owned());
    }
    runtime_info_for(&runtime, &diagnostics).await
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
    let component_store_status = health
        .pointer("/componentStore/status")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let component_store_ready = component_store_status == "ready";
    let default_model_available = component_store_ready
        && health
            .pointer("/componentStore/installations")
            .and_then(Value::as_array)
            .is_some_and(|installations| {
                installations.iter().any(|installation| {
                    installation.get("componentId").and_then(Value::as_str) == Some("whisper-model")
                        && installation
                            .pointer("/variant/model")
                            .and_then(Value::as_str)
                            == Some("tiny")
                        && installation
                            .get("verificationStatus")
                            .and_then(Value::as_str)
                            == Some("verified")
                })
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
        yt_dlp_configured: health
            .pointer("/engines/sourceImport")
            .and_then(Value::as_str)
            == Some("configured"),
        asr_backend: health
            .pointer("/runtime/backend")
            .and_then(Value::as_str)
            .unwrap_or("cpu")
            .to_owned(),
        asr_device: health
            .pointer("/runtime/selection/device")
            .and_then(Value::as_str)
            .map(str::to_owned),
        available_asr_backends: if component_store_ready {
            vec!["cpu".into(), "vulkan".into()]
        } else {
            vec!["cpu".into()]
        },
        ffmpeg_path: None,
        whisper_path: None,
        yt_dlp_path: None,
        runtime_manifest_path: runtime
            .manifest
            .as_ref()
            .map(|path| path.display().to_string()),
        default_model_available,
        default_model_path: "component:tiny".to_owned(),
        log_directory: diagnostics
            .log_directory
            .as_ref()
            .map(|path| path.display().to_string()),
        diagnostics_available: diagnostics.initialization_error.is_none()
            && diagnostics.log_directory.is_some(),
        component_store: health.get("componentStore").cloned().unwrap_or(Value::Null),
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
    let runtime = app.state::<RuntimePaths>();
    let response =
        execute_core_async(vec!["project".into(), "show".into(), project_id], &runtime).await?;
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
    let runtime = app.state::<RuntimePaths>();
    let response =
        execute_core_async(vec!["project".into(), "show".into(), project_id], &runtime).await?;
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
            app.manage(discover_runtime(resource_dir.as_deref())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            run_core,
            run_core_structured,
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

        assert_eq!(paths.core, core_path().unwrap());
        assert_eq!(paths.manifest.as_deref(), Some(manifest.as_path()));
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
