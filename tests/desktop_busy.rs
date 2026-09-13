use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn call(home: &Path, args: &[&str], direct: bool) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_siaocut-core"));
    command
        .env("SIAOCUT_HOME", home)
        .env("SIAOCUT_RESOURCE_CONFIG_HOME", home.join("resources"))
        .env("SIAOCUT_SERVICE_IDLE_MS", "100")
        .args(["--json"])
        .args(args);
    if direct {
        command.env("SIAOCUT_DIRECT", "1");
    } else {
        command.env_remove("SIAOCUT_DIRECT");
    }
    let output = command.output().unwrap();
    serde_json::from_slice(if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    })
    .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)))
}
fn desktop(home: &Path, request: Value, direct: bool) -> Value {
    let path = home.join("request.json");
    fs::write(&path, request.to_string()).unwrap();
    call(home, &["desktop-request", path.to_str().unwrap()], direct)
}
fn seed(home: &Path) -> Value {
    let media = home.join("source.wav");
    fs::write(&media, b"test media").unwrap();
    let p = call(home, &["import", media.to_str().unwrap()], true)["project"].clone();
    let id = p["id"].as_str().unwrap();
    assert_eq!(
        call(
            home,
            &[
                "transcript",
                "add",
                id,
                "--start",
                "0",
                "--end",
                "1",
                "--text",
                "original"
            ],
            true
        )["status"],
        "ok"
    );
    call(home, &["project", "show", id], true)["project"].clone()
}

#[test]
fn locked_desktop_save_returns_busy_promptly_and_retries_without_losing_drafts() {
    for direct in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let p = seed(home);
        let id = p["id"].as_str().unwrap();
        let draft = json!({"projectId":id,"sessionId":"test","segmentId":p["transcript"]["segments"][0]["id"],"field":"source","baseVersionId":p["history"]["currentVersionId"],"baseText":"original","text":"retained draft","revision":1});
        assert_eq!(
            desktop(
                home,
                json!({"kind":"editing","request":{"action":"journal","draft":draft}}),
                direct
            )["status"],
            "ok"
        );
        let request = json!({"kind":"editing","request":{"action":"save","edit":{"mutationId":"same-save","groupId":"group","expectedVersionId":p["history"]["currentVersionId"],"draft":draft}}});
        let lock = Connection::open(home.join("siaocut.db")).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let start = Instant::now();
        let failure = desktop(home, request.clone(), direct);
        assert!(start.elapsed() < Duration::from_secs(6));
        assert_eq!(failure["error"]["code"], "database_busy");
        assert!(
            failure["error"]["message"]
                .as_str()
                .unwrap()
                .contains("稍后重试")
        );
        assert!(
            !failure["error"]["message"]
                .as_str()
                .unwrap()
                .contains("草稿已")
        );
        lock.execute_batch("ROLLBACK").unwrap();
        assert_eq!(
            call(home, &["project", "show", id], direct)["project"]["transcript"]["segments"][0]["text"],
            "original"
        );
        let drafts = desktop(
            home,
            json!({"kind":"editing","request":{"action":"list","projectId":id}}),
            direct,
        );
        assert_eq!(drafts["drafts"][0]["text"], "retained draft");
        let saved = desktop(home, request.clone(), direct);
        assert_eq!(saved["status"], "ok");
        assert_eq!(
            desktop(home, request, direct)["editReceipt"],
            saved["editReceipt"]
        );
    }
}

#[test]
fn migration_lock_also_has_a_bounded_desktop_wait() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let p = seed(home);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(home.join("siaocut.db.migration.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let request = json!({"kind":"editing","request":{"action":"list","projectId":p["id"]}});
    let start = Instant::now();
    let failure = desktop(home, request.clone(), true);
    assert!(start.elapsed() < Duration::from_secs(6));
    assert_eq!(failure["error"]["code"], "database_busy");
    fs2::FileExt::unlock(&lock).unwrap();
    assert_eq!(desktop(home, request, true)["status"], "ok");
}
