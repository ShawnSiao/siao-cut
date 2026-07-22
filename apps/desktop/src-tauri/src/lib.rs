mod app_updates;
mod diagnostics;

use diagnostics::Diagnostics;
use serde::Serialize;
use serde_json::Value;
use std::{
    env,
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
    let bundled = resource_dir.map(|root| root.join("runtime"));
    let ffmpeg = first_file(
        env::var_os("SIAOCUT_FFMPEG")
            .map(PathBuf::from)
            .into_iter()
            .chain(bundled.iter().map(|root| root.join("ffmpeg/ffmpeg.exe"))),
    );
    let ffprobe = first_file(
        env::var_os("SIAOCUT_FFPROBE")
            .map(PathBuf::from)
            .into_iter()
            .chain(bundled.iter().map(|root| root.join("ffmpeg/ffprobe.exe"))),
    );
    let whisper = first_file(
        env::var_os("SIAOCUT_WHISPER_CLI")
            .map(PathBuf::from)
            .into_iter()
            .chain(
                bundled
                    .iter()
                    .map(|root| root.join("whisper/whisper-cli.exe")),
            ),
    );
    let whisper_vulkan = first_file(
        env::var_os("SIAOCUT_WHISPER_VULKAN_CLI")
            .map(PathBuf::from)
            .into_iter()
            .chain(
                bundled
                    .iter()
                    .map(|root| root.join("whisper-vulkan/whisper-cli.exe")),
            ),
    );
    let whisper_vad_model = first_file(
        env::var_os("SIAOCUT_WHISPER_VAD_MODEL")
            .map(PathBuf::from)
            .into_iter()
            .chain(
                bundled
                    .iter()
                    .map(|root| root.join("whisper/ggml-silero-v6.2.0.bin")),
            ),
    );
    let yt_dlp = first_file(
        env::var_os("SIAOCUT_YTDLP")
            .map(PathBuf::from)
            .into_iter()
            .chain(bundled.iter().map(|root| root.join("yt-dlp/yt-dlp.exe"))),
    );
    let manifest = first_file(
        bundled
            .iter()
            .map(|root| root.join("runtime-manifest.json"))
            .chain(
                resource_dir
                    .into_iter()
                    .map(|root| root.join("notices/runtime-manifest.json")),
            ),
    );
    Ok(RuntimePaths {
        core: core_path()?,
        ffmpeg,
        ffprobe,
        whisper,
        whisper_vad_model,
        whisper_vulkan,
        yt_dlp,
        manifest,
    })
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
}

fn validate_core_args(args: &[String]) -> Result<(), String> {
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
    ];
    if args.is_empty() || !ALLOWED.contains(&args[0].as_str()) {
        return Err("桌面应用拒绝了未知 Core 命令。".to_owned());
    }
    let max_args = if args.first().is_some_and(|command| command == "glossary") {
        410
    } else {
        32
    };
    if args.len() > max_args {
        return Err("Core 命令参数过多。".to_owned());
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

async fn execute_core_async(args: Vec<String>, runtime: &RuntimePaths) -> Result<Value, String> {
    validate_core_args(&args)?;
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
    let args = match backend.as_str() {
        "cpu" => vec!["runtime".into(), "reset".into()],
        "vulkan" => {
            let whisper = runtime
                .whisper_vulkan
                .as_ref()
                .ok_or_else(|| "当前安装包未包含 Vulkan 运行时；仍可继续使用 CPU。".to_owned())?;
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
    let model = env::var_os("SIAOCUT_DEFAULT_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
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
        vad_configured: runtime.whisper_vad_model.is_some(),
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
    fn allows_public_voice_intelligence_commands() {
        assert!(validate_core_args(&["speech".into(), "audio-latest".into()]).is_ok());
        assert!(validate_core_args(&["speaker".into(), "package".into()]).is_ok());
        assert!(validate_core_args(&["transcription".into(), "providers".into()]).is_ok());
        assert!(validate_core_args(&["agent".into(), "health".into()]).is_ok());
        assert!(validate_core_args(&["glossary".into(), "show".into(), "p1".into()]).is_ok());
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
