use anyhow::{Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, error::ErrorKind};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

mod artifacts;
mod audio_analysis;
mod auto_workflow;
mod canvas;
mod contracts;
mod cuts;
mod db;
mod export;
mod ipc;
mod media;
mod model;
mod models;
mod patches;
mod project;
mod runtime;
mod source_import;
mod speaker;
mod speech;
mod subtitle_import;
mod subtitle_quality;
mod subtitle_style;
mod subtitle_workbench;
mod tasks;
mod timeline;
mod transcription;
mod util;
mod video_export;
mod workflows;

const API_VERSION: &str = "0.1";

#[derive(Parser)]
#[command(name = "siaocut-core", about = "SiaoCut local Rust Core")]
struct Cli {
    /// Print a stable machine-readable envelope.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Health,
    Contract,
    Import {
        media: PathBuf,
        #[arg(long)]
        title: Option<String>,
    },
    #[command(subcommand)]
    Project(ProjectCommand),
    #[command(subcommand)]
    Canvas(CanvasCommand),
    #[command(subcommand)]
    Transcript(TranscriptCommand),
    #[command(subcommand)]
    Task(TaskCommand),
    #[command(subcommand)]
    Workflow(WorkflowCommand),
    #[command(subcommand)]
    Auto(AutoWorkflowCommand),
    #[command(subcommand)]
    Cut(CutCommand),
    #[command(subcommand)]
    Media(MediaCommand),
    #[command(subcommand)]
    Video(VideoCommand),
    #[command(subcommand)]
    Model(ModelCommand),
    #[command(subcommand)]
    Runtime(RuntimeCommand),
    #[command(subcommand)]
    Source(SourceCommand),
    #[command(subcommand)]
    Speech(SpeechCommand),
    #[command(subcommand)]
    Speaker(SpeakerCommand),
    #[command(subcommand)]
    Transcription(TranscriptionCommand),
    Audit {
        project_id: String,
    },
    /// Extract audio through FFmpeg and call a local whisper.cpp CLI.
    Transcribe(TranscribeArgs),
}

#[derive(Subcommand)]
enum ProjectCommand {
    List,
    Show {
        project_id: String,
    },
    DeletePreflight {
        project_id: String,
    },
    Delete {
        project_id: String,
    },
    Restore {
        project_id: String,
        version_id: String,
    },
    Undo {
        project_id: String,
    },
    Redo {
        project_id: String,
    },
    Relink {
        project_id: String,
        media: PathBuf,
    },
}

#[derive(Subcommand)]
enum SourceCommand {
    Inspect {
        url: String,
    },
    Start {
        url: String,
        #[arg(long)]
        confirm_media_id: String,
        #[arg(long, hide = true)]
        start_delay_ms: Option<u64>,
    },
    Status {
        job_id: String,
    },
    Jobs,
    Cancel {
        job_id: String,
    },
    Resume {
        job_id: String,
    },
}

#[derive(Subcommand)]
enum SpeechCommand {
    Analyze {
        project_id: String,
    },
    AudioStart {
        project_id: String,
        #[arg(long, hide = true)]
        start_delay_ms: Option<u64>,
    },
    AudioStatus {
        job_id: String,
    },
    AudioLatest {
        project_id: String,
    },
    AudioCancel {
        job_id: String,
    },
    AudioResume {
        job_id: String,
        #[arg(long, hide = true)]
        start_delay_ms: Option<u64>,
    },
}

#[derive(Subcommand)]
enum SpeakerCommand {
    Package {
        #[arg(long)]
        verify: bool,
    },
    Install,
    Jobs,
    JobStatus {
        job_id: String,
    },
    Cancel {
        job_id: String,
    },
    Resume {
        job_id: String,
    },
    Analyze {
        project_id: String,
    },
    Track {
        project_id: String,
    },
    Rename {
        project_id: String,
        speaker_id: String,
        #[arg(long)]
        name: String,
    },
    Merge {
        project_id: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        into: String,
    },
    Assign {
        project_id: String,
        segment_id: String,
        speaker_id: String,
    },
}

#[derive(Subcommand)]
enum TranscriptionCommand {
    Providers,
    Configure {
        #[arg(long)]
        endpoint: String,
        #[arg(long, default_value = transcription::DEFAULT_MODEL_ID)]
        model: String,
    },
    Health,
    Start {
        project_id: String,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long = "hotword")]
        hotwords: Vec<String>,
        #[arg(long, hide = true)]
        start_delay_ms: Option<u64>,
    },
    Status {
        job_id: String,
    },
    Latest {
        project_id: String,
    },
    Jobs {
        project_id: Option<String>,
    },
    Cancel {
        job_id: String,
    },
    Resume {
        job_id: String,
        #[arg(long, hide = true)]
        start_delay_ms: Option<u64>,
    },
    Apply {
        job_id: String,
        #[arg(long)]
        expected_version: String,
        #[arg(long)]
        confirm_replace: bool,
    },
    Discard {
        job_id: String,
    },
    Review {
        project_id: String,
        #[arg(long)]
        all: bool,
    },
    Resolve {
        item_id: String,
        #[arg(long)]
        action: String,
    },
    Export {
        project_id: String,
        #[arg(long, default_value = "json")]
        format: String,
        #[arg(short = 'o', long)]
        output: PathBuf,
        #[arg(long)]
        include_speaker_labels: bool,
        #[arg(long)]
        confirm_warnings: bool,
    },
}

#[derive(Subcommand)]
enum CanvasCommand {
    Show {
        project_id: String,
    },
    Set {
        project_id: String,
        #[arg(long)]
        aspect_ratio: String,
        #[arg(long, default_value = "contain-blur")]
        framing: String,
    },
}

#[derive(Subcommand)]
enum TranscriptCommand {
    Style {
        project_id: String,
    },
    SetStyle {
        project_id: String,
        #[arg(long)]
        preset: String,
        #[arg(long, default_value = "bottom")]
        position: String,
    },
    Add {
        project_id: String,
        #[arg(long)]
        start: f64,
        #[arg(long)]
        end: f64,
        #[arg(long)]
        text: String,
        #[arg(long)]
        confidence: Option<f64>,
    },
    Edit {
        project_id: String,
        segment_id: String,
        #[arg(long)]
        text: String,
    },
    Replace {
        project_id: String,
        #[arg(long)]
        find: String,
        #[arg(long)]
        replace: String,
    },
    Split {
        project_id: String,
        segment_id: String,
        #[arg(long)]
        text_offset: usize,
        #[arg(long = "at")]
        at_seconds: f64,
    },
    Merge {
        project_id: String,
        first_segment_id: String,
        second_segment_id: String,
        #[arg(long, default_value = " ")]
        separator: String,
    },
    Timing {
        project_id: String,
        segment_id: String,
        #[arg(long)]
        start: f64,
        #[arg(long)]
        end: f64,
    },
    Offset {
        project_id: String,
        #[arg(long = "segment", required = true)]
        segment_ids: Vec<String>,
        #[arg(long)]
        delta: f64,
    },
    InspectFile {
        project_id: String,
        input: PathBuf,
    },
    ImportFile {
        project_id: String,
        input: PathBuf,
        #[arg(long)]
        confirm_replace: bool,
        #[arg(long)]
        expected_sha256: String,
    },
    Quality {
        project_id: String,
    },
    Export(ExportArgs),
}

#[derive(Args)]
struct ExportArgs {
    project_id: String,
    #[arg(long, default_value = "srt")]
    format: String,
    #[arg(short = 'o', long)]
    output: PathBuf,
    #[arg(long)]
    lang: Option<String>,
    #[arg(long)]
    bilingual: bool,
    #[arg(long)]
    subtitle_mode: Option<String>,
    #[arg(long = "include-cuts")]
    include_cuts: bool,
}

#[derive(Subcommand)]
enum TaskCommand {
    Create {
        project_id: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, default_value = "zh-CN")]
        locale: String,
    },
    Claim {
        task_id: Option<String>,
        #[arg(long)]
        worker: String,
    },
    Submit {
        task_id: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        response: PathBuf,
    },
    Heartbeat {
        task_id: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        progress: f64,
        #[arg(long)]
        message: Option<String>,
    },
    Fail {
        task_id: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        message: String,
    },
    Retry {
        task_id: String,
    },
    Cancel {
        task_id: String,
    },
    Events {
        task_id: String,
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
    Diff {
        task_id: String,
    },
    Review {
        patch_item_id: String,
        #[arg(long)]
        action: String,
    },
    ReviewAll {
        task_id: String,
        #[arg(long)]
        action: String,
    },
}

#[derive(Subcommand)]
enum WorkflowCommand {
    Create {
        project_id: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, default_value = "zh-CN")]
        locale: String,
    },
    Status {
        workflow_id: String,
    },
    Continue {
        workflow_id: String,
    },
}

#[derive(Subcommand)]
enum AutoWorkflowCommand {
    Start(Box<AutoWorkflowStartArgs>),
    Status {
        workflow_id: String,
    },
    List,
    Cancel {
        workflow_id: String,
    },
    Continue {
        workflow_id: String,
    },
    Events {
        workflow_id: String,
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
}

#[derive(Args)]
struct AutoWorkflowStartArgs {
    #[arg(long)]
    media: Option<PathBuf>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    confirm_media_id: Option<String>,
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    language: Option<String>,
    #[arg(long, default_value = "zh-CN")]
    locale: String,
    #[arg(long)]
    translate: Option<String>,
    #[arg(short = 'o', long)]
    output: PathBuf,
    #[arg(long)]
    burn_subtitles: bool,
    #[arg(long, default_value = "source")]
    subtitle_mode: String,
    #[arg(long, hide = true)]
    start_delay_ms: Option<u64>,
}

#[derive(Subcommand)]
enum CutCommand {
    Detect {
        project_id: String,
    },
    Create {
        project_id: String,
        #[arg(long)]
        segment: String,
        #[arg(long)]
        from_word: String,
        #[arg(long)]
        to_word: String,
        #[arg(long, default_value_t = 100)]
        padding_ms: u32,
    },
    Preview {
        project_id: String,
        cut_id: String,
    },
    Apply {
        project_id: String,
        cut_id: String,
    },
    Restore {
        project_id: String,
        cut_id: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
enum MediaCommand {
    Prepare { project_id: String },
    Status { project_id: String },
    Timeline { project_id: String },
}

#[derive(Subcommand)]
enum VideoCommand {
    Export {
        project_id: String,
        #[arg(short = 'o', long)]
        output: PathBuf,
        #[arg(long)]
        burn_subtitles: bool,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        bilingual: bool,
        #[arg(long)]
        subtitle_mode: Option<String>,
        #[arg(long, hide = true)]
        start_delay_ms: Option<u64>,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
    Status {
        job_id: String,
    },
    List {
        project_id: String,
    },
    Cancel {
        job_id: String,
    },
    Retry {
        job_id: String,
    },
}

#[derive(Subcommand)]
enum ModelCommand {
    List {
        #[arg(long)]
        verify: bool,
    },
    Install {
        model_id: String,
    },
    Status {
        job_id: String,
    },
    Jobs,
    Cancel {
        job_id: String,
    },
    Verify {
        model_id: String,
    },
    Remove {
        model_id: String,
    },
}

#[derive(Subcommand)]
enum RuntimeCommand {
    Status,
    Select {
        backend: String,
        #[arg(long)]
        whisper: PathBuf,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        archive_sha256: Option<String>,
    },
    Reset,
}

#[derive(Args)]
struct TranscribeArgs {
    project_id: String,
    /// Absolute path to a ggml/gguf whisper.cpp model.
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    language: Option<String>,
}

fn envelope(payload: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("apiVersion".into(), json!(API_VERSION));
    object.insert("status".into(), json!("ok"));
    if let Value::Object(map) = payload {
        object.extend(map)
    }
    Value::Object(object)
}

fn run(cli: Cli) -> Result<Value> {
    if matches!(&cli.command, Commands::Contract) {
        return Ok(envelope(contracts::contract()));
    }
    let mut database = db::open()?;
    tasks::reconcile_expired(&mut database)?;
    models::reconcile_interrupted(&database)?;
    video_export::reconcile_interrupted(&database)?;
    source_import::reconcile_interrupted(&database)?;
    auto_workflow::reconcile_interrupted(&database)?;
    audio_analysis::reconcile_interrupted(&database)?;
    speaker::reconcile_interrupted(&database)?;
    transcription::reconcile_interrupted(&database)?;
    match cli.command {
        Commands::Contract => {
            unreachable!("contract command returns before database initialization")
        }
        Commands::Health => Ok(envelope(json!({
            "home": db::home_dir(),
            "database": db::database_path(),
            "engines": {
                "asr": if media::command_available(&media::whisper_cli_path()) { "configured" } else { "not_configured" },
                "ffmpeg": if media::command_available(&media::tool_path("SIAOCUT_FFMPEG", "ffmpeg")) { "configured" } else { "not_configured" },
                "vad": if media::whisper_vad_model_path().is_some() { "configured" } else { "not_configured" },
                "sourceImport": if source_import::configured() { "configured" } else { "not_configured" }
                ,"speaker": if speaker::package_status(false)?.installed { "configured" } else { "not_configured" }
            },
                "models": models::catalog(false)?,
                "runtime": runtime::status()?,
            "message": "Rust + SQLite Core 可用。"
        }))),
        Commands::Import { media, title } => {
            let project = project::create(&mut database, &media, title)?;
            Ok(envelope(
                json!({"projectId":project.id,"project":project,"message":"已创建项目。"}),
            ))
        }
        Commands::Source(command) => match command {
            SourceCommand::Inspect { url } => {
                let source = source_import::inspect(&url)?;
                Ok(envelope(json!({
                    "source": source,
                    "message": "已读取公开单视频信息；确认前不会下载或创建项目。"
                })))
            }
            SourceCommand::Start {
                url,
                confirm_media_id,
                start_delay_ms,
            } => {
                let job = source_import::start(&database, &url, &confirm_media_id, start_delay_ms)?;
                Ok(envelope(json!({
                    "sourceJobId": job.id,
                    "sourceJob": job,
                    "message": "已确认视频信息并开始后台下载；项目将在完整校验成功后创建。"
                })))
            }
            SourceCommand::Status { job_id } => {
                let job = source_import::load(&database, &job_id)?;
                Ok(envelope(json!({
                    "sourceJobId": job.id,
                    "sourceJob": job
                })))
            }
            SourceCommand::Jobs => Ok(envelope(json!({
                "sourceJobs": source_import::list(&database)?
            }))),
            SourceCommand::Cancel { job_id } => {
                let job = source_import::cancel(&database, &job_id)?;
                Ok(envelope(json!({
                    "sourceJobId": job.id,
                    "sourceJob": job,
                    "message": "已请求取消 URL 导入；部分下载保留到显式继续。"
                })))
            }
            SourceCommand::Resume { job_id } => {
                let job = source_import::resume(&database, &job_id)?;
                Ok(envelope(json!({
                    "sourceJobId": job.id,
                    "sourceJob": job,
                    "message": "URL 导入已显式继续，将复用已下载部分。"
                })))
            }
        },
        Commands::Speech(command) => match command {
            SpeechCommand::Analyze { project_id } => {
                let project = project::load(&database, &project_id)?;
                Ok(envelope(json!({
                    "projectId": project.id,
                    "speechInsights": project.speech_insights,
                    "message": if project.transcript.words.is_empty() {
                        "缺少词级时间，暂时无法分析语音节奏。"
                    } else {
                        "已根据本机词级时间生成语音节奏分析。"
                    }
                })))
            }
            SpeechCommand::AudioStart {
                project_id,
                start_delay_ms,
            } => {
                let job = audio_analysis::start(&database, &project_id, start_delay_ms)?;
                Ok(envelope(json!({
                    "audioAnalysisJob": job,
                    "message": "已开始本地音频质量分析；媒体不会上传。"
                })))
            }
            SpeechCommand::AudioStatus { job_id } => Ok(envelope(json!({
                "audioAnalysisJob": audio_analysis::load(&database, &job_id)?
            }))),
            SpeechCommand::AudioLatest { project_id } => Ok(envelope(json!({
                "audioAnalysisJob": audio_analysis::latest(&database, &project_id)?
            }))),
            SpeechCommand::AudioCancel { job_id } => Ok(envelope(json!({
                "audioAnalysisJob": audio_analysis::cancel(&database, &job_id)?,
                "message": "已请求取消本地音频分析。"
            }))),
            SpeechCommand::AudioResume {
                job_id,
                start_delay_ms,
            } => Ok(envelope(json!({
                "audioAnalysisJob": audio_analysis::resume(&database, &job_id, start_delay_ms)?,
                "message": "已显式继续本地音频分析。"
            }))),
        },
        Commands::Project(command) => match command {
            ProjectCommand::List => Ok(envelope(json!({"projects":project::list(&database)?}))),
            ProjectCommand::Show { project_id } => {
                let project = project::load(&database, &project_id)?;
                Ok(envelope(json!({"projectId":project.id,"project":project})))
            }
            ProjectCommand::DeletePreflight { project_id } => Ok(envelope(json!({
                "deletionPreflight": project::deletion_preflight(&database, &project_id)?
            }))),
            ProjectCommand::Delete { project_id } => {
                project::delete(&mut database, &project_id)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "message":"项目已删除；原始媒体文件未被修改。"
                })))
            }
            ProjectCommand::Restore {
                project_id,
                version_id,
            } => {
                let version = project::restore_version(&mut database, &project_id, &version_id)?;
                Ok(envelope(
                    json!({"projectId":project_id,"version":version,"message":"已恢复版本。"}),
                ))
            }
            ProjectCommand::Undo { project_id } => {
                let project = project::undo(&mut database, &project_id)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "project":project,
                    "message":"已撤销上一步项目修改。"
                })))
            }
            ProjectCommand::Redo { project_id } => {
                let project = project::redo(&mut database, &project_id)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "project":project,
                    "message":"已重做项目修改。"
                })))
            }
            ProjectCommand::Relink { project_id, media } => {
                let project = project::relink_media(&mut database, &project_id, &media)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "project":project,
                    "message":"已重新定位原片，并校验内容与项目记录一致。"
                })))
            }
        },
        Commands::Canvas(command) => match command {
            CanvasCommand::Show { project_id } => {
                let project = project::load(&database, &project_id)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "canvasSettings": project.canvas_settings
                })))
            }
            CanvasCommand::Set {
                project_id,
                aspect_ratio,
                framing,
            } => {
                let project =
                    project::set_canvas(&mut database, &project_id, &aspect_ratio, &framing)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "canvasSettings": project.canvas_settings,
                    "project": project,
                    "message": "画布设置已更新。"
                })))
            }
        },
        Commands::Transcript(command) => match command {
            TranscriptCommand::Style { project_id } => {
                let project = project::load(&database, &project_id)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "subtitleStyle": project.subtitle_style,
                    "subtitleStylePresets": subtitle_style::catalog()
                })))
            }
            TranscriptCommand::SetStyle {
                project_id,
                preset,
                position,
            } => {
                let project = subtitle_style::set(&mut database, &project_id, &preset, &position)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "subtitleStyle": project.subtitle_style,
                    "subtitleStylePresets": subtitle_style::catalog(),
                    "project": project,
                    "message": "字幕样式已更新；正文和时间未修改，可通过项目历史撤销。"
                })))
            }
            TranscriptCommand::Add {
                project_id,
                start,
                end,
                text,
                confidence,
            } => {
                let segment =
                    project::add_segment(&mut database, &project_id, start, end, text, confidence)?;
                Ok(envelope(json!({"projectId":project_id,"segment":segment})))
            }
            TranscriptCommand::Edit {
                project_id,
                segment_id,
                text,
            } => {
                let segment = project::edit_segment(&mut database, &project_id, &segment_id, text)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "segment": segment,
                    "message": "原文已更新；已有译文已标记为待更新。"
                })))
            }
            TranscriptCommand::Replace {
                project_id,
                find,
                replace,
            } => {
                let (project, changed_segments) =
                    project::replace_all(&mut database, &project_id, &find, &replace)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "project": project,
                    "changedSegments": changed_segments,
                    "message": if changed_segments == 0 { "没有找到匹配文字。" } else { "已完成批量替换；已有译文已标记为待更新。" }
                })))
            }
            TranscriptCommand::Split {
                project_id,
                segment_id,
                text_offset,
                at_seconds,
            } => {
                let result = subtitle_workbench::split(
                    &mut database,
                    &project_id,
                    &segment_id,
                    text_offset,
                    at_seconds,
                )?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "structureEdit": result,
                    "message": "字幕段已拆分；受影响的译文与剪辑证据已失效。"
                })))
            }
            TranscriptCommand::Merge {
                project_id,
                first_segment_id,
                second_segment_id,
                separator,
            } => {
                let result = subtitle_workbench::merge(
                    &mut database,
                    &project_id,
                    &first_segment_id,
                    &second_segment_id,
                    &separator,
                )?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "structureEdit": result,
                    "message": "相邻字幕段已合并；受影响的译文与剪辑证据已失效。"
                })))
            }
            TranscriptCommand::Timing {
                project_id,
                segment_id,
                start,
                end,
            } => {
                let result = subtitle_workbench::adjust_timing(
                    &mut database,
                    &project_id,
                    &segment_id,
                    start,
                    end,
                )?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "structureEdit": result,
                    "message": "字幕时间已更新；不再可信的词级、剪辑或说话人证据已失效。"
                })))
            }
            TranscriptCommand::Offset {
                project_id,
                segment_ids,
                delta,
            } => {
                let result =
                    subtitle_workbench::offset(&mut database, &project_id, &segment_ids, delta)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "structureEdit": result,
                    "message": "选中字幕与对应词级证据已批量偏移。"
                })))
            }
            TranscriptCommand::InspectFile { project_id, input } => {
                let preview = subtitle_import::inspect_file(&database, &project_id, &input)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "subtitleImportPreview": preview,
                    "message": "字幕文件已预检；尚未写入项目。"
                })))
            }
            TranscriptCommand::ImportFile {
                project_id,
                input,
                confirm_replace,
                expected_sha256,
            } => {
                let result = subtitle_import::import_file(
                    &mut database,
                    &project_id,
                    &input,
                    confirm_replace,
                    &expected_sha256,
                )?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "subtitleImport": result,
                    "project": result.project,
                    "message": "字幕已替换并创建可撤销版本；原片和既有导出文件未修改。"
                })))
            }
            TranscriptCommand::Quality { project_id } => {
                let project = project::load(&database, &project_id)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "subtitleQuality": project.subtitle_quality
                })))
            }
            TranscriptCommand::Export(arguments) => {
                let project = project::load(&database, &arguments.project_id)?;
                let subtitle_mode = export::resolve_subtitle_mode(
                    arguments.subtitle_mode.as_deref(),
                    arguments.lang.as_deref(),
                    arguments.bilingual,
                )?;
                let options = export::ExportOptions {
                    format: &arguments.format,
                    language: arguments.lang.as_deref(),
                    subtitle_mode,
                    include_cuts: arguments.include_cuts,
                };
                let report = export::audit_for_options(&project, &options);
                if report["ready"] != Value::Bool(true) {
                    bail!("导出前审计未通过，请先处理无效字幕或媒体问题")
                }
                fs::write(&arguments.output, export::render(&project, &options)?)?;
                Ok(envelope(json!({
                    "projectId": project.id,
                    "output": arguments.output,
                    "format": arguments.format,
                    "subtitleMode": subtitle_mode,
                    "audit": report
                })))
            }
        },
        Commands::Task(command) => match command {
            TaskCommand::Create {
                project_id,
                kind,
                lang,
                locale,
            } => {
                let task =
                    tasks::create_with_locale(&mut database, &project_id, &kind, lang, &locale)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "taskId":task.id,
                    "task":task,
                    "message":"任务已创建，等待 Agent 领取。"
                })))
            }
            TaskCommand::Claim { task_id, worker } => {
                match tasks::claim(&mut database, &worker, task_id.as_deref())? {
                    Some((project, task, payload)) => Ok(envelope(json!({
                        "projectId":project.id,
                        "taskId":task.id,
                        "language":task.language,
                        "instructionLocale":task.instruction_locale,
                        "contentLanguage":project.transcript.source_language,
                        "task":task,
                        "payload":payload
                    }))),
                    None => Ok(envelope(
                        json!({"task":null,"message":"当前没有待领取任务。"}),
                    )),
                }
            }
            TaskCommand::Submit {
                task_id,
                worker,
                response,
            } => {
                let response: Value = serde_json::from_str(&fs::read_to_string(response)?)?;
                let (project_id, task, patch_set) =
                    tasks::submit(&mut database, &task_id, &worker, response)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "taskId":task.id,
                    "task":task,
                    "patchSet":patch_set,
                    "message":"Agent 结果已提交，尚未修改项目。"
                })))
            }
            TaskCommand::Heartbeat {
                task_id,
                worker,
                progress,
                message,
            } => {
                let project_id = tasks::project_id(&database, &task_id)?;
                let task = tasks::heartbeat(
                    &mut database,
                    &task_id,
                    &worker,
                    progress,
                    message.as_deref(),
                )?;
                Ok(envelope(
                    json!({"projectId":project_id,"taskId":task.id,"task":task,"message":"任务进度已更新。"}),
                ))
            }
            TaskCommand::Fail {
                task_id,
                worker,
                message,
            } => {
                let project_id = tasks::project_id(&database, &task_id)?;
                let task = tasks::fail(&mut database, &task_id, &worker, &message)?;
                Ok(envelope(
                    json!({"projectId":project_id,"taskId":task.id,"task":task,"message":"任务已记录为失败，可重新排队。"}),
                ))
            }
            TaskCommand::Retry { task_id } => {
                let project_id = tasks::project_id(&database, &task_id)?;
                let task = tasks::retry(&mut database, &task_id)?;
                Ok(envelope(
                    json!({"projectId":project_id,"taskId":task.id,"task":task,"message":"任务已重新排队。"}),
                ))
            }
            TaskCommand::Cancel { task_id } => {
                let project_id = tasks::project_id(&database, &task_id)?;
                let task = tasks::cancel(&mut database, &task_id)?;
                Ok(envelope(
                    json!({"projectId":project_id,"taskId":task.id,"task":task,"message":"取消请求已记录。"}),
                ))
            }
            TaskCommand::Events { task_id, after } => {
                let project_id = tasks::project_id(&database, &task_id)?;
                let events = tasks::events(&database, &task_id, after)?;
                Ok(envelope(
                    json!({"projectId":project_id,"taskId":task_id,"events":events}),
                ))
            }
            TaskCommand::Diff { task_id } => {
                let project_id = tasks::project_id(&database, &task_id)?;
                let patch_set = patches::load_by_task(&database, &task_id)?;
                Ok(envelope(
                    json!({"projectId":project_id,"taskId":task_id,"patchSet":patch_set}),
                ))
            }
            TaskCommand::Review {
                patch_item_id,
                action,
            } => {
                let (project_id, patch_set) =
                    patches::review_item(&mut database, &patch_item_id, &action)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "patchSet":patch_set,
                    "project":project::load(&database,&project_id)?,
                    "message":if action == "apply" { "已应用此条建议。" } else { "已保留原文。" }
                })))
            }
            TaskCommand::ReviewAll { task_id, action } => {
                let (project_id, patch_set) =
                    patches::review_all(&mut database, &task_id, &action)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "taskId":task_id,
                    "patchSet":patch_set,
                    "project":project::load(&database,&project_id)?,
                    "message":if action == "apply" { "已应用全部待审建议。" } else { "已保留全部原文。" }
                })))
            }
        },
        Commands::Workflow(command) => match command {
            WorkflowCommand::Create {
                project_id,
                kind,
                lang,
                locale,
            } => {
                let workflow = workflows::create_with_locale(
                    &mut database,
                    &project_id,
                    &kind,
                    lang,
                    &locale,
                )?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "workflowId":workflow.id,
                    "taskId":workflow.task_id,
                    "workflow":workflow,
                    "message":"工作流已创建，需要 Agent 继续。"
                })))
            }
            WorkflowCommand::Status { workflow_id } => {
                let workflow = workflows::load(&database, &workflow_id)?;
                let patch_set = patches::load_by_task(&database, &workflow.task_id).ok();
                Ok(envelope(json!({
                    "workflowId":workflow.id,
                    "workflow":workflow,
                    "patchSet":patch_set
                })))
            }
            WorkflowCommand::Continue { workflow_id } => {
                let workflow = workflows::continue_workflow(&mut database, &workflow_id)?;
                Ok(envelope(json!({
                    "workflowId":workflow.id,
                    "workflow":workflow,
                    "message":"工作流状态已刷新。"
                })))
            }
        },
        Commands::Auto(command) => match command {
            AutoWorkflowCommand::Start(arguments) => {
                let AutoWorkflowStartArgs {
                    media,
                    url,
                    title,
                    confirm_media_id,
                    model,
                    language,
                    locale,
                    translate,
                    output,
                    burn_subtitles,
                    subtitle_mode,
                    start_delay_ms,
                } = *arguments;
                let input = match (media, url) {
                    (Some(media), None) => {
                        if confirm_media_id.is_some() {
                            bail!("auto_workflow_input_invalid: 本地文件不使用 --confirm-media-id")
                        }
                        auto_workflow::WorkflowInput::Local { media, title }
                    }
                    (None, Some(url)) => {
                        if title.is_some() {
                            bail!(
                                "auto_workflow_input_invalid: URL 标题来自预检结果，不使用 --title"
                            )
                        }
                        auto_workflow::WorkflowInput::Url {
                            url,
                            confirmed_media_id: confirm_media_id.ok_or_else(|| {
                                anyhow!("auto_workflow_confirmation_required: URL 输入必须提供 --confirm-media-id")
                            })?,
                        }
                    }
                    _ => bail!("auto_workflow_input_invalid: 必须且只能提供 --media 或 --url"),
                };
                let subtitle_mode = model::SubtitleMode::parse(&subtitle_mode)
                    .ok_or_else(|| anyhow!("auto_workflow_subtitle_mode_invalid: 字幕模式必须为 source、translated 或 bilingual"))?;
                let workflow = auto_workflow::start(
                    &mut database,
                    auto_workflow::StartRequest {
                        input,
                        model,
                        transcribe_language: language,
                        instruction_locale: locale,
                        translation_language: translate,
                        output,
                        burn_subtitles,
                        subtitle_mode,
                        start_delay_ms,
                    },
                )?;
                Ok(envelope(json!({
                    "workflowId": workflow.id,
                    "workflow": workflow,
                    "message": "自动工作流已启动；内容判断阶段仍会暂停等待确认。"
                })))
            }
            AutoWorkflowCommand::Status { workflow_id } => {
                let workflow = auto_workflow::load(&database, &workflow_id)?;
                Ok(envelope(json!({
                    "workflowId": workflow.id,
                    "workflow": workflow,
                    "events": auto_workflow::events(&database, &workflow_id, 0)?
                })))
            }
            AutoWorkflowCommand::List => Ok(envelope(json!({
                "workflows": auto_workflow::list(&database)?
            }))),
            AutoWorkflowCommand::Cancel { workflow_id } => {
                let workflow = auto_workflow::cancel(&mut database, &workflow_id)?;
                Ok(envelope(json!({
                    "workflowId": workflow.id,
                    "workflow": workflow,
                    "message": "自动工作流已取消。"
                })))
            }
            AutoWorkflowCommand::Continue { workflow_id } => {
                let workflow = auto_workflow::continue_workflow(&mut database, &workflow_id)?;
                Ok(envelope(json!({
                    "workflowId": workflow.id,
                    "workflow": workflow,
                    "message": "自动工作流已显式继续。"
                })))
            }
            AutoWorkflowCommand::Events { workflow_id, after } => Ok(envelope(json!({
                "workflowId": workflow_id,
                "events": auto_workflow::events(&database, &workflow_id, after)?
            }))),
        },
        Commands::Cut(command) => match command {
            CutCommand::Detect { project_id } => {
                let suggestions = cuts::detect(&mut database, &project_id)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "suggestions":suggestions,
                    "message":format!("发现 {} 条口头语、重复或重启建议，尚未删除。", suggestions.len())
                })))
            }
            CutCommand::Create {
                project_id,
                segment,
                from_word,
                to_word,
                padding_ms,
            } => {
                let cut = cuts::create_word_range(
                    &mut database,
                    &project_id,
                    &segment,
                    &from_word,
                    &to_word,
                    padding_ms,
                )?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "cut":cut,
                    "message":"词范围剪辑已创建，尚未应用。"
                })))
            }
            CutCommand::Preview { project_id, cut_id } => Ok(envelope(json!({
                "projectId":project_id,
                "preview":cuts::preview(&database,&project_id,&cut_id)?
            }))),
            CutCommand::Apply { project_id, cut_id } => {
                let cut = cuts::set_status(&mut database, &project_id, &cut_id, "applied")?;
                Ok(envelope(
                    json!({"projectId":project_id,"cut":cut,"message":"已应用可恢复软剪辑。"}),
                ))
            }
            CutCommand::Restore {
                project_id,
                cut_id,
                all,
            } => {
                if all {
                    let restored = cuts::restore_all(&mut database, &project_id)?;
                    Ok(envelope(
                        json!({"projectId":project_id,"restored":restored,"message":"已恢复原片时间线。"}),
                    ))
                } else {
                    let cut_id = cut_id.ok_or_else(|| anyhow!("请提供 cutId 或 --all"))?;
                    let restored =
                        cuts::set_status(&mut database, &project_id, &cut_id, "restored")?;
                    Ok(envelope(
                        json!({"projectId":project_id,"restored":restored,"message":"已恢复原片时间线。"}),
                    ))
                }
            }
        },
        Commands::Media(command) => match command {
            MediaCommand::Prepare { project_id } => {
                let artifacts = artifacts::prepare(&mut database, &project_id)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "artifacts":artifacts,
                    "project":project::load(&database,&project_id)?,
                    "message":"预览资源已生成；原片未修改。"
                })))
            }
            MediaCommand::Status { project_id } => Ok(envelope(json!({
                "projectId":project_id,
                "artifacts":artifacts::load(&database,&project_id)?
            }))),
            MediaCommand::Timeline { project_id } => {
                let project = project::load(&database, &project_id)?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "timeline":timeline::build(&project)
                })))
            }
        },
        Commands::Video(command) => match command {
            VideoCommand::Export {
                project_id,
                output,
                burn_subtitles,
                lang,
                bilingual,
                subtitle_mode,
                start_delay_ms,
                job_id,
            } => {
                let subtitle_mode = export::resolve_subtitle_mode(
                    subtitle_mode.as_deref(),
                    lang.as_deref(),
                    bilingual,
                )?;
                let job = video_export::create(
                    &mut database,
                    &project_id,
                    video_export::ExportRequest {
                        output: &output,
                        burn_subtitles,
                        language: lang,
                        subtitle_mode,
                        start_delay_ms,
                        job_id,
                    },
                )?;
                Ok(envelope(json!({
                    "projectId":project_id,
                    "jobId":job.id,
                    "job":job,
                    "message":"视频导出已开始，可关闭窗口后继续。"
                })))
            }
            VideoCommand::Status { job_id } => {
                let job = video_export::load(&database, &job_id)?;
                Ok(envelope(
                    json!({"projectId":job.project_id,"jobId":job.id,"job":job}),
                ))
            }
            VideoCommand::List { project_id } => Ok(envelope(json!({
                "projectId":project_id,
                "jobs":video_export::for_project(&database,&project_id)?
            }))),
            VideoCommand::Cancel { job_id } => {
                let job = video_export::cancel(&database, &job_id)?;
                Ok(envelope(json!({
                    "projectId":job.project_id,
                    "jobId":job.id,
                    "job":job,
                    "message":"已请求取消视频导出。"
                })))
            }
            VideoCommand::Retry { job_id } => {
                let job = video_export::retry(&database, &job_id)?;
                Ok(envelope(json!({
                    "projectId":job.project_id,
                    "jobId":job.id,
                    "job":job,
                    "message":"视频导出已重新开始。"
                })))
            }
        },
        Commands::Model(command) => match command {
            ModelCommand::List { verify } => Ok(envelope(json!({
                "models": models::catalog(verify)?
            }))),
            ModelCommand::Install { model_id } => {
                let job = models::create_download(&database, &model_id)?;
                Ok(envelope(json!({
                    "jobId": job.id,
                    "modelJob": job,
                    "message": "模型下载已开始；只会访问界面显示的模型来源。"
                })))
            }
            ModelCommand::Status { job_id } => {
                let job = models::load_job(&database, &job_id)?;
                Ok(envelope(json!({"jobId":job.id,"modelJob":job})))
            }
            ModelCommand::Jobs => Ok(envelope(json!({
                "modelJobs": models::list_jobs(&database)?
            }))),
            ModelCommand::Cancel { job_id } => {
                let job = models::cancel(&database, &job_id)?;
                Ok(envelope(json!({
                    "jobId":job.id,
                    "modelJob":job,
                    "message":"已请求取消模型下载；已下载部分可用于以后继续。"
                })))
            }
            ModelCommand::Verify { model_id } => Ok(envelope(json!({
                "model": models::verify(&model_id)?
            }))),
            ModelCommand::Remove { model_id } => {
                models::remove(&database, &model_id)?;
                Ok(envelope(json!({
                    "modelId":model_id,
                    "message":"模型已从本机移除；项目和原始媒体未受影响。"
                })))
            }
        },
        Commands::Speaker(command) => match command {
            SpeakerCommand::Package { verify } => Ok(envelope(json!({
                "speakerPackage": speaker::package_status(verify)?
            }))),
            SpeakerCommand::Install => {
                let job = speaker::create_install(&database)?;
                Ok(envelope(json!({
                    "speakerJob": job,
                    "message": "说话人模型包已进入本机下载队列；完成前不会启用分析。"
                })))
            }
            SpeakerCommand::Jobs => Ok(envelope(json!({
                "speakerJobs": speaker::list_jobs(&database)?
            }))),
            SpeakerCommand::JobStatus { job_id } => Ok(envelope(json!({
                "speakerJob": speaker::load_job(&database, &job_id)?
            }))),
            SpeakerCommand::Cancel { job_id } => Ok(envelope(json!({
                "speakerJob": speaker::cancel(&database, &job_id)?,
                "message": "说话人任务已取消；字幕、剪辑和原片未修改。"
            }))),
            SpeakerCommand::Resume { job_id } => Ok(envelope(json!({
                "speakerJob": speaker::resume(&database, &job_id)?,
                "message": "说话人任务已显式继续。"
            }))),
            SpeakerCommand::Analyze { project_id } => Ok(envelope(json!({
                "projectId": project_id,
                "speakerJob": speaker::create_analysis(&database, &project_id)?,
                "message": "本地说话人分析已开始；结果只进入待审阅说话人轨。"
            }))),
            SpeakerCommand::Track { project_id } => Ok(envelope(json!({
                "projectId": project_id,
                "speakerTrack": speaker::load_track(&database, &project_id)?
            }))),
            SpeakerCommand::Rename {
                project_id,
                speaker_id,
                name,
            } => Ok(envelope(json!({
                "projectId": project_id,
                "speakerTrack": speaker::rename(&mut database, &project_id, &speaker_id, &name)?,
                "message": "说话人名称已更新，可从项目历史恢复。"
            }))),
            SpeakerCommand::Merge {
                project_id,
                from,
                into,
            } => Ok(envelope(json!({
                "projectId": project_id,
                "speakerTrack": speaker::merge(&mut database, &project_id, &from, &into)?,
                "message": "说话人已合并，可从项目历史恢复。"
            }))),
            SpeakerCommand::Assign {
                project_id,
                segment_id,
                speaker_id,
            } => Ok(envelope(json!({
                "projectId": project_id,
                "speakerTrack": speaker::assign(&mut database, &project_id, &segment_id, &speaker_id)?,
                "message": "当前字幕段的说话人已重新分配，可从项目历史恢复。"
            }))),
        },
        Commands::Transcription(command) => match command {
            TranscriptionCommand::Providers => Ok(envelope(json!({
                "providers": [
                    {"id": "whisper_cpp", "role": "quick", "isDefault": true, "wordTimings": true, "integratedDiarization": false},
                    {"id": transcription::PROVIDER_ID, "role": "multispeaker_longform", "isDefault": false, "wordTimings": false, "integratedDiarization": true,
                     "defaultEndpoint": transcription::DEFAULT_ENDPOINT, "config": transcription::config(&database)?}
                ]
            }))),
            TranscriptionCommand::Configure { endpoint, model } => Ok(envelope(json!({
                "config": transcription::configure(&database, &endpoint, &model)?,
                "message": "MOSS 本机服务配置已保存；不会发送 API 密钥或连接远程地址。"
            }))),
            TranscriptionCommand::Health => Ok(envelope(
                json!({"providerHealth": transcription::health(&database)?}),
            )),
            TranscriptionCommand::Start {
                project_id,
                language,
                prompt,
                hotwords,
                start_delay_ms,
            } => Ok(envelope(json!({
                "transcriptionJob": transcription::start(&mut database, &project_id, language.as_deref(), prompt.as_deref(), &hotwords, start_delay_ms)?,
                "message": "多人长音频转写已进入后台队列；不会静默回退到快速转写。"
            }))),
            TranscriptionCommand::Status { job_id } => Ok(envelope(
                json!({"transcriptionJob": transcription::load(&database, &job_id)?}),
            )),
            TranscriptionCommand::Latest { project_id } => Ok(envelope(
                json!({"transcriptionJob": transcription::latest(&database, &project_id)?}),
            )),
            TranscriptionCommand::Jobs { project_id } => Ok(envelope(
                json!({"transcriptionJobs": transcription::list(&database, project_id.as_deref())?}),
            )),
            TranscriptionCommand::Cancel { job_id } => Ok(envelope(
                json!({"transcriptionJob": transcription::cancel(&database, &job_id)?, "message": "转写任务已取消；未完成结果不会修改项目。"}),
            )),
            TranscriptionCommand::Resume {
                job_id,
                start_delay_ms,
            } => Ok(envelope(
                json!({"transcriptionJob": transcription::resume(&mut database, &job_id, start_delay_ms)?, "message": "转写任务已显式继续。"}),
            )),
            TranscriptionCommand::Apply {
                job_id,
                expected_version,
                confirm_replace,
            } => Ok(envelope(json!({
                "transcriptionJob": transcription::apply_candidate(
                    &mut database,
                    &job_id,
                    &expected_version,
                    confirm_replace,
                )?,
                "message": "转写结果已替换字幕和说话人轨；可以通过项目历史撤销。"
            }))),
            TranscriptionCommand::Discard { job_id } => Ok(envelope(json!({
                "transcriptionJob": transcription::discard_candidate(&mut database, &job_id)?,
                "message": "待应用的转写结果已丢弃。"
            }))),
            TranscriptionCommand::Review { project_id, all } => Ok(envelope(
                json!({"reviewItems": transcription::review_items(&database, &project_id, !all)?}),
            )),
            TranscriptionCommand::Resolve { item_id, action } => Ok(envelope(
                json!({"reviewItem": transcription::resolve_review(&database, &item_id, &action)?}),
            )),
            TranscriptionCommand::Export {
                project_id,
                format,
                output,
                include_speaker_labels,
                confirm_warnings,
            } => {
                let (content, audit) = transcription::render_structured_export(
                    &database,
                    &project_id,
                    &format,
                    include_speaker_labels,
                    confirm_warnings,
                )?;
                fs::write(&output, content)?;
                Ok(envelope(json!({
                    "projectId": project_id,
                    "output": output,
                    "format": format,
                    "audit": audit,
                    "message": "结构化多人转写已导出；JSON/Markdown 保留说话人证据。"
                })))
            }
        },
        Commands::Runtime(command) => match command {
            RuntimeCommand::Status => Ok(envelope(json!({"runtime":runtime::status()?}))),
            RuntimeCommand::Select {
                backend,
                whisper,
                source,
                version,
                archive_sha256,
            } => {
                let selection =
                    runtime::select(&backend, &whisper, source, version, archive_sha256)?;
                Ok(envelope(json!({
                    "runtime":runtime::status()?,
                    "selection":selection,
                    "message":"已选择本机 whisper.cpp 运行时。"
                })))
            }
            RuntimeCommand::Reset => {
                runtime::reset()?;
                Ok(envelope(json!({
                    "runtime":runtime::status()?,
                    "message":"已恢复 CPU 基线运行时。"
                })))
            }
        },
        Commands::Audit { project_id } => {
            let project = project::load(&database, &project_id)?;
            Ok(envelope(
                json!({"projectId":project_id,"audit":export::audit(&project)}),
            ))
        }
        Commands::Transcribe(arguments) => {
            let (project, segments) = media::transcribe(
                &mut database,
                &arguments.project_id,
                &arguments.model,
                arguments.language.as_deref(),
            )?;
            Ok(envelope(json!({
                "projectId":project.id,
                "segments":segments,
                "project":project,
                "message": if segments == 0 { "未检测到清晰人声；没有生成字幕。" } else { "已完成本地转录。" }
            })))
        }
    }
}

fn ipc_error(arguments: &[String], error: &anyhow::Error) -> ipc::Response {
    let code = contracts::error_code(error);
    let message = error.to_string();
    let output = if arguments.iter().any(|argument| argument == "--json") {
        serde_json::to_string_pretty(&json!({
            "apiVersion": API_VERSION,
            "status": "error",
            "error": { "code": code, "message": message },
            "code": code,
            "message": message
        }))
        .unwrap()
    } else {
        format!("SiaoCut: {error}")
    };
    ipc::Response {
        exit_code: 1,
        output,
    }
}

pub(crate) fn execute_args(arguments: Vec<String>) -> ipc::Response {
    let mut argv = vec!["siaocut-core".to_owned()];
    argv.extend(arguments);
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
                _ => 2,
            };
            return ipc::Response {
                exit_code,
                output: error.to_string(),
            };
        }
    };
    let json_mode = cli.json;
    match run(cli) {
        Ok(value) => {
            let output = if json_mode {
                serde_json::to_string_pretty(&value).unwrap()
            } else {
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("完成。")
                    .to_owned()
            };
            ipc::Response {
                exit_code: 0,
                output,
            }
        }
        Err(error) => {
            let code = contracts::error_code(&error);
            let message = error.to_string();
            let value = json!({
                "apiVersion":API_VERSION,
                "status":"error",
                "error": { "code": code, "message": message },
                "code":code,
                "message":message
            });
            let output = if json_mode {
                serde_json::to_string_pretty(&value).unwrap()
            } else {
                format!("SiaoCut: {error}")
            };
            ipc::Response {
                exit_code: 1,
                output,
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().map(String::as_str) == Some("__model_worker") {
        let (Some(job_id), Some(model_id)) = (arguments.get(1), arguments.get(2)) else {
            eprintln!("SiaoCut model worker: missing job or model id");
            std::process::exit(2)
        };
        if let Err(error) = models::run_worker(job_id, model_id) {
            eprintln!("SiaoCut model worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__speaker_worker") {
        let Some(job_id) = arguments.get(1) else {
            eprintln!("SiaoCut speaker worker: missing job id");
            std::process::exit(2)
        };
        if let Err(error) = tokio::task::block_in_place(|| speaker::run_worker(job_id)) {
            eprintln!("SiaoCut speaker worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__export_worker") {
        let Some(job_id) = arguments.get(1) else {
            eprintln!("SiaoCut export worker: missing job id");
            std::process::exit(2)
        };
        let start_delay_ms = arguments.get(2).and_then(|value| value.parse().ok());
        if let Err(error) = video_export::run_worker(job_id, start_delay_ms) {
            eprintln!("SiaoCut export worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__source_worker") {
        let Some(job_id) = arguments.get(1) else {
            eprintln!("SiaoCut source worker: missing job id");
            std::process::exit(2)
        };
        let start_delay_ms = arguments.get(2).and_then(|value| value.parse().ok());
        if let Err(error) = source_import::run_worker(job_id, start_delay_ms) {
            eprintln!("SiaoCut source worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__auto_worker") {
        let Some(workflow_id) = arguments.get(1) else {
            eprintln!("SiaoCut auto workflow worker: missing workflow id");
            std::process::exit(2)
        };
        let start_delay_ms = arguments.get(2).and_then(|value| value.parse().ok());
        if let Err(error) = auto_workflow::run_worker(workflow_id, start_delay_ms) {
            eprintln!("SiaoCut auto workflow worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__audio_analysis_worker") {
        let Some(job_id) = arguments.get(1) else {
            eprintln!("SiaoCut audio analysis worker: missing job id");
            std::process::exit(2)
        };
        let start_delay_ms = arguments.get(2).and_then(|value| value.parse().ok());
        if let Err(error) = audio_analysis::run_worker(job_id, start_delay_ms) {
            eprintln!("SiaoCut audio analysis worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__transcription_worker") {
        let Some(job_id) = arguments.get(1) else {
            eprintln!("SiaoCut transcription worker: missing job id");
            std::process::exit(2)
        };
        let start_delay_ms = arguments.get(2).and_then(|value| value.parse().ok());
        if let Err(error) = transcription::run_worker(job_id, start_delay_ms) {
            eprintln!("SiaoCut transcription worker: {error}");
            std::process::exit(1)
        }
        return;
    }
    if arguments.first().map(String::as_str) == Some("__service") {
        if let Err(error) = ipc::serve().await {
            eprintln!("SiaoCut Core service: {error}");
            std::process::exit(1)
        }
        return;
    }

    let response = if env::var_os("SIAOCUT_DIRECT").is_some() {
        execute_args(arguments)
    } else {
        match ipc::request(arguments.clone()).await {
            Ok(response) => response,
            Err(error) => ipc_error(&arguments, &error),
        }
    };
    if response.exit_code == 0 {
        println!("{}", response.output)
    } else {
        eprintln!("{}", response.output)
    }
    if response.exit_code != 0 {
        std::process::exit(response.exit_code)
    }
}
