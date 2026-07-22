use anyhow::{Context, Result, bail};
use chrono::Utc;
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const CURRENT_SCHEMA_VERSION: i64 = 24;

struct Migration {
    version: i64,
    apply: fn(&Transaction<'_>) -> Result<()>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        apply: migration_1_initial_schema,
    },
    Migration {
        version: 2,
        apply: migration_2_resumable_tasks,
    },
    Migration {
        version: 3,
        apply: migration_3_reviewable_agent_patches,
    },
    Migration {
        version: 4,
        apply: migration_4_media_pipeline,
    },
    Migration {
        version: 5,
        apply: migration_5_model_downloads,
    },
    Migration {
        version: 6,
        apply: migration_6_worker_recovery,
    },
    Migration {
        version: 7,
        apply: migration_7_word_timings,
    },
    Migration {
        version: 8,
        apply: migration_8_align_segments_to_words,
    },
    Migration {
        version: 9,
        apply: migration_9_canvas_and_subtitle_mode,
    },
    Migration {
        version: 10,
        apply: migration_10_export_canvas_snapshot,
    },
    Migration {
        version: 11,
        apply: migration_11_word_range_cuts,
    },
    Migration {
        version: 12,
        apply: migration_12_project_history_cursor,
    },
    Migration {
        version: 13,
        apply: migration_13_cut_suggestions,
    },
    Migration {
        version: 14,
        apply: migration_14_source_imports,
    },
    Migration {
        version: 15,
        apply: migration_15_auto_workflows,
    },
    Migration {
        version: 16,
        apply: migration_16_audio_analysis_jobs,
    },
    Migration {
        version: 17,
        apply: migration_17_speaker_tracks,
    },
    Migration {
        version: 18,
        apply: migration_18_subtitle_style_snapshots,
    },
    Migration {
        version: 19,
        apply: migration_19_instruction_locales,
    },
    Migration {
        version: 20,
        apply: migration_20_transcription_providers,
    },
    Migration {
        version: 21,
        apply: migration_21_transcription_consistency,
    },
    Migration {
        version: 22,
        apply: migration_22_transcription_candidate_summary,
    },
    Migration {
        version: 23,
        apply: migration_23_agent_runs,
    },
    Migration {
        version: 24,
        apply: migration_24_translation_readiness,
    },
];

pub fn home_dir() -> PathBuf {
    env::var_os("SIAOCUT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| env::current_dir().expect("current directory is available"))
                .join("SiaoCut")
        })
}

pub fn database_path() -> PathBuf {
    home_dir().join("siaocut.db")
}

pub fn open() -> Result<Connection> {
    let home = home_dir();
    fs::create_dir_all(&home).context("无法创建 SiaoCut 数据目录")?;
    open_at(&database_path())
}

pub(crate) fn open_at(path: &Path) -> Result<Connection> {
    backup_before_upgrade(path)?;
    let mut db = Connection::open(path).context("无法打开 SiaoCut SQLite 数据库")?;
    db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    migrate(&mut db)?;
    Ok(db)
}

fn backup_before_upgrade(path: &Path) -> Result<Option<PathBuf>> {
    if !path.is_file() {
        return Ok(None);
    }
    let source = Connection::open(path).context("无法读取待升级的 SiaoCut 数据库")?;
    let has_migrations: bool = source.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !has_migrations {
        return Ok(None);
    }
    let installed: i64 = source.query_row(
        "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if installed <= 0 || installed >= CURRENT_SCHEMA_VERSION {
        return Ok(None);
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("siaocut.db");
    let backup_path = path.with_file_name(format!("{file_name}.schema-{installed}.bak"));
    if backup_path.exists() {
        return Ok(Some(backup_path));
    }
    let partial_path =
        backup_path.with_file_name(format!("{file_name}.schema-{installed}.bak.partial"));
    if partial_path.is_file() {
        fs::remove_file(&partial_path).context("无法清理未完成的数据库备份")?;
    }
    let backup_result = (|| -> Result<()> {
        let mut destination =
            Connection::open(&partial_path).context("无法创建数据库升级前备份")?;
        let backup = rusqlite::backup::Backup::new(&source, &mut destination)
            .context("无法初始化数据库升级前备份")?;
        backup
            .run_to_completion(128, Duration::from_millis(10), None)
            .context("数据库升级前备份失败")?;
        Ok(())
    })();
    if let Err(error) = backup_result {
        let _ = fs::remove_file(&partial_path);
        return Err(error);
    }
    fs::rename(&partial_path, &backup_path).context("无法完成数据库升级前备份")?;
    Ok(Some(backup_path))
}

fn migrate(db: &mut Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    let installed: i64 = db.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if installed > CURRENT_SCHEMA_VERSION {
        bail!(
            "database_version_unsupported: 数据库版本 {installed} 高于当前支持版本 {CURRENT_SCHEMA_VERSION}"
        );
    }

    for migration in MIGRATIONS.iter().filter(|item| item.version > installed) {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        (migration.apply)(&tx).with_context(|| {
            format!(
                "database_migration_failed: 迁移 {} 执行失败",
                migration.version
            )
        })?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES(?1, ?2)",
            params![migration.version, Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
    }
    Ok(())
}

fn migration_1_initial_schema(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            source_language TEXT NOT NULL DEFAULT 'auto'
        );
        CREATE TABLE IF NOT EXISTS media (
            project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
            source_path TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            extension TEXT NOT NULL,
            duration_seconds REAL
        );
        CREATE TABLE IF NOT EXISTS segments (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            start_seconds REAL NOT NULL,
            end_seconds REAL NOT NULL,
            text TEXT NOT NULL,
            confidence REAL
        );
        CREATE INDEX IF NOT EXISTS idx_segments_project_time
            ON segments(project_id, start_seconds);
        CREATE TABLE IF NOT EXISTS translations (
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            language TEXT NOT NULL,
            status TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(project_id, language)
        );
        CREATE TABLE IF NOT EXISTS translation_segments (
            project_id TEXT NOT NULL,
            language TEXT NOT NULL,
            segment_id TEXT NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
            text TEXT NOT NULL,
            PRIMARY KEY(project_id, language, segment_id),
            FOREIGN KEY(project_id, language)
                REFERENCES translations(project_id, language) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS edits (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            segment_id TEXT NOT NULL REFERENCES segments(id),
            start_seconds REAL NOT NULL,
            end_seconds REAL NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            language TEXT,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            completed_at TEXT,
            lease_worker TEXT,
            lease_id TEXT,
            lease_expires_at TEXT
        );
        CREATE TABLE IF NOT EXISTS summaries (
            project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
            text TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS versions (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL,
            snapshot_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_versions_project_created
            ON versions(project_id, created_at);
        CREATE TABLE IF NOT EXISTS operations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            created_at TEXT NOT NULL,
            payload_json TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn migration_2_resumable_tasks(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE tasks ADD COLUMN base_version_id TEXT;
         ALTER TABLE tasks ADD COLUMN progress REAL NOT NULL DEFAULT 0;
         ALTER TABLE tasks ADD COLUMN error_message TEXT;
         ALTER TABLE tasks ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE tasks ADD COLUMN cancel_requested_at TEXT;
         CREATE TABLE task_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            progress REAL,
            message TEXT NOT NULL,
            created_at TEXT NOT NULL
         );
         CREATE INDEX idx_task_events_task_id ON task_events(task_id, id);",
    )?;
    Ok(())
}

fn migration_3_reviewable_agent_patches(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE tasks ADD COLUMN workflow_id TEXT;
         CREATE TABLE workflows (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            language TEXT,
            status TEXT NOT NULL,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX idx_workflows_project ON workflows(project_id, created_at);
         CREATE TABLE agent_patch_sets (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id) ON DELETE CASCADE,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            language TEXT,
            status TEXT NOT NULL,
            base_version_id TEXT NOT NULL,
            created_at TEXT NOT NULL
         );
         CREATE TABLE agent_patch_items (
            id TEXT PRIMARY KEY,
            patch_set_id TEXT NOT NULL REFERENCES agent_patch_sets(id) ON DELETE CASCADE,
            segment_id TEXT REFERENCES segments(id),
            target TEXT NOT NULL,
            before_text TEXT NOT NULL,
            after_text TEXT NOT NULL,
            current_text_at_submit TEXT NOT NULL,
            reason TEXT NOT NULL,
            confidence REAL,
            status TEXT NOT NULL,
            ordinal INTEGER NOT NULL
         );
         CREATE INDEX idx_agent_patch_items_set ON agent_patch_items(patch_set_id, ordinal);",
    )?;
    Ok(())
}

fn migration_4_media_pipeline(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE media_artifacts (
            project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
            status TEXT NOT NULL,
            proxy_path TEXT,
            waveform_path TEXT,
            thumbnails_json TEXT NOT NULL DEFAULT '[]',
            source_sha256 TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            error_message TEXT
         );
         CREATE TABLE export_jobs (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            output_path TEXT NOT NULL,
            status TEXT NOT NULL,
            progress REAL NOT NULL DEFAULT 0,
            burn_subtitles INTEGER NOT NULL DEFAULT 0,
            language TEXT,
            bilingual INTEGER NOT NULL DEFAULT 0,
            cancel_requested_at TEXT,
            error_message TEXT,
            manifest_path TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            completed_at TEXT
         );
         CREATE INDEX idx_export_jobs_project ON export_jobs(project_id, created_at);",
    )?;
    Ok(())
}

fn migration_5_model_downloads(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE model_downloads (
            id TEXT PRIMARY KEY,
            model_id TEXT NOT NULL,
            status TEXT NOT NULL,
            progress REAL NOT NULL DEFAULT 0,
            bytes_downloaded INTEGER NOT NULL DEFAULT 0,
            total_bytes INTEGER NOT NULL,
            target_path TEXT NOT NULL,
            cancel_requested_at TEXT,
            error_message TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            completed_at TEXT
         );
         CREATE INDEX idx_model_downloads_model ON model_downloads(model_id, created_at);",
    )?;
    Ok(())
}

fn migration_6_worker_recovery(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE export_jobs ADD COLUMN worker_pid INTEGER;
         ALTER TABLE model_downloads ADD COLUMN worker_pid INTEGER;",
    )?;
    Ok(())
}

fn migration_7_word_timings(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE words (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            segment_id TEXT NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
            start_seconds REAL NOT NULL,
            end_seconds REAL NOT NULL,
            text TEXT NOT NULL,
            confidence REAL,
            ordinal INTEGER NOT NULL
         );
         CREATE INDEX idx_words_project_time ON words(project_id, start_seconds, ordinal);
         CREATE INDEX idx_words_segment ON words(segment_id, ordinal);",
    )?;
    Ok(())
}

fn migration_8_align_segments_to_words(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "UPDATE segments
         SET start_seconds = (SELECT MIN(words.start_seconds) FROM words WHERE words.segment_id = segments.id),
             end_seconds = (SELECT MAX(words.end_seconds) FROM words WHERE words.segment_id = segments.id)
         WHERE EXISTS (SELECT 1 FROM words WHERE words.segment_id = segments.id);",
    )?;
    Ok(())
}

fn migration_9_canvas_and_subtitle_mode(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE projects ADD COLUMN canvas_aspect_ratio TEXT NOT NULL DEFAULT 'source';
         ALTER TABLE projects ADD COLUMN canvas_framing TEXT NOT NULL DEFAULT 'contain-blur';
         ALTER TABLE export_jobs ADD COLUMN subtitle_mode TEXT NOT NULL DEFAULT 'source';
         UPDATE export_jobs
         SET subtitle_mode = CASE
             WHEN bilingual = 1 THEN 'bilingual'
             WHEN language IS NOT NULL THEN 'translated'
             ELSE 'source'
         END;",
    )?;
    Ok(())
}

fn migration_10_export_canvas_snapshot(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE export_jobs ADD COLUMN canvas_aspect_ratio TEXT NOT NULL DEFAULT 'source';
         ALTER TABLE export_jobs ADD COLUMN canvas_framing TEXT NOT NULL DEFAULT 'contain-blur';",
    )?;
    Ok(())
}

fn migration_11_word_range_cuts(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE word_range_cuts (
            edit_id TEXT PRIMARY KEY REFERENCES edits(id) ON DELETE CASCADE,
            from_word_id TEXT NOT NULL REFERENCES words(id),
            to_word_id TEXT NOT NULL REFERENCES words(id),
            selected_start_seconds REAL NOT NULL,
            selected_end_seconds REAL NOT NULL,
            padding_ms INTEGER NOT NULL,
            transcript_hash TEXT NOT NULL,
            stale INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX idx_word_range_cuts_words ON word_range_cuts(from_word_id,to_word_id);",
    )?;
    Ok(())
}

fn migration_12_project_history_cursor(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE versions ADD COLUMN history_index INTEGER;
         UPDATE versions AS current
         SET history_index = 1 + (
             SELECT COUNT(*) FROM versions AS earlier
             WHERE earlier.project_id=current.project_id
               AND (earlier.created_at < current.created_at
                    OR (earlier.created_at=current.created_at AND earlier.rowid < current.rowid))
         );
         CREATE UNIQUE INDEX idx_versions_project_history
             ON versions(project_id,history_index);
         CREATE TABLE project_history (
             project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
             cursor_index INTEGER NOT NULL,
             updated_at TEXT NOT NULL
         );
         INSERT INTO project_history(project_id,cursor_index,updated_at)
         SELECT project_id,MAX(history_index),CURRENT_TIMESTAMP
         FROM versions GROUP BY project_id;",
    )?;
    Ok(())
}

fn migration_13_cut_suggestions(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE cut_suggestions (
             edit_id TEXT PRIMARY KEY REFERENCES edits(id) ON DELETE CASCADE,
             suggestion_type TEXT NOT NULL CHECK(suggestion_type IN ('standalone_filler','adjacent_repetition','speech_restart')),
             confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
             detector_version TEXT NOT NULL
         );
         CREATE INDEX idx_cut_suggestions_type ON cut_suggestions(suggestion_type);",
    )?;
    Ok(())
}

fn migration_14_source_imports(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE source_imports (
             id TEXT PRIMARY KEY,
             project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
             original_url TEXT NOT NULL,
             webpage_url TEXT NOT NULL,
             site_media_id TEXT NOT NULL,
             extractor TEXT NOT NULL,
             title TEXT NOT NULL,
             duration_seconds REAL NOT NULL,
             file_size_bytes INTEGER,
             status TEXT NOT NULL,
             progress REAL NOT NULL DEFAULT 0,
             bytes_downloaded INTEGER NOT NULL DEFAULT 0,
             total_bytes INTEGER,
             output_directory TEXT NOT NULL,
             output_path TEXT,
             output_sha256 TEXT,
             tool_version TEXT NOT NULL,
             tool_sha256 TEXT NOT NULL,
             cancel_requested_at TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX idx_source_imports_status ON source_imports(status, created_at);
         CREATE INDEX idx_source_imports_project ON source_imports(project_id);",
    )?;
    Ok(())
}

fn migration_15_auto_workflows(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE auto_workflows (
             id TEXT PRIMARY KEY,
             input_kind TEXT NOT NULL CHECK(input_kind IN ('local','url')),
             input_value TEXT NOT NULL,
             title TEXT,
             confirmed_media_id TEXT,
             project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
             source_import_id TEXT REFERENCES source_imports(id) ON DELETE SET NULL,
             model_path TEXT NOT NULL,
             transcribe_language TEXT,
             translation_language TEXT,
             output_path TEXT NOT NULL,
             burn_subtitles INTEGER NOT NULL DEFAULT 0,
             subtitle_mode TEXT NOT NULL CHECK(subtitle_mode IN ('source','translated','bilingual')),
             status TEXT NOT NULL CHECK(status IN ('queued','running','needs_agent','needs_review','failed','interrupted','cancelled','completed')),
             current_stage TEXT NOT NULL CHECK(current_stage IN ('import','transcribe','suggestions','translate','review','audit','export','complete')),
             progress REAL NOT NULL DEFAULT 0,
             transcript_version_id TEXT REFERENCES versions(id) ON DELETE SET NULL,
             agent_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
             export_job_id TEXT REFERENCES export_jobs(id) ON DELETE SET NULL,
             audit_json TEXT,
             cancel_requested_at TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX idx_auto_workflows_status ON auto_workflows(status, created_at);
         CREATE INDEX idx_auto_workflows_project ON auto_workflows(project_id);
         CREATE TABLE auto_workflow_events (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             workflow_id TEXT NOT NULL REFERENCES auto_workflows(id) ON DELETE CASCADE,
             stage TEXT NOT NULL,
             status TEXT NOT NULL,
             progress REAL NOT NULL,
             message TEXT NOT NULL,
             created_at TEXT NOT NULL
         );
         CREATE INDEX idx_auto_workflow_events_workflow ON auto_workflow_events(workflow_id, id);",
    )?;
    Ok(())
}

fn migration_16_audio_analysis_jobs(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE audio_analysis_jobs (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             status TEXT NOT NULL CHECK(status IN ('queued','running','cancelled','interrupted','failed','completed')),
             progress REAL NOT NULL DEFAULT 0,
             report_json TEXT,
             cancel_requested_at TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX idx_audio_analysis_project ON audio_analysis_jobs(project_id, created_at);
         CREATE INDEX idx_audio_analysis_status ON audio_analysis_jobs(status, created_at);",
    )?;
    Ok(())
}

fn migration_17_speaker_tracks(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE speaker_tracks (
             project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
             status TEXT NOT NULL CHECK(status IN ('ready','no_speech')),
             runtime_version TEXT NOT NULL,
             segmentation_model TEXT NOT NULL,
             embedding_model TEXT NOT NULL,
             generated_at TEXT NOT NULL
         );
         CREATE TABLE speakers (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             source_label TEXT NOT NULL,
             label TEXT NOT NULL,
             color_index INTEGER NOT NULL DEFAULT 0,
             created_at TEXT NOT NULL,
             UNIQUE(project_id, source_label)
         );
         CREATE INDEX idx_speakers_project ON speakers(project_id, color_index);
         CREATE TABLE speaker_turns (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             speaker_id TEXT NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
             start_seconds REAL NOT NULL CHECK(start_seconds >= 0),
             end_seconds REAL NOT NULL CHECK(end_seconds > start_seconds),
             confidence REAL CHECK(confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
             source TEXT NOT NULL,
             model_version TEXT NOT NULL,
             created_at TEXT NOT NULL
         );
         CREATE INDEX idx_speaker_turns_project ON speaker_turns(project_id, start_seconds);
         CREATE TABLE segment_speakers (
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             segment_id TEXT NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
             speaker_id TEXT NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
             source TEXT NOT NULL,
             confidence REAL CHECK(confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
             updated_at TEXT NOT NULL,
             PRIMARY KEY(project_id, segment_id)
         );
         CREATE INDEX idx_segment_speakers_speaker ON segment_speakers(speaker_id, segment_id);
         CREATE TABLE speaker_jobs (
             id TEXT PRIMARY KEY,
             kind TEXT NOT NULL CHECK(kind IN ('install','analyze')),
             project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
             status TEXT NOT NULL CHECK(status IN ('queued','running','cancelled','interrupted','failed','completed')),
             stage TEXT NOT NULL,
             progress REAL NOT NULL DEFAULT 0,
             bytes_downloaded INTEGER NOT NULL DEFAULT 0,
             total_bytes INTEGER NOT NULL DEFAULT 0,
             cancel_requested_at TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX idx_speaker_jobs_status ON speaker_jobs(status, created_at);
         CREATE INDEX idx_speaker_jobs_project ON speaker_jobs(project_id, created_at);",
    )?;
    Ok(())
}

fn migration_18_subtitle_style_snapshots(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE projects ADD COLUMN subtitle_style_json TEXT NOT NULL DEFAULT '{\"preset\":\"standard\",\"position\":\"bottom\"}';
         ALTER TABLE export_jobs ADD COLUMN subtitle_style_json TEXT NOT NULL DEFAULT '{\"preset\":\"standard\",\"position\":\"bottom\"}';",
    )?;
    Ok(())
}

fn migration_19_instruction_locales(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE tasks ADD COLUMN instruction_locale TEXT NOT NULL DEFAULT 'zh-CN';
         ALTER TABLE workflows ADD COLUMN instruction_locale TEXT NOT NULL DEFAULT 'zh-CN';
         ALTER TABLE auto_workflows ADD COLUMN instruction_locale TEXT NOT NULL DEFAULT 'zh-CN';",
    )?;
    Ok(())
}

fn migration_20_transcription_providers(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE transcription_provider_config (
             provider_id TEXT PRIMARY KEY,
             endpoint TEXT NOT NULL,
             model_id TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );
         INSERT INTO transcription_provider_config(provider_id,endpoint,model_id,updated_at)
         VALUES('moss_openai','http://127.0.0.1:8000','OpenMOSS-Team/MOSS-Transcribe-Diarize','migration');
         CREATE TABLE transcription_jobs (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             provider_id TEXT NOT NULL,
             endpoint TEXT NOT NULL,
             model_id TEXT NOT NULL,
             language TEXT,
             prompt TEXT,
             hotwords_json TEXT NOT NULL DEFAULT '[]',
             status TEXT NOT NULL CHECK(status IN ('queued','running','finalizing','cancelled','interrupted','failed','completed')),
             stage TEXT NOT NULL,
             result_run_id TEXT,
             cancel_requested_at TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX idx_transcription_jobs_project ON transcription_jobs(project_id,created_at);
         CREATE INDEX idx_transcription_jobs_status ON transcription_jobs(status,created_at);
         CREATE TABLE transcription_runs (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             job_id TEXT NOT NULL UNIQUE REFERENCES transcription_jobs(id) ON DELETE CASCADE,
             provider_id TEXT NOT NULL,
             model_id TEXT NOT NULL,
             source_sha256 TEXT NOT NULL,
             result_sha256 TEXT NOT NULL,
             raw_result_path TEXT NOT NULL,
             segment_count INTEGER NOT NULL,
             speaker_count INTEGER NOT NULL,
             has_word_timings INTEGER NOT NULL DEFAULT 0,
             created_at TEXT NOT NULL
         );
         CREATE INDEX idx_transcription_runs_project ON transcription_runs(project_id,created_at);
         CREATE TABLE transcription_review_items (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             run_id TEXT NOT NULL REFERENCES transcription_runs(id) ON DELETE CASCADE,
             segment_id TEXT REFERENCES segments(id) ON DELETE CASCADE,
             severity TEXT NOT NULL CHECK(severity IN ('info','warning','error')),
             kind TEXT NOT NULL,
             message TEXT NOT NULL,
             status TEXT NOT NULL CHECK(status IN ('open','resolved','ignored')),
             created_at TEXT NOT NULL,
             resolved_at TEXT
         );
         CREATE INDEX idx_transcription_review_project ON transcription_review_items(project_id,status,severity);
         ALTER TABLE speaker_tracks ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'legacy_diarization';
         ALTER TABLE speaker_tracks ADD COLUMN model_id TEXT NOT NULL DEFAULT '';
         ALTER TABLE speaker_tracks ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'cascade';",
    )?;
    Ok(())
}

fn migration_21_transcription_consistency(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE transcription_jobs_v21 (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             provider_id TEXT NOT NULL,
             endpoint TEXT NOT NULL,
             model_id TEXT NOT NULL,
             language TEXT,
             prompt TEXT,
             hotwords_json TEXT NOT NULL DEFAULT '[]',
             status TEXT NOT NULL CHECK(status IN ('queued','running','finalizing','awaiting_apply','cancelled','interrupted','failed','completed','discarded')),
             stage TEXT NOT NULL,
             result_run_id TEXT,
             base_version_id TEXT,
             source_sha256 TEXT,
             input_audio_sha256 TEXT,
             cancel_requested_at TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         INSERT INTO transcription_jobs_v21(
             id,project_id,provider_id,endpoint,model_id,language,prompt,hotwords_json,
             status,stage,result_run_id,base_version_id,source_sha256,input_audio_sha256,
             cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,attempt_count
         )
         SELECT
             id,project_id,provider_id,endpoint,model_id,language,prompt,hotwords_json,
             CASE
                 WHEN status IN ('queued','running','finalizing') AND id NOT IN (
                     SELECT id FROM (
                         SELECT id,ROW_NUMBER() OVER (
                             PARTITION BY project_id
                             ORDER BY CASE status WHEN 'finalizing' THEN 0 WHEN 'running' THEN 1 ELSE 2 END,
                                      updated_at DESC,id DESC
                         ) AS position
                         FROM transcription_jobs
                         WHERE status IN ('queued','running','finalizing')
                     ) WHERE position=1
                 ) THEN 'interrupted'
                 ELSE status
             END,
             CASE
                 WHEN status IN ('queued','running','finalizing') AND id NOT IN (
                     SELECT id FROM (
                         SELECT id,ROW_NUMBER() OVER (
                             PARTITION BY project_id
                             ORDER BY CASE status WHEN 'finalizing' THEN 0 WHEN 'running' THEN 1 ELSE 2 END,
                                      updated_at DESC,id DESC
                         ) AS position
                         FROM transcription_jobs
                         WHERE status IN ('queued','running','finalizing')
                     ) WHERE position=1
                 ) THEN 'interrupted'
                 ELSE stage
             END,
             result_run_id,NULL,NULL,NULL,cancel_requested_at,
             CASE
                 WHEN status IN ('queued','running','finalizing') AND id NOT IN (
                     SELECT id FROM (
                         SELECT id,ROW_NUMBER() OVER (
                             PARTITION BY project_id
                             ORDER BY CASE status WHEN 'finalizing' THEN 0 WHEN 'running' THEN 1 ELSE 2 END,
                                      updated_at DESC,id DESC
                         ) AS position
                         FROM transcription_jobs
                         WHERE status IN ('queued','running','finalizing')
                     ) WHERE position=1
                 ) THEN 'transcription_interrupted: 数据库升级时发现同一项目存在多个活动转写任务。'
                 ELSE error_message
             END,
             created_at,updated_at,completed_at,
             CASE
                 WHEN status IN ('queued','running','finalizing') AND id NOT IN (
                     SELECT id FROM (
                         SELECT id,ROW_NUMBER() OVER (
                             PARTITION BY project_id
                             ORDER BY CASE status WHEN 'finalizing' THEN 0 WHEN 'running' THEN 1 ELSE 2 END,
                                      updated_at DESC,id DESC
                         ) AS position
                         FROM transcription_jobs
                         WHERE status IN ('queued','running','finalizing')
                     ) WHERE position=1
                 ) THEN NULL
                 ELSE worker_pid
             END,
             attempt_count
         FROM transcription_jobs;

         CREATE TABLE transcription_runs_v21 (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             job_id TEXT NOT NULL UNIQUE REFERENCES transcription_jobs_v21(id) ON DELETE CASCADE,
             provider_id TEXT NOT NULL,
             model_id TEXT NOT NULL,
             status TEXT NOT NULL DEFAULT 'applied' CHECK(status IN ('prepared','applied','discarded')),
             base_version_id TEXT,
             source_sha256 TEXT NOT NULL,
             input_audio_sha256 TEXT,
             result_sha256 TEXT NOT NULL,
             raw_result_path TEXT NOT NULL,
             segment_count INTEGER NOT NULL,
             speaker_count INTEGER NOT NULL,
             has_word_timings INTEGER NOT NULL DEFAULT 0,
             applied_version_id TEXT,
             created_at TEXT NOT NULL
         );
         INSERT INTO transcription_runs_v21(
             id,project_id,job_id,provider_id,model_id,status,base_version_id,source_sha256,
             input_audio_sha256,result_sha256,raw_result_path,segment_count,speaker_count,
             has_word_timings,applied_version_id,created_at
         )
         SELECT id,project_id,job_id,provider_id,model_id,'applied',NULL,source_sha256,NULL,
                result_sha256,raw_result_path,segment_count,speaker_count,has_word_timings,NULL,created_at
         FROM transcription_runs;

         CREATE TABLE transcription_review_items_v21 (
             id TEXT PRIMARY KEY,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             run_id TEXT NOT NULL REFERENCES transcription_runs_v21(id) ON DELETE CASCADE,
             segment_id TEXT REFERENCES segments(id) ON DELETE CASCADE,
             severity TEXT NOT NULL CHECK(severity IN ('info','warning','error')),
             kind TEXT NOT NULL,
             message TEXT NOT NULL,
             status TEXT NOT NULL CHECK(status IN ('open','resolved','ignored')),
             created_at TEXT NOT NULL,
             resolved_at TEXT
         );
         INSERT INTO transcription_review_items_v21
         SELECT * FROM transcription_review_items;

         DROP TABLE transcription_review_items;
         DROP TABLE transcription_runs;
         DROP TABLE transcription_jobs;
         ALTER TABLE transcription_jobs_v21 RENAME TO transcription_jobs;
         ALTER TABLE transcription_runs_v21 RENAME TO transcription_runs;
         ALTER TABLE transcription_review_items_v21 RENAME TO transcription_review_items;

         CREATE INDEX idx_transcription_jobs_project ON transcription_jobs(project_id,created_at);
         CREATE INDEX idx_transcription_jobs_status ON transcription_jobs(status,created_at);
         CREATE UNIQUE INDEX idx_transcription_jobs_one_active
             ON transcription_jobs(project_id)
             WHERE status IN ('queued','running','finalizing','awaiting_apply');
         CREATE INDEX idx_transcription_runs_project ON transcription_runs(project_id,created_at);
         CREATE INDEX idx_transcription_review_project ON transcription_review_items(project_id,status,severity);",
    )?;
    Ok(())
}

fn migration_22_transcription_candidate_summary(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE transcription_runs ADD COLUMN duration_seconds REAL;
         ALTER TABLE transcription_runs ADD COLUMN warning_count INTEGER NOT NULL DEFAULT 0;",
    )?;
    Ok(())
}

fn migration_23_agent_runs(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE agent_runs (
             id TEXT PRIMARY KEY,
             task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id) ON DELETE CASCADE,
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             provider TEXT NOT NULL DEFAULT 'codex',
             status TEXT NOT NULL CHECK(status IN ('queued','running','submitting','completed','cancelled','interrupted','failed')),
             base_version_id TEXT NOT NULL,
             progress REAL NOT NULL DEFAULT 0,
             current_batch INTEGER NOT NULL DEFAULT 0,
             batch_count INTEGER NOT NULL DEFAULT 0,
             timeout_seconds INTEGER NOT NULL,
             cli_version TEXT,
             auth_mode TEXT,
             codex_thread_id TEXT,
             cancel_requested_at TEXT,
             error_code TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             started_at TEXT,
             completed_at TEXT,
             worker_pid INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX idx_agent_runs_project ON agent_runs(project_id,created_at);
         CREATE INDEX idx_agent_runs_status ON agent_runs(status,updated_at);
         CREATE TABLE agent_run_batches (
             id TEXT PRIMARY KEY,
             run_id TEXT NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
             ordinal INTEGER NOT NULL,
             status TEXT NOT NULL CHECK(status IN ('queued','running','completed','cancelled','failed')),
             segment_ids_json TEXT NOT NULL,
             result_json TEXT,
             codex_thread_id TEXT,
             error_code TEXT,
             error_message TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             started_at TEXT,
             completed_at TEXT,
             attempt_count INTEGER NOT NULL DEFAULT 0,
             UNIQUE(run_id,ordinal)
         );
         CREATE INDEX idx_agent_run_batches_run ON agent_run_batches(run_id,ordinal);",
    )?;
    Ok(())
}

fn migration_24_translation_readiness(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE translations ADD COLUMN glossary_version INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE translation_segments ADD COLUMN source_hash TEXT NOT NULL DEFAULT '';
         ALTER TABLE translation_segments ADD COLUMN status TEXT NOT NULL DEFAULT 'stale';
         ALTER TABLE translation_segments ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
         UPDATE translation_segments SET status='stale',source_hash='',updated_at='migration-24';
         UPDATE translations SET status='stale' WHERE EXISTS (
             SELECT 1 FROM translation_segments s
             WHERE s.project_id=translations.project_id AND s.language=translations.language
         );
         CREATE TABLE project_glossaries (
             project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
             current_version INTEGER NOT NULL DEFAULT 0,
             updated_at TEXT NOT NULL
         );
         INSERT INTO project_glossaries(project_id,current_version,updated_at)
             SELECT id,0,'migration-24' FROM projects;
         CREATE TABLE glossary_versions (
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             version INTEGER NOT NULL,
             created_at TEXT NOT NULL,
             PRIMARY KEY(project_id,version)
         );
         INSERT INTO glossary_versions(project_id,version,created_at)
             SELECT id,0,'migration-24' FROM projects;
         CREATE TABLE glossary_entries (
             project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
             version INTEGER NOT NULL,
             language TEXT NOT NULL,
             source TEXT NOT NULL,
             target TEXT NOT NULL,
             ordinal INTEGER NOT NULL,
             PRIMARY KEY(project_id,version,language,source)
         );
         CREATE INDEX idx_glossary_entries_current
             ON glossary_entries(project_id,version,language,ordinal);
         ALTER TABLE tasks ADD COLUMN glossary_version INTEGER;
         CREATE TABLE task_segments (
             task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
             segment_id TEXT NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
             source_hash TEXT NOT NULL,
             ordinal INTEGER NOT NULL,
             PRIMARY KEY(task_id,segment_id)
         );
         CREATE INDEX idx_task_segments_task ON task_segments(task_id,ordinal);
         ALTER TABLE export_jobs ADD COLUMN allow_stale_translation INTEGER NOT NULL DEFAULT 0;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn initializes_and_reopens_current_schema() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("siaocut.db");
        let first = open_at(&path).unwrap();
        let version: i64 = first
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        drop(first);

        let second = open_at(&path).unwrap();
        let migration_count: i64 = second
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(migration_count, MIGRATIONS.len() as i64);
        let word_range_table: bool = second
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='word_range_cuts')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(word_range_table);
        let history_table: bool = second
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='project_history')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(history_table);
        let agent_run_tables: i64 = second
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('agent_runs','agent_run_batches')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(agent_run_tables, 2);
        second
            .execute(
                "INSERT INTO projects(id,title,created_at,updated_at) VALUES('p-locale','Legacy','now','now')",
                [],
            )
            .unwrap();
        second
            .execute(
                "INSERT INTO tasks(id,project_id,kind,status,created_at) VALUES('t-locale','p-locale','polish','queued','now')",
                [],
            )
            .unwrap();
        let locale: String = second
            .query_row(
                "SELECT instruction_locale FROM tasks WHERE id='t-locale'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(locale, "zh-CN");
        let suggestion_table: bool = second
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cut_suggestions')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(suggestion_table);
        let source_import_table: bool = second
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_imports')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(source_import_table);
        let auto_workflow_table: bool = second
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='auto_workflows')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(auto_workflow_table);
        let project_style_default: String = second
            .query_row(
                "SELECT dflt_value FROM pragma_table_info('projects') WHERE name='subtitle_style_json'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(project_style_default.contains("standard"));
        let export_style_column: bool = second
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('export_jobs') WHERE name='subtitle_style_json')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(export_style_column);
    }

    #[test]
    fn migration_24_marks_legacy_translations_stale_and_preserves_their_text() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("schema-23.db");
        let mut database = Connection::open(&path).unwrap();
        database.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
        )
        .unwrap();
        for migration in MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= 23)
        {
            let tx = database.transaction().unwrap();
            (migration.apply)(&tx).unwrap();
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,'test')",
                [migration.version],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        database
            .execute(
                "INSERT INTO projects(id,title,created_at,updated_at) VALUES('p','legacy','now','now')",
                [],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text) VALUES('s','p',0,1,'source')",
                [],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO translations(project_id,language,status,updated_at) VALUES('p','en','current','now')",
                [],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO translation_segments(project_id,language,segment_id,text) VALUES('p','en','s','legacy translation')",
                [],
            )
            .unwrap();
        drop(database);

        let migrated = open_at(&path).unwrap();
        let backup_path = path.with_file_name("schema-23.db.schema-23.bak");
        assert!(backup_path.is_file());
        let backup = Connection::open(&backup_path).unwrap();
        let backup_version: i64 = backup
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(backup_version, 23);
        let backup_translation: String = backup
            .query_row(
                "SELECT text FROM translation_segments WHERE project_id='p' AND language='en' AND segment_id='s'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(backup_translation, "legacy translation");
        let row: (String, String, String, String) = migrated
            .query_row(
                "SELECT text,source_hash,status,updated_at FROM translation_segments WHERE project_id='p' AND language='en' AND segment_id='s'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row.0, "legacy translation");
        assert!(row.1.is_empty());
        assert_eq!(row.2, "stale");
        assert_eq!(row.3, "migration-24");
        let glossary_version: i64 = migrated
            .query_row(
                "SELECT current_version FROM project_glossaries WHERE project_id='p'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(glossary_version, 0);
    }

    #[test]
    fn migration_21_reconciles_duplicate_active_transcriptions_and_enforces_uniqueness() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("schema-20.db");
        let mut database = Connection::open(&path).unwrap();
        database.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        database
            .execute_batch(
                "CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );",
            )
            .unwrap();
        for migration in MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= 20)
        {
            let tx = database.transaction().unwrap();
            (migration.apply)(&tx).unwrap();
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,'test')",
                [migration.version],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        database
            .execute(
                "INSERT INTO projects(id,title,created_at,updated_at) VALUES('p','P','now','now')",
                [],
            )
            .unwrap();
        for (id, status, updated_at) in [
            ("queued", "queued", "2026-01-01T00:00:00Z"),
            ("running", "running", "2026-01-01T00:01:00Z"),
        ] {
            database
                .execute(
                    "INSERT INTO transcription_jobs(
                        id,project_id,provider_id,endpoint,model_id,status,stage,created_at,updated_at
                     ) VALUES(?1,'p','moss_openai','http://127.0.0.1:8000','moss',?2,?2,'now',?3)",
                    params![id, status, updated_at],
                )
                .unwrap();
        }

        migrate(&mut database).unwrap();

        let active: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM transcription_jobs WHERE project_id='p' AND status IN ('queued','running','finalizing','awaiting_apply')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 1);
        let interrupted: String = database
            .query_row(
                "SELECT status FROM transcription_jobs WHERE id='queued'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(interrupted, "interrupted");
        let duplicate = database.execute(
            "INSERT INTO transcription_jobs(
                id,project_id,provider_id,endpoint,model_id,status,stage,created_at,updated_at
             ) VALUES('duplicate','p','moss_openai','http://127.0.0.1:8000','moss','queued','queued','now','now')",
            [],
        );
        assert!(duplicate.is_err());
    }

    #[test]
    fn rejects_database_from_newer_core() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("newer.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations(version, applied_at) VALUES(999, 'future');",
        )
        .unwrap();
        drop(db);

        let error = open_at(&path).unwrap_err().to_string();
        assert!(error.contains("database_version_unsupported"));
        assert!(error.contains("999"));
    }

    #[test]
    fn migration_seeds_history_cursor_for_existing_versions() {
        let temp = tempdir().unwrap();
        let mut db = Connection::open(temp.path().join("history-upgrade.db")).unwrap();
        for migration in MIGRATIONS.iter().filter(|migration| migration.version < 12) {
            let tx = db.transaction().unwrap();
            (migration.apply)(&tx).unwrap();
            tx.commit().unwrap();
        }
        db.execute(
            "INSERT INTO projects(id,title,created_at,updated_at,source_language) VALUES('p','test','2026-01-01','2026-01-01','en')",
            [],
        )
        .unwrap();
        for (id, created_at) in [
            ("v1", "2026-01-01T00:00:00Z"),
            ("v2", "2026-01-01T00:00:01Z"),
        ] {
            db.execute(
                "INSERT INTO versions(id,project_id,reason,created_at,snapshot_json) VALUES(?1,'p','test',?2,'{}')",
                params![id, created_at],
            )
            .unwrap();
        }
        let tx = db.transaction().unwrap();
        migration_12_project_history_cursor(&tx).unwrap();
        tx.commit().unwrap();

        let indexes = db
            .prepare(
                "SELECT history_index FROM versions WHERE project_id='p' ORDER BY history_index",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(indexes, vec![1, 2]);
        let cursor: i64 = db
            .query_row(
                "SELECT cursor_index FROM project_history WHERE project_id='p'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cursor, 2);
    }

    #[test]
    fn migrates_resumable_task_columns() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("tasks.db");
        let db = open_at(&path).unwrap();
        let columns = db
            .prepare("PRAGMA table_info(tasks)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(columns.contains(&"base_version_id".to_owned()));
        assert!(columns.contains(&"cancel_requested_at".to_owned()));
        let events_exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='task_events')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(events_exists);
    }

    #[test]
    fn migrates_reviewable_agent_patch_tables() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("patches.db")).unwrap();
        for table in ["workflows", "agent_patch_sets", "agent_patch_items"] {
            let exists: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing {table}");
        }
    }

    #[test]
    fn migrates_resumable_auto_workflow_tables() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("auto-workflows.db")).unwrap();
        for table in ["auto_workflows", "auto_workflow_events"] {
            let exists: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing {table}");
        }
        let columns = db
            .prepare("PRAGMA table_info(auto_workflows)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for required in [
            "current_stage",
            "source_import_id",
            "transcript_version_id",
            "agent_task_id",
            "export_job_id",
            "worker_pid",
            "attempt_count",
        ] {
            assert!(columns.contains(&required.to_owned()), "missing {required}");
        }
    }

    #[test]
    fn migrates_media_artifact_and_export_job_tables() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("media-pipeline.db")).unwrap();
        for table in ["media_artifacts", "export_jobs"] {
            let exists: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing {table}");
        }
    }

    #[test]
    fn migrates_audio_analysis_job_table() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("audio-analysis.db")).unwrap();
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='audio_analysis_jobs')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn migrates_model_download_table() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("models.db")).unwrap();
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='model_downloads')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn migrates_word_timing_table() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("words.db")).unwrap();
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='words')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn migration_aligns_existing_segments_to_their_word_evidence() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("word-alignment.db");
        let mut db = open_at(&path).unwrap();
        db.execute(
            "INSERT INTO projects(id,title,created_at,updated_at) VALUES('p','test','now','now')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text) VALUES('s','p',5.0,9.0,'hello')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES('w','p','s',1.25,2.5,'hello',0)",
            [],
        )
        .unwrap();
        let tx = db.transaction().unwrap();
        migration_8_align_segments_to_words(&tx).unwrap();
        tx.commit().unwrap();
        let range: (f64, f64) = db
            .query_row(
                "SELECT start_seconds,end_seconds FROM segments WHERE id='s'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(range, (1.25, 2.5));
    }

    #[test]
    fn migrates_canvas_and_subtitle_mode_columns_with_safe_defaults() {
        let temp = tempdir().unwrap();
        let db = open_at(&temp.path().join("canvas.db")).unwrap();
        db.execute(
            "INSERT INTO projects(id,title,created_at,updated_at) VALUES('p','test','now','now')",
            [],
        )
        .unwrap();
        let canvas: (String, String) = db
            .query_row(
                "SELECT canvas_aspect_ratio,canvas_framing FROM projects WHERE id='p'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(canvas, ("source".into(), "contain-blur".into()));

        let export_columns = db
            .prepare("PRAGMA table_info(export_jobs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for column in ["subtitle_mode", "canvas_aspect_ratio", "canvas_framing"] {
            assert!(
                export_columns.contains(&column.to_owned()),
                "missing {column}"
            );
        }
    }
}
