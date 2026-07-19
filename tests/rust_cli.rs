use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;

fn run(home: &Path, arguments: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_siaocut-core"))
        .env("SIAOCUT_HOME", home)
        .env("SIAOCUT_SERVICE_IDLE_MS", "100")
        .args(["--json"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_error(home: &Path, arguments: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_siaocut-core"))
        .env("SIAOCUT_HOME", home)
        .env("SIAOCUT_SERVICE_IDLE_MS", "100")
        .args(["--json"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(!output.status.success());
    serde_json::from_slice(&output.stderr).unwrap()
}

fn run_direct(home: &Path, arguments: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_siaocut-core"))
        .env("SIAOCUT_HOME", home)
        .env("SIAOCUT_DIRECT", "1")
        .args(["--json"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "direct CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_direct_error(home: &Path, arguments: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_siaocut-core"))
        .env("SIAOCUT_HOME", home)
        .env("SIAOCUT_DIRECT", "1")
        .args(["--json"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(!output.status.success());
    serde_json::from_slice(&output.stderr).unwrap()
}

#[test]
fn health_uses_stable_json_envelope() {
    let temp = tempdir().unwrap();
    let response = run(temp.path(), &["health"]);
    assert_eq!(response["apiVersion"], "0.1");
    assert_eq!(response["status"], "ok");
    assert!(
        response["database"]
            .as_str()
            .unwrap()
            .ends_with("siaocut.db")
    );
}

#[test]
fn subtitle_structure_cli_is_explicit_atomic_and_recoverable() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("subtitle-structure.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run_direct(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    for (start, end, text) in [
        ("0", "2", "hello world"),
        ("2", "4", "second"),
        ("4", "6", "third"),
    ] {
        run_direct(
            temp.path(),
            &[
                "transcript",
                "add",
                project_id,
                "--start",
                start,
                "--end",
                end,
                "--text",
                text,
            ],
        );
    }
    let shown = run_direct(temp.path(), &["project", "show", project_id]);
    let segment_ids = shown["project"]["transcript"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|segment| segment["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();

    let invalid = run_direct_error(
        temp.path(),
        &[
            "transcript",
            "merge",
            project_id,
            &segment_ids[0],
            &segment_ids[2],
        ],
    );
    assert_eq!(invalid["code"], "subtitle_merge_not_adjacent");
    assert_eq!(invalid["error"]["code"], "subtitle_merge_not_adjacent");
    let unchanged = run_direct(temp.path(), &["project", "show", project_id]);
    assert_eq!(
        unchanged["project"]["transcript"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let split = run_direct(
        temp.path(),
        &[
            "transcript",
            "split",
            project_id,
            &segment_ids[0],
            "--text-offset",
            "5",
            "--at",
            "1",
        ],
    );
    assert_eq!(split["structureEdit"]["operation"], "split");
    assert_eq!(
        split["structureEdit"]["affectedSegmentIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let right_id = split["structureEdit"]["createdSegmentId"].as_str().unwrap();

    let merged = run_direct(
        temp.path(),
        &["transcript", "merge", project_id, right_id, &segment_ids[0]],
    );
    assert_eq!(merged["structureEdit"]["operation"], "merge");
    assert_eq!(merged["structureEdit"]["removedSegmentIds"][0], right_id);
    assert_eq!(
        merged["structureEdit"]["project"]["transcript"]["segments"][0]["text"],
        "hello world"
    );

    let timed = run_direct(
        temp.path(),
        &[
            "transcript",
            "timing",
            project_id,
            &segment_ids[0],
            "--start",
            "0.1",
            "--end",
            "2.1",
        ],
    );
    assert_eq!(timed["structureEdit"]["operation"], "timing");
    assert_eq!(
        timed["structureEdit"]["project"]["transcript"]["segments"][0]["start"],
        0.1
    );

    let offset = run_direct(
        temp.path(),
        &[
            "transcript",
            "offset",
            project_id,
            "--segment",
            &segment_ids[0],
            "--segment",
            &segment_ids[1],
            "--delta",
            "0.5",
        ],
    );
    assert_eq!(offset["structureEdit"]["operation"], "offset");
    assert_eq!(
        offset["structureEdit"]["affectedSegmentIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        offset["structureEdit"]["project"]["transcript"]["segments"][0]["start"],
        0.6
    );

    let undone = run_direct(temp.path(), &["project", "undo", project_id]);
    assert_eq!(undone["project"]["transcript"]["segments"][0]["start"], 0.1);
    assert_eq!(undone["project"]["history"]["canRedo"], true);
    let redone = run_direct(temp.path(), &["project", "redo", project_id]);
    assert_eq!(redone["project"]["transcript"]["segments"][0]["start"], 0.6);
    assert_eq!(fs::read(&media).unwrap(), b"audio");
}

#[test]
fn subtitle_file_cli_previews_confirms_checks_and_recovers() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("subtitle-import.wav");
    let captions = temp.path().join("captions.srt");
    fs::write(&media, b"audio").unwrap();
    fs::write(
        &captions,
        "1\n00:00:00,000 --> 00:00:01,500\nImported first\n\n2\n00:00:01,700 --> 00:00:03,000\nImported second",
    )
    .unwrap();
    let imported = run_direct(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    run_direct(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0",
            "--end",
            "1",
            "--text",
            "Original",
        ],
    );

    let preview = run_direct(
        temp.path(),
        &[
            "transcript",
            "inspect-file",
            project_id,
            captions.to_str().unwrap(),
        ],
    );
    assert_eq!(preview["subtitleImportPreview"]["format"], "srt");
    assert_eq!(preview["subtitleImportPreview"]["segmentCount"], 2);
    assert_eq!(preview["subtitleImportPreview"]["canImport"], true);
    assert_eq!(
        preview["subtitleImportPreview"]["requiresConfirmation"],
        true
    );
    let hash = preview["subtitleImportPreview"]["sha256"].as_str().unwrap();

    let confirmation = run_direct_error(
        temp.path(),
        &[
            "transcript",
            "import-file",
            project_id,
            captions.to_str().unwrap(),
            "--expected-sha256",
            hash,
        ],
    );
    assert_eq!(
        confirmation["code"],
        "subtitle_import_confirmation_required"
    );
    let unchanged = run_direct(temp.path(), &["project", "show", project_id]);
    assert_eq!(
        unchanged["project"]["transcript"]["segments"][0]["text"],
        "Original"
    );

    let applied = run_direct(
        temp.path(),
        &[
            "transcript",
            "import-file",
            project_id,
            captions.to_str().unwrap(),
            "--confirm-replace",
            "--expected-sha256",
            hash,
        ],
    );
    assert_eq!(applied["subtitleImport"]["insertedSegments"], 2);
    assert_eq!(
        applied["project"]["transcript"]["segments"][0]["text"],
        "Imported first"
    );
    assert_eq!(applied["project"]["history"]["canUndo"], true);
    assert_eq!(fs::read(&media).unwrap(), b"audio");

    let quality = run_direct(temp.path(), &["transcript", "quality", project_id]);
    assert_eq!(quality["subtitleQuality"]["status"], "good");
    assert_eq!(quality["subtitleQuality"]["statusLabel"], "未发现字幕问题");
    let undone = run_direct(temp.path(), &["project", "undo", project_id]);
    assert_eq!(
        undone["project"]["transcript"]["segments"][0]["text"],
        "Original"
    );
}

#[test]
fn speech_analyze_exposes_local_rhythm_evidence() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("speech.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run_direct(
        temp.path(),
        &["import", media.to_str().unwrap(), "--title", "Speech test"],
    );
    let project_id = imported["projectId"].as_str().unwrap();
    let database = rusqlite::Connection::open(temp.path().join("siaocut.db")).unwrap();
    database.execute(
        "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES('s1',?1,0,3.2,'嗯 开始',0.8)",
        [project_id],
    ).unwrap();
    database.execute(
        "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,confidence,ordinal) VALUES('w1',?1,'s1',0,0.5,'嗯',0.5,0),('w2',?1,'s1',2.5,3.2,'开始',0.9,1)",
        [project_id],
    ).unwrap();
    drop(database);

    let response = run_direct(temp.path(), &["speech", "analyze", project_id]);
    assert_eq!(response["speechInsights"]["status"], "ready");
    assert_eq!(response["speechInsights"]["analyzerVersion"], "rhythm-v1");
    assert_eq!(response["speechInsights"]["tokenCount"], 2);
    assert_eq!(response["speechInsights"]["fillerCount"], 1);
    assert_eq!(response["speechInsights"]["longPauseCount"], 1);
    assert_eq!(response["speechInsights"]["pauses"][0]["duration"], 2.0);

    let shown = run_direct(temp.path(), &["project", "show", project_id]);
    assert_eq!(
        shown["project"]["speechInsights"],
        response["speechInsights"]
    );
}

#[test]
fn audio_analysis_cli_is_local_cancellable_and_explicitly_resumable() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("quality.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run_direct(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();

    let started = run_direct(
        temp.path(),
        &[
            "speech",
            "audio-start",
            project_id,
            "--start-delay-ms",
            "30000",
        ],
    );
    let job_id = started["audioAnalysisJob"]["id"].as_str().unwrap();
    assert_eq!(started["audioAnalysisJob"]["projectId"], project_id);

    let duplicate = run_direct(temp.path(), &["speech", "audio-start", project_id]);
    assert_eq!(duplicate["audioAnalysisJob"]["id"], job_id);
    let cancelled = run_direct(temp.path(), &["speech", "audio-cancel", job_id]);
    assert_eq!(cancelled["audioAnalysisJob"]["status"], "cancelled");
    assert!(cancelled["audioAnalysisJob"]["cancelRequestedAt"].is_string());

    let resumed = run_direct(
        temp.path(),
        &[
            "speech",
            "audio-resume",
            job_id,
            "--start-delay-ms",
            "30000",
        ],
    );
    assert_eq!(resumed["audioAnalysisJob"]["status"], "queued");
    assert_eq!(resumed["audioAnalysisJob"]["attemptCount"], 2);
    let latest = run_direct(temp.path(), &["speech", "audio-latest", project_id]);
    assert_eq!(latest["audioAnalysisJob"]["id"], job_id);
    let _ = run_direct(temp.path(), &["speech", "audio-cancel", job_id]);
}

#[test]
fn auto_workflow_cli_starts_deduplicates_queries_and_cancels() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("auto.wav");
    let model = temp.path().join("model.bin");
    let output = temp.path().join("auto.mp4");
    fs::write(&media, b"audio").unwrap();
    fs::write(&model, b"model").unwrap();
    let arguments = [
        "auto",
        "start",
        "--media",
        media.to_str().unwrap(),
        "--model",
        model.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--locale",
        "en-US",
        "--start-delay-ms",
        "5000",
    ];
    let started = run_direct(temp.path(), &arguments);
    let workflow_id = started["workflowId"].as_str().unwrap();
    assert_eq!(started["workflow"]["currentStage"], "import");
    assert_eq!(started["workflow"]["instructionLocale"], "en-US");

    let mut status = Value::Null;
    for _ in 0..40 {
        status = run_direct(temp.path(), &["auto", "status", workflow_id]);
        if status["workflow"]["workerPid"].is_number() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(status["workflow"]["status"], "running");
    assert!(status["workflow"]["workerPid"].is_number());

    let duplicate = run_direct(temp.path(), &arguments);
    assert_eq!(duplicate["workflowId"], workflow_id);
    let listed = run_direct(temp.path(), &["auto", "list"]);
    assert_eq!(listed["workflows"].as_array().unwrap().len(), 1);

    let cancelled = run_direct(temp.path(), &["auto", "cancel", workflow_id]);
    assert_eq!(cancelled["workflow"]["status"], "cancelled");
    assert!(cancelled["workflow"]["cancelRequestedAt"].is_string());
    let events = run_direct(
        temp.path(),
        &["auto", "events", workflow_id, "--after", "0"],
    );
    assert!(events["events"].as_array().unwrap().len() >= 2);
}

#[test]
fn source_inspect_rejects_private_network_before_creating_a_project() {
    let temp = tempdir().unwrap();
    let response = run_error(
        temp.path(),
        &["source", "inspect", "https://127.0.0.1/video.mp4"],
    );
    assert_eq!(response["code"], "source_private_network");
    assert_eq!(response["status"], "error");
    let database = rusqlite::Connection::open(temp.path().join("siaocut.db")).unwrap();
    let projects: i64 = database
        .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
        .unwrap();
    assert_eq!(projects, 0);
}

#[test]
fn project_agent_and_export_contract_remain_compatible() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("talk.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run(
        temp.path(),
        &["import", media.to_str().unwrap(), "--title", "CLI test"],
    );
    let project_id = imported["projectId"].as_str().unwrap();
    assert_eq!(
        imported["project"]["canvasSettings"]["aspectRatio"],
        "source"
    );
    let canvas = run(
        temp.path(),
        &[
            "canvas",
            "set",
            project_id,
            "--aspect-ratio",
            "9:16",
            "--framing",
            "cover-center",
        ],
    );
    assert_eq!(canvas["canvasSettings"]["aspectRatio"], "9:16");
    assert_eq!(canvas["canvasSettings"]["framing"], "cover-center");
    let shown = run(temp.path(), &["canvas", "show", project_id]);
    assert_eq!(shown["canvasSettings"], canvas["canvasSettings"]);
    let added = run(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0",
            "--end",
            "2",
            "--text",
            "你好",
        ],
    );
    let segment_id = added["segment"]["id"].as_str().unwrap();
    let task = run(
        temp.path(),
        &[
            "task",
            "create",
            project_id,
            "--kind",
            "translate",
            "--lang",
            "en",
            "--locale",
            "en-US",
        ],
    );
    assert_eq!(task["task"]["instructionLocale"], "en-US");
    let task_id = task["taskId"].as_str().unwrap();
    let claim = run(
        temp.path(),
        &["task", "claim", "--worker", "integration-agent"],
    );
    assert_eq!(claim["taskId"], task_id);
    assert_eq!(claim["instructionLocale"], "en-US");
    assert_eq!(claim["contentLanguage"], "auto");
    assert!(
        claim["payload"]["instructions"]
            .as_str()
            .unwrap()
            .starts_with("Translate each segment")
    );
    assert!(claim["payload"]["baseVersionId"].is_string());
    let base_version_id = claim["payload"]["baseVersionId"].as_str().unwrap();
    let heartbeat = run(
        temp.path(),
        &[
            "task",
            "heartbeat",
            task_id,
            "--worker",
            "integration-agent",
            "--progress",
            "0.5",
            "--message",
            "翻译进行中",
        ],
    );
    assert_eq!(heartbeat["task"]["progress"], 0.5);
    let events = run(temp.path(), &["task", "events", task_id, "--after", "0"]);
    assert!(events["events"].as_array().unwrap().len() >= 3);

    let response_file = temp.path().join("response.json");
    fs::write(
        &response_file,
        json!({"baseVersionId":base_version_id,"patches":[{"segmentId":segment_id,"before":"你好","after":"Hello","reason":"翻译为英语","confidence":0.98}]}).to_string(),
    )
    .unwrap();
    let submitted = run(
        temp.path(),
        &[
            "task",
            "submit",
            task_id,
            "--worker",
            "integration-agent",
            "--response",
            response_file.to_str().unwrap(),
        ],
    );
    assert_eq!(submitted["task"]["status"], "review");
    assert_eq!(submitted["patchSet"]["items"][0]["status"], "pending");
    let diff = run(temp.path(), &["task", "diff", task_id]);
    assert_eq!(diff["patchSet"]["items"][0]["afterText"], "Hello");
    let reviewed = run(
        temp.path(),
        &["task", "review-all", task_id, "--action", "apply"],
    );
    assert_eq!(reviewed["patchSet"]["status"], "applied");
    assert_eq!(reviewed["project"]["tasks"][0]["status"], "done");

    let output = temp.path().join("talk.srt");
    let exported = run(
        temp.path(),
        &[
            "transcript",
            "export",
            project_id,
            "--format",
            "srt",
            "--lang",
            "en",
            "--subtitle-mode",
            "translated",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert_eq!(exported["audit"]["ready"], true);
    assert!(fs::read_to_string(output).unwrap().contains("Hello"));
}

#[test]
fn word_range_cut_cli_previews_applies_and_exports_remaining_words() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("word-cut.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    let added = run(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0.2",
            "--end",
            "2.7",
            "--text",
            "hello brave world",
        ],
    );
    let segment_id = added["segment"]["id"].as_str().unwrap();
    let database = rusqlite::Connection::open(temp.path().join("siaocut.db")).unwrap();
    for (id, start, end, text, ordinal) in [
        ("w1", 0.2, 0.7, "hello", 0),
        ("w2", 1.0, 1.5, "brave", 1),
        ("w3", 2.0, 2.7, "world", 2),
    ] {
        database
            .execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![id, project_id, segment_id, start, end, text, ordinal],
            )
            .unwrap();
    }
    drop(database);

    let created = run(
        temp.path(),
        &[
            "cut",
            "create",
            project_id,
            "--segment",
            segment_id,
            "--from-word",
            "w2",
            "--to-word",
            "w2",
            "--padding-ms",
            "200",
        ],
    );
    let cut_id = created["cut"]["id"].as_str().unwrap();
    assert_eq!(created["cut"]["start"], 0.8);
    assert_eq!(created["cut"]["end"], 1.7);
    let preview = run(temp.path(), &["cut", "preview", project_id, cut_id]);
    assert_eq!(preview["preview"]["previewStart"], 0.0);
    assert_eq!(preview["preview"]["previewEnd"], 2.7);
    let applied = run(temp.path(), &["cut", "apply", project_id, cut_id]);
    assert_eq!(applied["cut"]["status"], "applied");

    let output = temp.path().join("word-cut.srt");
    run(
        temp.path(),
        &[
            "transcript",
            "export",
            project_id,
            "--format",
            "srt",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    let srt = fs::read_to_string(output).unwrap();
    assert!(srt.contains("hello"));
    assert!(srt.contains("world"));
    assert!(!srt.contains("brave"));

    let undone = run(temp.path(), &["project", "undo", project_id]);
    assert_eq!(undone["project"]["history"]["canRedo"], true);
    assert_eq!(undone["project"]["edits"][0]["status"], "proposed");
    let restored_output = temp.path().join("word-cut-undone.srt");
    run(
        temp.path(),
        &[
            "transcript",
            "export",
            project_id,
            "--format",
            "srt",
            "-o",
            restored_output.to_str().unwrap(),
        ],
    );
    assert!(
        fs::read_to_string(restored_output)
            .unwrap()
            .contains("brave")
    );
    let redone = run(temp.path(), &["project", "redo", project_id]);
    assert_eq!(redone["project"]["history"]["canRedo"], false);
    assert_eq!(redone["project"]["edits"][0]["status"], "applied");
}

#[test]
fn cut_detect_returns_reviewable_typed_word_suggestions() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("suggestions.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    let added = run(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0",
            "--end",
            "2",
            "--text",
            "we um can start",
        ],
    );
    let segment_id = added["segment"]["id"].as_str().unwrap();
    let database = rusqlite::Connection::open(temp.path().join("siaocut.db")).unwrap();
    for (ordinal, (id, text)) in [
        ("sw1", "we"),
        ("sw2", "um"),
        ("sw3", "can"),
        ("sw4", "start"),
    ]
    .into_iter()
    .enumerate()
    {
        let start = ordinal as f64 * 0.5;
        database
            .execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![id, project_id, segment_id, start, start + 0.3, text, ordinal as i64],
            )
            .unwrap();
    }
    drop(database);

    let detected = run(temp.path(), &["cut", "detect", project_id]);
    assert_eq!(detected["suggestions"].as_array().unwrap().len(), 1);
    let suggestion = &detected["suggestions"][0];
    assert_eq!(suggestion["status"], "proposed");
    assert_eq!(suggestion["kind"], "word_cut");
    assert_eq!(suggestion["cutRange"]["fromWordId"], "sw2");
    assert_eq!(suggestion["cutRange"]["toWordId"], "sw2");
    assert_eq!(
        suggestion["suggestion"]["suggestionType"],
        "standalone_filler"
    );
    assert_eq!(suggestion["suggestion"]["confidence"], 0.99);
    let cut_id = suggestion["id"].as_str().unwrap();
    let shown = run(temp.path(), &["project", "show", project_id]);
    assert_eq!(shown["project"]["timeline"]["outputDuration"], 2.0);
    assert_eq!(
        run(temp.path(), &["cut", "preview", project_id, cut_id])["preview"]["skipRange"],
        true
    );
    assert_eq!(
        run(temp.path(), &["cut", "apply", project_id, cut_id])["cut"]["status"],
        "applied"
    );
    assert_eq!(
        run(temp.path(), &["cut", "restore", project_id, cut_id])["restored"]["status"],
        "restored"
    );
    assert!(
        run(temp.path(), &["cut", "detect", project_id])["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn subtitle_style_is_recoverable_and_drives_ass_export() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("styled-subtitles.wav");
    fs::write(&media, b"audio").unwrap();
    let imported = run(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    run(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0",
            "--end",
            "2",
            "--text",
            "受控字幕样式",
        ],
    );
    let before = run(temp.path(), &["project", "show", project_id]);
    let styled = run(
        temp.path(),
        &[
            "transcript",
            "set-style",
            project_id,
            "--preset",
            "emphasis",
            "--position",
            "center",
        ],
    );
    assert_eq!(styled["subtitleStyle"]["preset"], "emphasis");
    assert_eq!(styled["subtitleStyle"]["position"], "center");
    assert_eq!(
        styled["project"]["transcript"],
        before["project"]["transcript"]
    );

    let output = temp.path().join("styled.ass");
    run(
        temp.path(),
        &[
            "transcript",
            "export",
            project_id,
            "--format",
            "ass",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    let ass = fs::read_to_string(output).unwrap();
    assert!(ass.contains("Style: Primary,Microsoft YaHei UI,60"));
    assert!(ass.contains(",4,2,5,80,80,0,1"));
    assert!(ass.contains("Dialogue: 0,0:00:00,0:00:02,Primary,受控字幕样式"));

    let undone = run(temp.path(), &["project", "undo", project_id]);
    assert_eq!(undone["project"]["subtitleStyle"]["preset"], "standard");
    assert_eq!(
        undone["project"]["transcript"],
        before["project"]["transcript"]
    );
}

#[test]
fn voice_subtitle_agent_and_style_changes_share_one_recoverable_time_map() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("combined-workbench.wav");
    fs::write(&media, b"immutable source media").unwrap();
    let source_bytes = fs::read(&media).unwrap();
    let imported = run_direct(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    let first = run_direct(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0",
            "--end",
            "2",
            "--text",
            "we um start",
        ],
    );
    let second = run_direct(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "3",
            "--end",
            "6",
            "--text",
            "final line",
        ],
    );
    let first_id = first["segment"]["id"].as_str().unwrap();
    let second_id = second["segment"]["id"].as_str().unwrap();
    let database = rusqlite::Connection::open(temp.path().join("siaocut.db")).unwrap();
    for (id, segment_id, start, end, text, ordinal) in [
        ("cw1", first_id, 0.0, 0.5, "we", 0),
        ("cw2", first_id, 0.6, 1.0, "um", 1),
        ("cw3", first_id, 1.1, 1.5, "start", 2),
        ("cw4", second_id, 3.0, 3.5, "final", 3),
        ("cw5", second_id, 3.6, 4.0, "line", 4),
    ] {
        database
            .execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![id, project_id, segment_id, start, end, text, ordinal],
            )
            .unwrap();
    }
    drop(database);

    let speech = run_direct(temp.path(), &["speech", "analyze", project_id]);
    assert_eq!(speech["speechInsights"]["status"], "ready");
    assert_eq!(speech["speechInsights"]["fillerCount"], 1);
    assert_eq!(speech["speechInsights"]["longPauseCount"], 1);

    let cut = run_direct(
        temp.path(),
        &[
            "cut",
            "create",
            project_id,
            "--segment",
            first_id,
            "--from-word",
            "cw2",
            "--to-word",
            "cw2",
            "--padding-ms",
            "100",
        ],
    );
    let cut_id = cut["cut"]["id"].as_str().unwrap();
    run_direct(temp.path(), &["cut", "apply", project_id, cut_id]);
    let offset = run_direct(
        temp.path(),
        &[
            "transcript",
            "offset",
            project_id,
            "--segment",
            second_id,
            "--delta",
            "0.2",
        ],
    );
    assert_eq!(
        offset["structureEdit"]["project"]["transcript"]["segments"][1]["start"],
        3.2
    );

    let workflow = run_direct(
        temp.path(),
        &[
            "workflow", "create", project_id, "--kind", "polish", "--locale", "en-US",
        ],
    );
    assert_eq!(workflow["workflow"]["instructionLocale"], "en-US");
    let claim = run_direct(
        temp.path(),
        &["task", "claim", "--worker", "combined-acceptance"],
    );
    let task_id = claim["taskId"].as_str().unwrap();
    assert_eq!(claim["instructionLocale"], "en-US");
    let response = temp.path().join("combined-agent-response.json");
    fs::write(
        &response,
        json!({
            "baseVersionId": claim["payload"]["baseVersionId"],
            "patches": [{
                "segmentId": second_id,
                "before": "final line",
                "after": "Final line.",
                "reason": "Normalize punctuation",
                "confidence": 0.99
            }]
        })
        .to_string(),
    )
    .unwrap();
    run_direct(
        temp.path(),
        &[
            "task",
            "submit",
            task_id,
            "--worker",
            "combined-acceptance",
            "--response",
            response.to_str().unwrap(),
        ],
    );
    let before_review = run_direct(temp.path(), &["project", "show", project_id]);
    assert_eq!(
        before_review["project"]["transcript"]["segments"][1]["text"],
        "final line"
    );
    let reviewed = run_direct(
        temp.path(),
        &["task", "review-all", task_id, "--action", "apply"],
    );
    assert_eq!(
        reviewed["project"]["transcript"]["segments"][1]["text"],
        "Final line."
    );
    assert_eq!(
        run_direct(
            temp.path(),
            &[
                "workflow",
                "status",
                workflow["workflowId"].as_str().unwrap()
            ]
        )["workflow"]["status"],
        "completed"
    );

    run_direct(
        temp.path(),
        &[
            "transcript",
            "set-style",
            project_id,
            "--preset",
            "emphasis",
            "--position",
            "bottom",
        ],
    );
    let output = temp.path().join("combined.ass");
    run_direct(
        temp.path(),
        &[
            "transcript",
            "export",
            project_id,
            "--format",
            "ass",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    let ass = fs::read_to_string(output).unwrap();
    assert!(ass.contains("Style: Primary,Microsoft YaHei UI,60"));
    assert!(ass.contains("Dialogue: 0,0:00:02.600,0:00:05.600,Primary,Final line."));
    let accepted = run_direct(temp.path(), &["project", "show", project_id]);
    assert_eq!(accepted["project"]["speechInsights"]["status"], "ready");
    assert_eq!(accepted["project"]["edits"][0]["status"], "applied");
    assert_eq!(accepted["project"]["subtitleStyle"]["preset"], "emphasis");
    assert_eq!(fs::read(&media).unwrap(), source_bytes);
}

#[test]
fn subtitle_format_and_language_matrix_stays_explicit() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("subtitle-matrix.wav");
    fs::write(&media, b"subtitle matrix source").unwrap();
    let imported = run_direct(temp.path(), &["import", media.to_str().unwrap()]);
    let project_id = imported["projectId"].as_str().unwrap();
    let added = run_direct(
        temp.path(),
        &[
            "transcript",
            "add",
            project_id,
            "--start",
            "0",
            "--end",
            "2",
            "--text",
            "本地字幕",
        ],
    );
    let segment_id = added["segment"]["id"].as_str().unwrap();
    let task = run_direct(
        temp.path(),
        &[
            "task",
            "create",
            project_id,
            "--kind",
            "translate",
            "--lang",
            "en",
        ],
    );
    let task_id = task["taskId"].as_str().unwrap();
    let claim = run_direct(
        temp.path(),
        &["task", "claim", "--worker", "subtitle-matrix"],
    );
    let response = temp.path().join("subtitle-matrix-response.json");
    fs::write(
        &response,
        json!({
            "baseVersionId": claim["payload"]["baseVersionId"],
            "patches": [{
                "segmentId": segment_id,
                "before": "本地字幕",
                "after": "Local subtitles",
                "reason": "Translate to English",
                "confidence": 0.99
            }]
        })
        .to_string(),
    )
    .unwrap();
    run_direct(
        temp.path(),
        &[
            "task",
            "submit",
            task_id,
            "--worker",
            "subtitle-matrix",
            "--response",
            response.to_str().unwrap(),
        ],
    );
    run_direct(
        temp.path(),
        &["task", "review-all", task_id, "--action", "apply"],
    );

    for format in ["srt", "vtt", "ass", "markdown"] {
        for mode in ["source", "translated", "bilingual"] {
            let output = temp.path().join(format!("matrix-{mode}.{format}"));
            let mut arguments = vec![
                "transcript",
                "export",
                project_id,
                "--format",
                format,
                "--subtitle-mode",
                mode,
                "-o",
                output.to_str().unwrap(),
            ];
            if mode != "source" {
                arguments.extend(["--lang", "en"]);
            }
            run_direct(temp.path(), &arguments);
            let rendered = fs::read_to_string(&output).unwrap();
            assert!(!rendered.trim().is_empty(), "empty {format}/{mode} export");
            assert_eq!(rendered.contains("本地字幕"), mode != "translated");
            assert_eq!(rendered.contains("Local subtitles"), mode != "source");
            if format == "ass" {
                assert!(rendered.contains("Style: Primary"));
                assert!(rendered.contains("Style: Secondary"));
                if mode == "bilingual" {
                    assert!(rendered.contains("\\N{\\rSecondary}Local subtitles"));
                }
            }
        }
    }
}
