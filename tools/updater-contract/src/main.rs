use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tauri_plugin_updater::UpdaterExt;

const CURRENT_VERSION: &str = "0.1.1";
const TARGET: &str = "windows-x86_64";

struct FixtureServer {
    endpoint: String,
    thread: thread::JoinHandle<Result<usize>>,
    expected_requests: usize,
}

impl FixtureServer {
    fn spawn(
        version: &str,
        signature: &str,
        size: u64,
        sha256: &str,
        artifact: Arc<Vec<u8>>,
        expected_requests: usize,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let endpoint = format!("http://{address}/latest.json");
        let artifact_url = format!("http://{address}/artifact.exe");
        let manifest = serde_json::to_vec(&json!({
            "version": version,
            "notes": "Local signed updater contract fixture",
            "platforms": {
                TARGET: {
                    "url": artifact_url,
                    "signature": signature,
                    "size": size,
                    "sha256": sha256
                }
            }
        }))?;
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut served = 0;
            while served < expected_requests && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        serve_request(&mut stream, &manifest, &artifact)?;
                        served += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            ensure!(
                served == expected_requests,
                "local server received {served}/{expected_requests} expected requests"
            );
            Ok(served)
        });
        Ok(Self {
            endpoint,
            thread,
            expected_requests,
        })
    }

    fn finish(self) -> Result<usize> {
        let served = self
            .thread
            .join()
            .map_err(|_| anyhow!("local updater server thread panicked"))??;
        ensure!(served == self.expected_requests);
        Ok(served)
    }
}

fn serve_request(stream: &mut TcpStream, manifest: &[u8], artifact: &[u8]) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk)?;
        ensure!(
            read > 0,
            "updater request ended before its headers completed"
        );
        request.extend_from_slice(&chunk[..read]);
        ensure!(
            request.len() <= 16 * 1024,
            "updater request headers exceed 16 KiB"
        );
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let first_line = String::from_utf8_lossy(&request)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let body = if first_line.contains(" /latest.json ") {
        manifest
    } else if first_line.contains(" /artifact.exe ") {
        artifact
    } else {
        let response = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(response)?;
        bail!("unexpected updater request: {first_line}");
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn repository_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .context("cannot resolve repository root")
}

fn run_tauri_cli<I, S>(repository: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let cli = repository.join("apps/desktop/node_modules/@tauri-apps/cli/tauri.js");
    let output = Command::new("node.exe")
        .arg(cli)
        .args(args)
        .current_dir(repository)
        .stdin(Stdio::null())
        .output()
        .context("cannot execute the pinned Tauri CLI")?;
    if !output.status.success() {
        bail!(
            "Tauri CLI failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

fn generate_ephemeral_key(repository: &Path, directory: &Path) -> Result<(PathBuf, String)> {
    let private_key = directory.join("updater.key");
    run_tauri_cli(
        repository,
        [
            OsStr::new("signer"),
            OsStr::new("generate"),
            OsStr::new("--ci"),
            OsStr::new("--force"),
            OsStr::new("--password"),
            OsStr::new("local-contract-only"),
            OsStr::new("--write-keys"),
            private_key.as_os_str(),
        ],
    )?;
    let public_key = std::fs::read_to_string(format!("{}.pub", private_key.display()))
        .context("cannot read generated updater public key")?;
    Ok((private_key, public_key.trim().to_owned()))
}

fn sign_fixture(repository: &Path, private_key: &Path, fixture_path: &Path) -> Result<String> {
    run_tauri_cli(
        repository,
        [
            OsStr::new("signer"),
            OsStr::new("sign"),
            OsStr::new("--private-key-path"),
            private_key.as_os_str(),
            OsStr::new("--password"),
            OsStr::new("local-contract-only"),
            fixture_path.as_os_str(),
        ],
    )?;
    std::fs::read_to_string(format!("{}.sig", fixture_path.display()))
        .context("cannot read generated updater signature")
        .map(|value| value.trim().to_owned())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify_manifest_integrity(update: &tauri_plugin_updater::Update, bytes: &[u8]) -> Result<()> {
    let platform = update
        .raw_json
        .get("platforms")
        .and_then(Value::as_object)
        .and_then(|platforms| platforms.get(TARGET))
        .context("manifest does not contain the expected platform")?;
    let expected_size = platform
        .get("size")
        .and_then(Value::as_u64)
        .context("manifest does not contain size")?;
    let expected_sha256 = platform
        .get("sha256")
        .and_then(Value::as_str)
        .context("manifest does not contain SHA-256")?;
    ensure!(bytes.len() as u64 == expected_size, "size mismatch");
    ensure!(sha256(bytes) == expected_sha256, "SHA-256 mismatch");
    Ok(())
}

fn authenticode_status(path: &Path) -> Result<String> {
    let script = "Import-Module Microsoft.PowerShell.Security -ErrorAction Stop; $signature = Get-AuthenticodeSignature -LiteralPath $env:SIAOCUT_UPDATE_VERIFY_PATH; Write-Output $signature.Status";
    let windows_root =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()));
    let output = Command::new(windows_root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("SIAOCUT_UPDATE_VERIFY_PATH", path)
        .env(
            "PSModulePath",
            windows_root.join("System32/WindowsPowerShell/v1.0/Modules"),
        )
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "Authenticode command failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn verify_authenticode(bytes: &[u8]) -> Result<String> {
    let mut fixture = tempfile::Builder::new()
        .prefix("siaocut-update-contract-")
        .suffix(".exe")
        .tempfile()?;
    fixture.write_all(bytes)?;
    fixture.flush()?;
    fixture.as_file().sync_all()?;
    let fixture_path = fixture.into_temp_path();
    authenticode_status(&fixture_path)
}

async fn run_case(
    public_key: &str,
    version: &str,
    signature: &str,
    artifact: Arc<Vec<u8>>,
    manifest_sha256: &str,
    download: bool,
) -> Result<Value> {
    let expected_requests = if download { 2 } else { 1 };
    let server = FixtureServer::spawn(
        version,
        signature,
        artifact.len() as u64,
        manifest_sha256,
        Arc::clone(&artifact),
        expected_requests,
    )?;
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.package_info_mut().version = CURRENT_VERSION.parse()?;
    context.config_mut().plugins.0.insert(
        "updater".to_owned(),
        json!({
            "endpoints": [server.endpoint],
            "pubkey": public_key,
            "dangerousInsecureTransportProtocol": true
        }),
    );
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().target(TARGET).build())
        .build(context)?;
    let update = app
        .handle()
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()?
        .check()
        .await?;

    let mut result = json!({
        "updateAvailable": update.is_some(),
        "announcedVersion": update.as_ref().map(|candidate| candidate.version.clone()),
        "requests": expected_requests
    });
    if download {
        let update = update.context("expected a newer release")?;
        match update.download(|_, _| {}, || {}).await {
            Ok(bytes) => {
                result["tauriSignatureAccepted"] = json!(true);
                result["integrityAccepted"] =
                    json!(verify_manifest_integrity(&update, &bytes).is_ok());
                result["authenticodeStatus"] = json!(verify_authenticode(&bytes)?);
            }
            Err(error) => {
                result["tauriSignatureAccepted"] = json!(false);
                result["downloadError"] = json!(error.to_string());
            }
        }
    }
    let requests = server.finish()?;
    result["requests"] = json!(requests);
    Ok(result)
}

fn find_node() -> Result<PathBuf> {
    let output = Command::new("where.exe").arg("node.exe").output()?;
    ensure!(output.status.success(), "node.exe is required");
    String::from_utf8(output.stdout)?
        .lines()
        .next()
        .map(PathBuf::from)
        .context("where.exe returned no node.exe path")
}

#[tokio::main]
async fn main() -> Result<()> {
    let repository = repository_root()?;
    let temporary = tempfile::tempdir()?;
    let (private_key, public_key) = generate_ephemeral_key(&repository, temporary.path())?;

    let small_path = temporary.path().join("unsigned-fixture.exe");
    let small_bytes = b"SiaoCut local updater contract fixture\n".to_vec();
    std::fs::write(&small_path, &small_bytes)?;
    let small_signature = sign_fixture(&repository, &private_key, &small_path)?;

    let trusted_path = temporary.path().join("trusted-fixture.exe");
    std::fs::copy(find_node()?, &trusted_path)?;
    let trusted_signature = sign_fixture(&repository, &private_key, &trusted_path)?;
    let trusted_bytes = Arc::new(std::fs::read(&trusted_path)?);
    ensure!(authenticode_status(&trusted_path)? == "Valid");

    let small_bytes = Arc::new(small_bytes);
    let mut tampered = small_bytes.as_ref().clone();
    tampered.extend_from_slice(b"tampered");
    let tampered = Arc::new(tampered);
    let mut cases = BTreeMap::new();

    let higher = run_case(
        &public_key,
        "0.2.0",
        &trusted_signature,
        Arc::clone(&trusted_bytes),
        &sha256(&trusted_bytes),
        true,
    )
    .await?;
    ensure!(higher["updateAvailable"] == true);
    ensure!(higher["tauriSignatureAccepted"] == true);
    ensure!(higher["integrityAccepted"] == true);
    ensure!(higher["authenticodeStatus"] == "Valid");
    cases.insert("higherVersionAllGates".to_owned(), higher);

    let same = run_case(
        &public_key,
        CURRENT_VERSION,
        &small_signature,
        Arc::clone(&small_bytes),
        &sha256(&small_bytes),
        false,
    )
    .await?;
    ensure!(same["updateAvailable"] == false);
    cases.insert("sameVersionRejected".to_owned(), same);

    let downgrade = run_case(
        &public_key,
        "0.1.0",
        &small_signature,
        Arc::clone(&small_bytes),
        &sha256(&small_bytes),
        false,
    )
    .await?;
    ensure!(downgrade["updateAvailable"] == false);
    cases.insert("downgradeRejected".to_owned(), downgrade);

    let tamper = run_case(
        &public_key,
        "0.2.1",
        &small_signature,
        Arc::clone(&tampered),
        &sha256(&tampered),
        true,
    )
    .await?;
    ensure!(tamper["updateAvailable"] == true);
    ensure!(tamper["tauriSignatureAccepted"] == false);
    cases.insert("tamperedArtifactRejected".to_owned(), tamper);

    let bad_sha = run_case(
        &public_key,
        "0.2.1",
        &small_signature,
        Arc::clone(&small_bytes),
        &"0".repeat(64),
        true,
    )
    .await?;
    ensure!(bad_sha["tauriSignatureAccepted"] == true);
    ensure!(bad_sha["integrityAccepted"] == false);
    cases.insert("badSha256Rejected".to_owned(), bad_sha);

    let unsigned_tauri = run_case(
        &public_key,
        "0.2.1",
        "",
        Arc::clone(&small_bytes),
        &sha256(&small_bytes),
        true,
    )
    .await?;
    ensure!(unsigned_tauri["tauriSignatureAccepted"] == false);
    cases.insert("missingTauriSignatureRejected".to_owned(), unsigned_tauri);

    let unsigned_authenticode = run_case(
        &public_key,
        "0.2.1",
        &small_signature,
        Arc::clone(&small_bytes),
        &sha256(&small_bytes),
        true,
    )
    .await?;
    ensure!(unsigned_authenticode["tauriSignatureAccepted"] == true);
    ensure!(unsigned_authenticode["integrityAccepted"] == true);
    ensure!(unsigned_authenticode["authenticodeStatus"] != "Valid");
    cases.insert(
        "unsignedAuthenticodeRejected".to_owned(),
        unsigned_authenticode,
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "passed",
            "currentVersion": CURRENT_VERSION,
            "target": TARGET,
            "transport": "loopback HTTP enabled only in isolated test configuration",
            "ephemeralPrivateKeyPersisted": false,
            "cases": cases
        }))?
    );
    Ok(())
}
