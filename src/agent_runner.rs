use crate::{
    contracts, db,
    model::{AgentRun, AgentRunBatch},
    project, tasks,
    util::{KillOnCloseJob, hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const BATCH_SIZE: usize = 80;
const DEFAULT_TIMEOUT_SECONDS: u64 = 900;
const MIN_TIMEOUT_SECONDS: u64 = 30;
const MAX_TIMEOUT_SECONDS: u64 = 3600;
const RECONCILE_GRACE_SECONDS: i64 = 5;
const HEARTBEAT_SECONDS: u64 = 240;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexHealth {
    pub available: bool,
    pub authenticated: bool,
    pub version: Option<String>,
    pub auth_mode: Option<String>,
}

#[derive(Debug, Clone)]
struct RunnerConfig {
    executable: PathBuf,
    temp_root: PathBuf,
}

#[derive(Debug)]
struct InvocationSpec {
    arguments: Vec<String>,
    stdin: String,
    environment: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct EventSummary {
    thread_id: Option<String>,
    saw_turn_completed: bool,
    saw_error: bool,
}

struct EphemeralDirectory(PathBuf);

impl Drop for EphemeralDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn health() -> CodexHealth {
    let Ok(executable) = resolve_codex_cli() else {
        return CodexHealth {
            available: false,
            authenticated: false,
            version: None,
            auth_mode: None,
        };
    };
    health_with(&executable)
}

pub fn start(
    db: &mut Connection,
    task_id: &str,
    timeout_seconds: Option<u64>,
    start_delay_ms: Option<u64>,
) -> Result<AgentRun> {
    let timeout_seconds = timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS);
    validate_timeout(timeout_seconds)?;
    let executable = require_ready_codex()?;
    let cli_health = health_with(&executable);
    let (project_id, task_status, base_version_id, kind): (String, String, Option<String>, String) =
        db.query_row(
            "SELECT project_id,status,base_version_id,kind FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    if task_status != "queued" {
        bail!("agent_run_active: 只有排队中的任务可以启动本机 Agent")
    }
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE task_id=?1)",
        [task_id],
        |row| row.get::<_, bool>(0),
    )? {
        bail!("agent_run_active: 此任务已存在本机 Agent 运行记录")
    }
    let base_version_id = base_version_id
        .ok_or_else(|| anyhow!("agent_project_version_conflict: 任务缺少基线版本"))?;
    if project::current_version_id(db, &project_id)?.as_deref() != Some(base_version_id.as_str()) {
        bail!("agent_project_version_conflict: 项目版本已变化，请重新创建或重试任务")
    }
    let segment_ids = project_segment_ids(db, &project_id)?;
    if segment_ids.is_empty() {
        bail!("agent_batch_incomplete: 项目没有可处理的字幕段")
    }
    let batches = split_batches(&kind, &segment_ids);
    let run_id = new_id("ar");
    let timestamp = now();
    let tx = db.transaction()?;
    tx.execute(
        "INSERT INTO agent_runs(id,task_id,project_id,status,base_version_id,progress,current_batch,batch_count,timeout_seconds,cli_version,auth_mode,created_at,updated_at,attempt_count) VALUES(?1,?2,?3,'queued',?4,0,0,?5,?6,?7,?8,?9,?9,1)",
        params![&run_id, task_id, &project_id, &base_version_id, batches.len() as i64, timeout_seconds as i64, &cli_health.version, &cli_health.auth_mode, &timestamp],
    )?;
    insert_batches(&tx, &run_id, &batches, &timestamp)?;
    tx.commit()?;
    if let Err(error) = spawn_worker(&run_id, start_delay_ms) {
        mark_start_failed(db, &run_id, &error)?;
        return Err(error);
    }
    load(db, &run_id)
}

pub fn load(db: &Connection, run_id: &str) -> Result<AgentRun> {
    let mut run = db
        .query_row(
            "SELECT id,task_id,project_id,provider,status,base_version_id,progress,current_batch,batch_count,timeout_seconds,cli_version,auth_mode,codex_thread_id,cancel_requested_at,error_code,error_message,created_at,updated_at,started_at,completed_at,worker_pid,attempt_count FROM agent_runs WHERE id=?1",
            [run_id],
            |row| {
                Ok(AgentRun {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    project_id: row.get(2)?,
                    provider: row.get(3)?,
                    status: row.get(4)?,
                    base_version_id: row.get(5)?,
                    progress: row.get(6)?,
                    current_batch: row.get::<_, i64>(7)? as u32,
                    batch_count: row.get::<_, i64>(8)? as u32,
                    timeout_seconds: row.get::<_, i64>(9)? as u64,
                    cli_version: row.get(10)?,
                    auth_mode: row.get(11)?,
                    codex_thread_id: row.get(12)?,
                    cancel_requested_at: row.get(13)?,
                    error_code: row.get(14)?,
                    error_message: row.get(15)?,
                    created_at: row.get(16)?,
                    updated_at: row.get(17)?,
                    started_at: row.get(18)?,
                    completed_at: row.get(19)?,
                    worker_pid: row.get(20)?,
                    attempt_count: row.get::<_, i64>(21)? as u32,
                    batches: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("agent_run_not_found: Agent 运行记录不存在：{run_id}"))?;
    run.batches = db
        .prepare(
            "SELECT id,ordinal,status,segment_ids_json,codex_thread_id,error_code,error_message,started_at,completed_at,attempt_count FROM agent_run_batches WHERE run_id=?1 ORDER BY ordinal",
        )?
        .query_map([run_id], |row| {
            let raw_ids: String = row.get(3)?;
            Ok(AgentRunBatch {
                id: row.get(0)?,
                ordinal: row.get::<_, i64>(1)? as u32,
                status: row.get(2)?,
                segment_ids: serde_json::from_str(&raw_ids).unwrap_or_default(),
                codex_thread_id: row.get(4)?,
                error_code: row.get(5)?,
                error_message: row.get(6)?,
                started_at: row.get(7)?,
                completed_at: row.get(8)?,
                attempt_count: row.get::<_, i64>(9)? as u32,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(run)
}

pub fn list(db: &Connection, project_id: Option<&str>) -> Result<Vec<AgentRun>> {
    let ids = db
        .prepare(
            "SELECT id FROM agent_runs WHERE (?1 IS NULL OR project_id=?1) ORDER BY created_at DESC",
        )?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter().map(|id| load(db, &id)).collect()
}

pub fn cancel(db: &mut Connection, run_id: &str) -> Result<AgentRun> {
    let run = load(db, run_id)?;
    if !["queued", "running", "submitting"].contains(&run.status.as_str()) {
        bail!("agent_run_not_cancellable: 当前 Agent 运行不能取消")
    }
    let timestamp = now();
    db.execute(
        "UPDATE agent_runs SET cancel_requested_at=?2,updated_at=?2 WHERE id=?1",
        params![run_id, &timestamp],
    )?;
    let _ = tasks::cancel(db, &run.task_id);
    if let Some(worker_pid) = run.worker_pid
        && worker_pid != std::process::id()
        && crate::util::process_is_active(worker_pid)
    {
        let _ = crate::util::terminate_process_tree_by_id(worker_pid);
    }
    tasks::finish_runner_cancel(db, &run.task_id)?;
    db.execute(
        "UPDATE agent_runs SET status='cancelled',progress=MIN(progress,0.99),worker_pid=NULL,error_code=NULL,error_message=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
        params![run_id, &timestamp],
    )?;
    db.execute(
        "UPDATE agent_run_batches SET status='cancelled',completed_at=?2,updated_at=?2 WHERE run_id=?1 AND status IN ('queued','running')",
        params![run_id, &timestamp],
    )?;
    load(db, run_id)
}

pub fn resume(db: &mut Connection, run_id: &str, start_delay_ms: Option<u64>) -> Result<AgentRun> {
    let run = load(db, run_id)?;
    if !["cancelled", "failed", "interrupted"].contains(&run.status.as_str()) {
        bail!("agent_run_not_resumable: 当前 Agent 运行不能继续")
    }
    let executable = require_ready_codex()?;
    let cli_health = health_with(&executable);
    let task = tasks::requeue_for_runner(db, &run.task_id)?;
    let base_version_id = task
        .base_version_id
        .ok_or_else(|| anyhow!("agent_project_version_conflict: 任务缺少基线版本"))?;
    let kind: String = db.query_row(
        "SELECT kind FROM tasks WHERE id=?1",
        [&run.task_id],
        |row| row.get(0),
    )?;
    let segment_ids = project_segment_ids(db, &run.project_id)?;
    if segment_ids.is_empty() {
        bail!("agent_batch_incomplete: 项目没有可处理的字幕段")
    }
    let batches = split_batches(&kind, &segment_ids);
    let timestamp = now();
    let tx = db.transaction()?;
    tx.execute("DELETE FROM agent_run_batches WHERE run_id=?1", [run_id])?;
    insert_batches(&tx, run_id, &batches, &timestamp)?;
    tx.execute(
        "UPDATE agent_runs SET status='queued',base_version_id=?2,progress=0,current_batch=0,batch_count=?3,cli_version=?4,auth_mode=?5,codex_thread_id=NULL,cancel_requested_at=NULL,error_code=NULL,error_message=NULL,started_at=NULL,completed_at=NULL,worker_pid=NULL,attempt_count=attempt_count+1,updated_at=?6 WHERE id=?1",
        params![run_id, &base_version_id, batches.len() as i64, &cli_health.version, &cli_health.auth_mode, &timestamp],
    )?;
    tx.commit()?;
    if let Err(error) = spawn_worker(run_id, start_delay_ms) {
        mark_start_failed(db, run_id, &error)?;
        return Err(error);
    }
    load(db, run_id)
}

pub fn reconcile_interrupted(db: &mut Connection) -> Result<()> {
    let runs = db
        .prepare(
            "SELECT id,task_id,worker_pid,updated_at FROM agent_runs WHERE status IN ('queued','running','submitting')",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<u32>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (run_id, task_id, worker_pid, updated_at) in runs {
        let stale = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= RECONCILE_GRACE_SECONDS
            })
            .unwrap_or(true);
        if stale && !worker_pid.is_some_and(crate::util::process_is_active) {
            let timestamp = now();
            db.execute(
                "UPDATE agent_runs SET status='interrupted',worker_pid=NULL,error_code='agent_worker_interrupted',error_message='上次本机 Agent 进程意外中断；需要显式继续。',updated_at=?2 WHERE id=?1",
                params![&run_id, &timestamp],
            )?;
            db.execute(
                "UPDATE agent_run_batches SET status='failed',error_code='agent_worker_interrupted',error_message='本机 Agent 进程意外中断。',updated_at=?2 WHERE run_id=?1 AND status='running'",
                params![&run_id, &timestamp],
            )?;
            tasks::interrupt_runner(db, &task_id)?;
        }
    }
    Ok(())
}

pub fn run_worker(run_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let mut db = db::open()?;
    let claimed = db.execute(
        "UPDATE agent_runs SET status='running',progress=0.01,worker_pid=?2,started_at=COALESCE(started_at,?3),updated_at=?3 WHERE id=?1 AND status='queued'",
        params![run_id, std::process::id(), now()],
    )?;
    if claimed == 0 {
        return Ok(());
    }
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    match execute_run(&mut db, run_id) {
        Ok(()) => Ok(()),
        Err(error) => {
            finalize_worker_error(&mut db, run_id, &error)?;
            Err(error)
        }
    }
}

fn execute_run(db: &mut Connection, run_id: &str) -> Result<()> {
    let config = RunnerConfig {
        executable: require_ready_codex()?,
        temp_root: env::temp_dir().join("SiaoCut-Agent"),
    };
    execute_run_with_config(db, run_id, &config)
}

fn execute_run_with_config(db: &mut Connection, run_id: &str, config: &RunnerConfig) -> Result<()> {
    let run = load(db, run_id)?;
    let worker = format!("codex-{run_id}");
    let (_, task, payload) = tasks::claim(db, &worker, Some(&run.task_id))?
        .ok_or_else(|| anyhow!("agent_run_active: Agent 任务不再处于可领取状态"))?;
    if task.base_version_id.as_deref() != Some(run.base_version_id.as_str()) {
        bail!("agent_project_version_conflict: Agent 任务基线已变化")
    }
    tasks::heartbeat(
        db,
        &run.task_id,
        &worker,
        0.02,
        Some("本机 Agent 已开始处理"),
    )?;
    let batch_rows = load_batch_rows(db, run_id)?;
    let mut results = Vec::with_capacity(batch_rows.len());
    for (index, batch) in batch_rows.iter().enumerate() {
        if cancel_requested(db, run_id, &run.task_id)? {
            bail!("agent_run_cancelled: 本机 Agent 任务已取消")
        }
        let started_at = now();
        db.execute(
            "UPDATE agent_run_batches SET status='running',attempt_count=attempt_count+1,started_at=?2,completed_at=NULL,error_code=NULL,error_message=NULL,updated_at=?2 WHERE id=?1",
            params![&batch.id, &started_at],
        )?;
        db.execute(
            "UPDATE agent_runs SET current_batch=?2,updated_at=?3 WHERE id=?1",
            params![run_id, index as i64, &started_at],
        )?;
        let batch_payload = payload_for_batch(&payload, &batch.segment_ids)?;
        let schema = output_schema(
            payload
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("polish"),
            payload
                .get("baseVersionId")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let batch_result = invoke_codex(
            db,
            config,
            run_id,
            &run.task_id,
            &batch.id,
            &batch_payload,
            &schema,
            run.timeout_seconds,
        )?;
        validate_batch_result(&batch_payload, &batch.segment_ids, &batch_result.value)?;
        let completed_at = now();
        db.execute(
            "UPDATE agent_run_batches SET status='completed',result_json=?2,codex_thread_id=?3,error_code=NULL,error_message=NULL,completed_at=?4,updated_at=?4 WHERE id=?1",
            params![&batch.id, serde_json::to_string(&batch_result.value)?, &batch_result.thread_id, &completed_at],
        )?;
        let progress = 0.05 + 0.85 * ((index + 1) as f64 / batch_rows.len() as f64);
        db.execute(
            "UPDATE agent_runs SET progress=?2,current_batch=?3,codex_thread_id=COALESCE(?4,codex_thread_id),updated_at=?5 WHERE id=?1",
            params![run_id, progress, (index + 1) as i64, &batch_result.thread_id, &completed_at],
        )?;
        tasks::heartbeat(
            db,
            &run.task_id,
            &worker,
            progress,
            Some("本机 Agent 已完成一个文本批次"),
        )?;
        results.push(batch_result.value);
    }
    ensure_project_version(db, &run.project_id, &run.base_version_id)?;
    let response = aggregate_results(&payload, &results)?;
    db.execute(
        "UPDATE agent_runs SET status='submitting',progress=0.95,updated_at=?2 WHERE id=?1",
        params![run_id, now()],
    )?;
    let (_, _, patch_set) = tasks::submit(db, &run.task_id, &worker, response)?;
    if patch_set.status != "pending_review" {
        bail!("agent_output_invalid: Agent 结果未进入人工审阅")
    }
    let completed_at = now();
    db.execute(
        "UPDATE agent_runs SET status='completed',progress=1,worker_pid=NULL,error_code=NULL,error_message=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
        params![run_id, &completed_at],
    )?;
    Ok(())
}

fn ensure_project_version(
    db: &Connection,
    project_id: &str,
    expected_version_id: &str,
) -> Result<()> {
    if project::current_version_id(db, project_id)?.as_deref() != Some(expected_version_id) {
        bail!("agent_project_version_conflict: 处理期间项目版本已变化，结果未提交")
    }
    Ok(())
}

#[derive(Debug)]
struct BatchResult {
    value: Value,
    thread_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn invoke_codex(
    db: &mut Connection,
    config: &RunnerConfig,
    run_id: &str,
    task_id: &str,
    batch_id: &str,
    payload: &Value,
    schema: &Value,
    timeout_seconds: u64,
) -> Result<BatchResult> {
    let attempt: i64 = db.query_row(
        "SELECT attempt_count FROM agent_run_batches WHERE id=?1",
        [batch_id],
        |row| row.get(0),
    )?;
    let directory = config
        .temp_root
        .join(run_id)
        .join(format!("{batch_id}-{attempt}"));
    fs::create_dir_all(&directory).context("无法创建 Agent 临时目录")?;
    let _cleanup = EphemeralDirectory(directory.clone());
    fs::write(
        directory.join("schema.json"),
        serde_json::to_vec_pretty(schema)?,
    )?;
    let prompt = serde_json::to_string(&json!({
        "protocol": "siaocut-agent-v1",
        "instruction": "Process only the supplied text task. Return exactly the required JSON. Do not access local files, media, repositories, databases, credentials, or network resources.",
        "task": payload,
        "completionRule": "processedSegmentIds must contain every supplied segment ID exactly once. patches may only reference supplied segment IDs. Results are suggestions for human review and must not be applied directly."
    }))?;
    let spec = invocation_spec(&config.executable, prompt);
    let mut command = codex_command(&config.executable);
    command
        .args(&spec.arguments)
        .current_dir(&directory)
        .env_clear()
        .envs(&spec.environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| anyhow!("codex_cli_missing: 无法启动 Codex CLI"))?;
    let job = match KillOnCloseJob::assign(&child) {
        Ok(job) => job,
        Err(error) => {
            crate::util::terminate_process_tree(&mut child);
            return Err(error).context("无法建立 Codex 子进程隔离");
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(spec.stdin.as_bytes())?;
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("agent_output_invalid: Codex CLI 缺少事件输出"))?;
    let event_reader = thread::spawn(move || parse_events(BufReader::new(stdout)));
    let started = Instant::now();
    let mut last_heartbeat = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel_requested(db, run_id, task_id)? {
            crate::util::terminate_process_tree(&mut child);
            drop(job);
            let _ = event_reader.join();
            bail!("agent_run_cancelled: 本机 Agent 任务已取消")
        }
        if started.elapsed() >= Duration::from_secs(timeout_seconds) {
            crate::util::terminate_process_tree(&mut child);
            drop(job);
            let _ = event_reader.join();
            bail!("agent_run_timeout: Codex 处理超过运行时限")
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(HEARTBEAT_SECONDS) {
            let progress: f64 = db.query_row(
                "SELECT progress FROM agent_runs WHERE id=?1",
                [run_id],
                |row| row.get(0),
            )?;
            tasks::heartbeat(
                db,
                task_id,
                &format!("codex-{run_id}"),
                progress.max(0.03),
                Some("本机 Agent 仍在处理文本批次"),
            )?;
            db.execute(
                "UPDATE agent_runs SET updated_at=?2 WHERE id=?1",
                params![run_id, now()],
            )?;
            last_heartbeat = Instant::now();
        }
        thread::sleep(Duration::from_millis(100));
    };
    drop(job);
    let events = event_reader
        .join()
        .map_err(|_| anyhow!("agent_output_invalid: 无法读取 Codex 事件"))?;
    if !status.success() {
        bail!(
            "agent_output_invalid: Codex 进程未成功完成（exit={:?}）",
            status.code()
        )
    }
    if events.saw_error {
        bail!("agent_output_invalid: Codex 事件流包含错误")
    }
    let raw = fs::read_to_string(directory.join("result.json"))
        .map_err(|_| anyhow!("agent_output_invalid: Codex 未生成结构化结果"))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|_| anyhow!("agent_output_invalid: Codex 结果不是有效 JSON"))?;
    if !events.saw_turn_completed {
        bail!("agent_output_invalid: Codex 事件流未确认完成")
    }
    Ok(BatchResult {
        value,
        thread_id: events.thread_id,
    })
}

fn invocation_spec(executable: &Path, stdin: String) -> InvocationSpec {
    let arguments = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--output-schema".to_owned(),
        "schema.json".to_owned(),
        "--output-last-message".to_owned(),
        "result.json".to_owned(),
        "--sandbox".to_owned(),
        "read-only".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--ignore-user-config".to_owned(),
        "--ignore-rules".to_owned(),
        "-".to_owned(),
    ];
    InvocationSpec {
        arguments,
        stdin,
        environment: safe_environment(executable),
    }
}

fn safe_environment(executable: &Path) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for key in [
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
        "APPDATA",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "COMSPEC",
    ] {
        if let Ok(value) = env::var(key) {
            values.insert(key.to_owned(), value);
        }
    }
    let mut path_entries = Vec::new();
    if let Some(parent) = executable.parent() {
        path_entries.push(parent.to_string_lossy().into_owned());
    }
    if let Some(root) = values.get("SystemRoot") {
        path_entries.push(
            Path::new(root)
                .join("System32")
                .to_string_lossy()
                .into_owned(),
        );
    }
    if let Some(node) = find_on_path("node.exe")
        && let Some(parent) = node.parent()
    {
        path_entries.push(parent.to_string_lossy().into_owned());
    }
    values.insert("PATH".to_owned(), path_entries.join(";"));
    values.insert("PATHEXT".to_owned(), ".COM;.EXE;.BAT;.CMD".to_owned());
    values.insert("NO_COLOR".to_owned(), "1".to_owned());
    values
}

fn parse_events(reader: impl BufRead) -> EventSummary {
    let mut summary = EventSummary::default();
    for line in reader.lines().map_while(|line| line.ok()) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            summary.saw_error = true;
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                summary.thread_id = value
                    .get("thread_id")
                    .or_else(|| value.get("threadId"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("turn.completed") => summary.saw_turn_completed = true,
            Some("turn.failed" | "error") => summary.saw_error = true,
            _ => {}
        }
    }
    summary
}

fn payload_for_batch(payload: &Value, expected_ids: &[String]) -> Result<Value> {
    let expected = expected_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut payload = payload.clone();
    let object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("agent_output_invalid: Agent 任务载荷无效"))?;
    let segments = object
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("agent_batch_incomplete: Agent 任务缺少字幕段"))?
        .iter()
        .filter(|segment| {
            segment
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| expected.contains(id))
        })
        .cloned()
        .collect::<Vec<_>>();
    if segments.len() != expected_ids.len() {
        bail!("agent_batch_incomplete: Agent 批次字幕段不完整")
    }
    object.insert("segments".to_owned(), Value::Array(segments));
    object.insert(
        "runnerRequirements".to_owned(),
        json!({
            "baseVersionId": "Return the exact task baseVersionId.",
            "processedSegmentIds": "Return every supplied segment ID exactly once.",
            "scope": "Suggestions may only reference supplied segment IDs."
        }),
    );
    Ok(payload)
}

fn output_schema(kind: &str, base_version_id: &str) -> Value {
    let processed = json!({
        "type": "array",
        "items": {"type": "string"},
        "minItems": 1,
        "uniqueItems": true
    });
    let mut properties = Map::new();
    properties.insert(
        "baseVersionId".to_owned(),
        json!({"type":"string","const":base_version_id}),
    );
    properties.insert("processedSegmentIds".to_owned(), processed);
    let mut required = vec!["baseVersionId", "processedSegmentIds"];
    if kind == "summary" {
        properties.insert("summary".to_owned(), json!({"type":"string","minLength":1}));
        properties.insert("reason".to_owned(), json!({"type":"string","minLength":1}));
        properties.insert(
            "confidence".to_owned(),
            json!({"type":["number","null"],"minimum":0,"maximum":1}),
        );
        required.extend(["summary", "reason", "confidence"]);
    } else if kind == "speaker_names" {
        properties.insert("speakers".to_owned(), json!({
            "type":"array","minItems":1,"items":{
                "type":"object","additionalProperties":false,
                "properties":{
                    "speakerId":{"type":"string"},"before":{"type":"string"},
                    "after":{"type":"string","minLength":1},"reason":{"type":"string","minLength":1},
                    "confidence":{"type":["number","null"],"minimum":0,"maximum":1}
                },
                "required":["speakerId","before","after","reason","confidence"]
            }
        }));
        required.push("speakers");
    } else {
        properties.insert(
            "patches".to_owned(),
            json!({
                "type":"array","items":{
                    "type":"object","additionalProperties":false,
                    "properties":{
                        "segmentId":{"type":"string"},"before":{"type":"string"},
                        "after":{"type":"string"},"reason":{"type":"string","minLength":1},
                        "confidence":{"type":["number","null"],"minimum":0,"maximum":1}
                    },
                    "required":["segmentId","before","after","reason","confidence"]
                }
            }),
        );
        required.push("patches");
    }
    json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object",
        "additionalProperties":false,
        "properties":properties,
        "required":required
    })
}

fn validate_batch_result(payload: &Value, expected_ids: &[String], result: &Value) -> Result<()> {
    let base = payload
        .get("baseVersionId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("agent_output_invalid: Agent 任务缺少基线版本"))?;
    let result_base = result
        .get("baseVersionId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("agent_output_invalid: Agent 结果缺少基线版本"))?;
    if result_base != base {
        bail!("agent_project_version_conflict: Agent 结果版本与任务基线不一致")
    }
    let processed = result
        .get("processedSegmentIds")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("agent_batch_incomplete: Agent 结果缺少已处理字幕段"))?;
    let mut processed_ids = BTreeSet::new();
    let expected = expected_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for value in processed {
        let id = value
            .as_str()
            .ok_or_else(|| anyhow!("agent_output_invalid: 已处理字幕段 ID 无效"))?;
        if !processed_ids.insert(id) {
            bail!("agent_segment_duplicate: Agent 结果重复声明字幕段")
        }
        if !expected.contains(id) {
            bail!("agent_segment_unauthorized: Agent 结果包含批次外字幕段")
        }
    }
    if processed_ids != expected {
        bail!("agent_batch_incomplete: Agent 未确认处理批次中的全部字幕段")
    }
    let kind = payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("polish");
    if kind == "summary" {
        if result
            .get("summary")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            bail!("agent_output_invalid: 摘要结果为空")
        }
        return Ok(());
    }
    if kind == "speaker_names" {
        let speakers = result
            .get("speakers")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .ok_or_else(|| anyhow!("agent_output_invalid: 人物姓名建议为空"))?;
        let allowed = payload
            .pointer("/speakerEvidence/speakers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| value.get("id").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        for speaker in speakers {
            let id = speaker
                .get("speakerId")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("agent_output_invalid: 人物姓名建议缺少 ID"))?;
            if !seen.insert(id) {
                bail!("agent_segment_duplicate: Agent 重复提交同一人物")
            }
            if !allowed.contains(id) {
                bail!("agent_segment_unauthorized: Agent 提交了任务外人物")
            }
        }
        return Ok(());
    }
    let source_text = payload
        .get("segments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|segment| Some((segment.get("id")?.as_str()?, segment.get("text")?.as_str()?)))
        .collect::<BTreeMap<_, _>>();
    let patches = result
        .get("patches")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("agent_output_invalid: Agent 建议结构无效"))?;
    let mut seen = BTreeSet::new();
    for patch in patches {
        let id = patch
            .get("segmentId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("agent_output_invalid: Agent 建议缺少字幕段 ID"))?;
        if !seen.insert(id) {
            bail!("agent_segment_duplicate: Agent 重复提交同一字幕段")
        }
        if !expected.contains(id) {
            bail!("agent_segment_unauthorized: Agent 建议包含批次外字幕段")
        }
        if patch.get("before").and_then(Value::as_str) != source_text.get(id).copied() {
            bail!("patch_before_mismatch: Agent 建议原文与任务基线不一致")
        }
    }
    Ok(())
}

fn aggregate_results(payload: &Value, results: &[Value]) -> Result<Value> {
    let base = payload
        .get("baseVersionId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("agent_output_invalid: Agent 任务缺少基线版本"))?;
    let kind = payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("polish");
    if kind == "summary" {
        return results
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("agent_batch_incomplete: Agent 没有返回摘要结果"));
    }
    let key = if kind == "speaker_names" {
        "speakers"
    } else {
        "patches"
    };
    let mut items = Vec::new();
    for result in results {
        items.extend(
            result
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("agent_output_invalid: Agent 批次结果结构无效"))?
                .iter()
                .cloned(),
        );
    }
    Ok(json!({"baseVersionId":base,key:items}))
}

fn resolve_codex_cli() -> Result<PathBuf> {
    if let Some(path) = env::var_os("SIAOCUT_CODEX_CLI").map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        bail!("codex_cli_missing: 配置的 Codex CLI 不存在")
    }
    find_all_on_path("codex")
        .into_iter()
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| {
                    ["exe", "cmd", "bat", "ps1"]
                        .iter()
                        .any(|allowed| extension.eq_ignore_ascii_case(allowed))
                })
        })
        .find(|path| path.is_file())
        .ok_or_else(|| anyhow!("codex_cli_missing: 未找到 Codex CLI"))
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    find_all_on_path(name).into_iter().next()
}

fn find_all_on_path(name: &str) -> Vec<PathBuf> {
    let Ok(output) = hidden_command("where.exe").arg(name).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn require_ready_codex() -> Result<PathBuf> {
    let executable = resolve_codex_cli()?;
    let health = health_with(&executable);
    if !health.available {
        bail!("codex_cli_missing: Codex CLI 无法运行")
    }
    if !health.authenticated {
        bail!("codex_not_logged_in: Codex CLI 尚未登录")
    }
    Ok(executable)
}

fn health_with(executable: &Path) -> CodexHealth {
    let version = run_health_command(executable, &["--version"])
        .ok()
        .and_then(|output| sanitize_line(&output));
    let login = run_health_command(executable, &["login", "status"]).ok();
    let auth_mode = login.as_deref().and_then(auth_mode);
    CodexHealth {
        available: version.is_some(),
        authenticated: auth_mode.is_some(),
        version,
        auth_mode,
    }
}

fn run_health_command(executable: &Path, arguments: &[&str]) -> Result<String> {
    let mut command = codex_command(executable);
    let output = command
        .args(arguments)
        .env_clear()
        .envs(safe_environment(executable))
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        bail!("Codex CLI command failed")
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

fn codex_command(executable: &Path) -> Command {
    let extension = executable
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("ps1") {
        let mut command = hidden_command("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
        command.arg(executable);
        command
    } else if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
        let mut command = hidden_command("cmd.exe");
        command.args(["/D", "/S", "/C"]);
        command.arg(executable);
        command
    } else {
        hidden_command(executable)
    }
}

fn sanitize_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(96).collect())
}

fn auth_mode(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("logged in using chatgpt") {
        Some("chatgpt".to_owned())
    } else if lower.contains("logged in") && lower.contains("api key") {
        Some("api_key".to_owned())
    } else {
        None
    }
}

fn validate_timeout(timeout_seconds: u64) -> Result<()> {
    if !(MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(&timeout_seconds) {
        bail!(
            "invalid_request: Agent 超时必须在 {MIN_TIMEOUT_SECONDS} 到 {MAX_TIMEOUT_SECONDS} 秒之间"
        )
    }
    Ok(())
}

fn project_segment_ids(db: &Connection, project_id: &str) -> Result<Vec<String>> {
    Ok(db
        .prepare("SELECT id FROM segments WHERE project_id=?1 ORDER BY start_seconds,id")?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn split_batches(kind: &str, segment_ids: &[String]) -> Vec<Vec<String>> {
    if matches!(kind, "summary" | "speaker_names") {
        return vec![segment_ids.to_vec()];
    }
    segment_ids
        .chunks(BATCH_SIZE)
        .map(<[String]>::to_vec)
        .collect()
}

fn insert_batches(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    batches: &[Vec<String>],
    timestamp: &str,
) -> Result<()> {
    for (ordinal, segment_ids) in batches.iter().enumerate() {
        tx.execute(
            "INSERT INTO agent_run_batches(id,run_id,ordinal,status,segment_ids_json,created_at,updated_at) VALUES(?1,?2,?3,'queued',?4,?5,?5)",
            params![new_id("arb"), run_id, ordinal as i64, serde_json::to_string(segment_ids)?, timestamp],
        )?;
    }
    Ok(())
}

fn load_batch_rows(db: &Connection, run_id: &str) -> Result<Vec<AgentRunBatch>> {
    Ok(load(db, run_id)?.batches)
}

fn spawn_worker(run_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let delay = start_delay_ms.map(|value| value.to_string());
    let mut arguments = vec!["__agent_worker", run_id];
    if let Some(delay) = delay.as_deref() {
        arguments.push(delay);
    }
    crate::util::spawn_detached_current(&arguments).context("无法启动本机 Agent Worker")
}

fn mark_start_failed(db: &mut Connection, run_id: &str, _error: &anyhow::Error) -> Result<()> {
    let timestamp = now();
    let task_id: String = db.query_row(
        "SELECT task_id FROM agent_runs WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    db.execute(
        "UPDATE agent_runs SET status='failed',error_code='agent_worker_interrupted',error_message='无法启动本机 Agent Worker。',completed_at=?2,updated_at=?2 WHERE id=?1",
        params![run_id, timestamp],
    )?;
    tasks::fail_runner(db, &task_id, None, "无法启动本机 Agent Worker。")?;
    Ok(())
}

fn cancel_requested(db: &Connection, run_id: &str, task_id: &str) -> Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_runs ar JOIN tasks t ON t.id=?2 WHERE ar.id=?1 AND (ar.cancel_requested_at IS NOT NULL OR ar.status='cancelled' OR t.cancel_requested_at IS NOT NULL))",
        params![run_id, task_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn finalize_worker_error(db: &mut Connection, run_id: &str, error: &anyhow::Error) -> Result<()> {
    let run = load(db, run_id)?;
    let code = contracts::error_code(error);
    let timestamp = now();
    if code == "agent_run_cancelled" || run.cancel_requested_at.is_some() {
        tasks::finish_runner_cancel(db, &run.task_id)?;
        db.execute(
            "UPDATE agent_runs SET status='cancelled',worker_pid=NULL,error_code=NULL,error_message=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
            params![run_id, &timestamp],
        )?;
        db.execute(
            "UPDATE agent_run_batches SET status='cancelled',completed_at=?2,updated_at=?2 WHERE run_id=?1 AND status IN ('queued','running')",
            params![run_id, &timestamp],
        )?;
        return Ok(());
    }
    let message = public_error_message(code);
    let worker = format!("codex-{run_id}");
    tasks::fail_runner(db, &run.task_id, Some(&worker), message)?;
    db.execute(
        "UPDATE agent_runs SET status='failed',worker_pid=NULL,error_code=?2,error_message=?3,completed_at=?4,updated_at=?4 WHERE id=?1",
        params![run_id, code, message, &timestamp],
    )?;
    db.execute(
        "UPDATE agent_run_batches SET status='failed',error_code=?2,error_message=?3,completed_at=?4,updated_at=?4 WHERE run_id=?1 AND status='running'",
        params![run_id, code, message, &timestamp],
    )?;
    Ok(())
}

fn public_error_message(code: &str) -> &'static str {
    match code {
        "codex_cli_missing" => "Codex CLI 不可用。",
        "codex_not_logged_in" => "Codex CLI 尚未登录。",
        "agent_run_timeout" => "本机 Agent 处理超时；可以显式继续。",
        "agent_batch_incomplete" => "Agent 没有确认处理全部字幕段。",
        "agent_segment_duplicate" => "Agent 结果包含重复字幕段。",
        "agent_segment_unauthorized" => "Agent 结果包含任务范围外字幕段。",
        "agent_project_version_conflict" => "处理期间项目版本已变化；结果未提交。",
        "patch_before_mismatch" => "Agent 建议原文与任务基线不一致。",
        _ => "Codex 未返回可安全提交的结构化结果。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn database_fixture() -> (
        tempfile::TempDir,
        Connection,
        crate::model::Project,
        crate::model::Task,
        String,
    ) {
        let temp = tempdir().unwrap();
        let mut database = db::open_at(&temp.path().join("core.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut database, &media, Some("test".into())).unwrap();
        let segment =
            project::add_segment(&mut database, &project.id, 0.0, 1.0, "hello".into(), None)
                .unwrap();
        let task = tasks::create(&mut database, &project.id, "polish", None).unwrap();
        (temp, database, project, task, segment.id)
    }

    fn insert_test_run(
        database: &mut Connection,
        task: &crate::model::Task,
        project_id: &str,
        segment_id: &str,
        status: &str,
        updated_at: &str,
    ) -> String {
        let run_id = new_id("ar");
        database
            .execute(
                "INSERT INTO agent_runs(id,task_id,project_id,status,base_version_id,progress,current_batch,batch_count,timeout_seconds,created_at,updated_at,attempt_count) VALUES(?1,?2,?3,?4,?5,0,0,1,30,?6,?6,1)",
                params![&run_id, &task.id, project_id, status, task.base_version_id.as_deref().unwrap(), updated_at],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO agent_run_batches(id,run_id,ordinal,status,segment_ids_json,created_at,updated_at) VALUES(?1,?2,0,'queued',?3,?4,?4)",
                params![new_id("arb"), &run_id, serde_json::to_string(&vec![segment_id]).unwrap(), updated_at],
            )
            .unwrap();
        run_id
    }

    fn fake_codex_script(directory: &Path, base_version_id: &str, segment_id: &str) -> PathBuf {
        let path = directory.join("fake-codex.cmd");
        fs::write(
            &path,
            format!(
                "@echo off\r\nif \"%~1\"==\"--version\" (\r\n  echo codex-cli 0.fake\r\n  exit /b 0\r\n)\r\nif \"%~1\"==\"login\" (\r\n  echo Logged in using ChatGPT\r\n  exit /b 0\r\n)\r\n> result.json echo {{\"baseVersionId\":\"{base_version_id}\",\"processedSegmentIds\":[\"{segment_id}\"],\"patches\":[{{\"segmentId\":\"{segment_id}\",\"before\":\"hello\",\"after\":\"hello.\",\"reason\":\"fake codex integration\",\"confidence\":0.9}}]}}\r\necho {{\"type\":\"thread.started\",\"thread_id\":\"fake-thread\"}}\r\necho {{\"type\":\"turn.completed\"}}\r\nexit /b 0\r\n"
            ),
        )
        .unwrap();
        path
    }

    fn slow_codex_script(directory: &Path) -> PathBuf {
        let path = directory.join("slow-codex.cmd");
        fs::write(
            &path,
            "@echo off\r\nping.exe -n 10 127.0.0.1 >nul\r\nexit /b 0\r\n",
        )
        .unwrap();
        path
    }

    #[test]
    fn batches_segment_tasks_but_keeps_summary_atomic() {
        let ids = (0..161)
            .map(|index| format!("s-{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            split_batches("polish", &ids)
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![80, 80, 1]
        );
        assert_eq!(split_batches("summary", &ids), vec![ids]);
    }

    #[test]
    fn invocation_excludes_user_config_credentials_and_machine_targets() {
        let payload = json!({
            "task": {"segments":[{"id":"s-1","text":"safe text"}]},
            "completionRule":"review only"
        });
        let executable = Path::new(r"C:\Program Files\Codex\codex.exe");
        let spec = invocation_spec(executable, serde_json::to_string(&payload).unwrap());
        let arguments = spec.arguments.join(" ").to_ascii_lowercase();
        assert!(arguments.contains("--sandbox read-only"));
        assert!(arguments.contains("--ignore-user-config"));
        assert!(arguments.contains("--ignore-rules"));
        assert!(!arguments.contains("siaocut.db"));
        assert!(!arguments.contains("githubprojects"));
        assert!(!spec.stdin.to_ascii_lowercase().contains("media path"));
        for forbidden in [
            "CODEX_HOME",
            "CODEX_API_KEY",
            "OPENAI_API_KEY",
            "SIAOCUT_HOME",
            "PWD",
        ] {
            assert!(!spec.environment.contains_key(forbidden));
        }
    }

    #[test]
    fn batch_validation_rejects_missing_duplicate_and_unauthorized_segments() {
        let payload = json!({
            "kind":"polish","baseVersionId":"v-1",
            "segments":[{"id":"s-1","text":"one"},{"id":"s-2","text":"two"}]
        });
        let ids = vec!["s-1".to_owned(), "s-2".to_owned()];
        let patch = |id: &str, before: &str| json!({"segmentId":id,"before":before,"after":before,"reason":"review","confidence":0.8});
        let missing = json!({"baseVersionId":"v-1","processedSegmentIds":["s-1"],"patches":[patch("s-1","one")]});
        assert_eq!(
            contracts::error_code(&validate_batch_result(&payload, &ids, &missing).unwrap_err()),
            "agent_batch_incomplete"
        );
        let duplicate = json!({"baseVersionId":"v-1","processedSegmentIds":["s-1","s-1"],"patches":[patch("s-1","one")]});
        assert_eq!(
            contracts::error_code(&validate_batch_result(&payload, &ids, &duplicate).unwrap_err()),
            "agent_segment_duplicate"
        );
        let unauthorized = json!({"baseVersionId":"v-1","processedSegmentIds":["s-1","s-3"],"patches":[patch("s-1","one")]});
        assert_eq!(
            contracts::error_code(
                &validate_batch_result(&payload, &ids, &unauthorized).unwrap_err()
            ),
            "agent_segment_unauthorized"
        );
    }

    #[test]
    fn batch_validation_accepts_complete_reviewable_result() {
        let payload = json!({
            "kind":"polish","baseVersionId":"v-1",
            "segments":[{"id":"s-1","text":"one"},{"id":"s-2","text":"two"}]
        });
        let ids = vec!["s-1".to_owned(), "s-2".to_owned()];
        let result = json!({
            "baseVersionId":"v-1","processedSegmentIds":["s-1","s-2"],
            "patches":[{"segmentId":"s-1","before":"one","after":"One.","reason":"punctuation","confidence":0.9}]
        });
        validate_batch_result(&payload, &ids, &result).unwrap();
    }

    #[test]
    fn batch_validation_rejects_result_outside_schema() {
        let payload = json!({
            "kind":"polish","baseVersionId":"v-1",
            "segments":[{"id":"s-1","text":"one"}]
        });
        let error = validate_batch_result(&payload, &["s-1".into()], &json!({})).unwrap_err();
        assert_eq!(contracts::error_code(&error), "agent_output_invalid");
        let schema = output_schema("polish", "v-1");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["patches"]["items"]["additionalProperties"],
            false
        );
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("processedSegmentIds"))
        );
    }

    #[test]
    fn parses_only_sanitized_event_state() {
        let raw = b"{\"type\":\"thread.started\",\"thread_id\":\"thread-1\",\"secret\":\"discard\"}\n{\"type\":\"item.completed\",\"text\":\"discard reasoning\"}\n{\"type\":\"turn.completed\"}\n";
        let summary = parse_events(BufReader::new(&raw[..]));
        assert_eq!(summary.thread_id.as_deref(), Some("thread-1"));
        assert!(summary.saw_turn_completed);
        assert!(!summary.saw_error);
    }

    #[test]
    fn fake_codex_worker_submits_only_pending_review_changes() {
        let (temp, mut database, project, task, segment_id) = database_fixture();
        let run_id = insert_test_run(
            &mut database,
            &task,
            &project.id,
            &segment_id,
            "running",
            &now(),
        );
        let script = fake_codex_script(
            temp.path(),
            task.base_version_id.as_deref().unwrap(),
            &segment_id,
        );
        let health = health_with(&script);
        assert!(health.available);
        assert!(health.authenticated);
        let config = RunnerConfig {
            executable: script,
            temp_root: temp.path().join("agent-temp"),
        };

        execute_run_with_config(&mut database, &run_id, &config).unwrap();

        let run = load(&database, &run_id).unwrap();
        assert_eq!(run.status, "completed");
        assert_eq!(run.codex_thread_id.as_deref(), Some("fake-thread"));
        assert_eq!(run.batches[0].status, "completed");
        let reloaded = project::load(&database, &project.id).unwrap();
        assert_eq!(reloaded.transcript.segments[0].text, "hello");
        assert_eq!(reloaded.tasks[0].status, "review");
        assert_eq!(reloaded.patch_sets[0].status, "pending_review");
        assert_eq!(reloaded.patch_sets[0].items[0].after_text, "hello.");
    }

    #[test]
    fn stale_worker_is_interrupted_without_applying_content() {
        let (_temp, mut database, project, task, segment_id) = database_fixture();
        tasks::claim(&mut database, "codex-stale", Some(&task.id))
            .unwrap()
            .unwrap();
        let stale = (chrono::Utc::now() - chrono::Duration::seconds(10)).to_rfc3339();
        let run_id = insert_test_run(
            &mut database,
            &task,
            &project.id,
            &segment_id,
            "running",
            &stale,
        );

        reconcile_interrupted(&mut database).unwrap();

        assert_eq!(load(&database, &run_id).unwrap().status, "interrupted");
        let reloaded = project::load(&database, &project.id).unwrap();
        assert_eq!(reloaded.tasks[0].status, "interrupted");
        assert!(reloaded.patch_sets.is_empty());
    }

    #[test]
    fn project_version_change_blocks_submission() {
        let (_temp, mut database, project, _task, segment_id) = database_fixture();
        let base = project::current_version_id(&database, &project.id)
            .unwrap()
            .unwrap();
        project::edit_segment(&mut database, &project.id, &segment_id, "human edit".into())
            .unwrap();
        let error = ensure_project_version(&database, &project.id, &base).unwrap_err();
        assert_eq!(
            contracts::error_code(&error),
            "agent_project_version_conflict"
        );
    }

    #[test]
    fn missing_or_logged_out_cli_is_reported_without_raw_output() {
        let missing = health_with(Path::new(r"Z:\missing\codex.exe"));
        assert!(!missing.available);
        assert!(!missing.authenticated);
        assert_eq!(auth_mode("Not logged in"), None);
        assert_eq!(auth_mode("Logged in using ChatGPT"), Some("chatgpt".into()));
    }

    #[test]
    fn timeout_terminates_fake_codex_process_tree() {
        let (temp, mut database, project, task, segment_id) = database_fixture();
        let run_id = insert_test_run(
            &mut database,
            &task,
            &project.id,
            &segment_id,
            "running",
            &now(),
        );
        let batch_id = load(&database, &run_id).unwrap().batches[0].id.clone();
        let payload = json!({
            "kind":"polish","baseVersionId":task.base_version_id,
            "segments":[{"id":segment_id,"text":"hello"}]
        });
        let config = RunnerConfig {
            executable: slow_codex_script(temp.path()),
            temp_root: temp.path().join("agent-temp"),
        };
        let error = invoke_codex(
            &mut database,
            &config,
            &run_id,
            &task.id,
            &batch_id,
            &payload,
            &output_schema("polish", task.base_version_id.as_deref().unwrap()),
            0,
        )
        .unwrap_err();
        assert_eq!(contracts::error_code(&error), "agent_run_timeout");
    }

    #[test]
    fn cancellation_terminates_fake_codex_process_tree() {
        let (temp, mut database, project, task, segment_id) = database_fixture();
        let run_id = insert_test_run(
            &mut database,
            &task,
            &project.id,
            &segment_id,
            "running",
            &now(),
        );
        database
            .execute(
                "UPDATE agent_runs SET cancel_requested_at=?2 WHERE id=?1",
                params![&run_id, now()],
            )
            .unwrap();
        let batch_id = load(&database, &run_id).unwrap().batches[0].id.clone();
        let payload = json!({
            "kind":"polish","baseVersionId":task.base_version_id,
            "segments":[{"id":segment_id,"text":"hello"}]
        });
        let config = RunnerConfig {
            executable: slow_codex_script(temp.path()),
            temp_root: temp.path().join("agent-temp"),
        };
        let error = invoke_codex(
            &mut database,
            &config,
            &run_id,
            &task.id,
            &batch_id,
            &payload,
            &output_schema("polish", task.base_version_id.as_deref().unwrap()),
            30,
        )
        .unwrap_err();
        assert_eq!(contracts::error_code(&error), "agent_run_cancelled");
    }
}
