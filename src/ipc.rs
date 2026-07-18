use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{env, io, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
    task::JoinSet,
    time::{sleep, timeout},
};

const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const START_RETRIES: usize = 40;

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    arguments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub exit_code: i32,
    pub output: String,
}

type Executor = Arc<dyn Fn(Vec<String>) -> Response + Send + Sync>;

enum ExchangeFailure {
    Retryable(anyhow::Error),
    Ambiguous(anyhow::Error),
}

impl ExchangeFailure {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Retryable(error) | Self::Ambiguous(error) => error,
        }
    }
}

pub fn pipe_name() -> String {
    let home = crate::db::home_dir().to_string_lossy().to_lowercase();
    let digest = format!("{:x}", Sha256::digest(home.as_bytes()));
    format!(r"\\.\pipe\siaocut-core-{}", &digest[..16])
}

pub async fn request(arguments: Vec<String>) -> Result<Response> {
    let pipe = pipe_name();
    if let Ok(client) = connect(&pipe).await {
        match exchange(client, arguments.clone()).await {
            Ok(response) => return Ok(response),
            Err(ExchangeFailure::Retryable(error)) if is_stale_pipe(&error) => {}
            Err(error) => return Err(error.into_error()),
        }
    }

    for attempt in 0..START_RETRIES {
        // A named pipe can remain connectable briefly while its process is
        // exiting. Periodically start a fresh service instead of trusting that
        // stale instance, while first_pipe_instance prevents duplicates.
        if attempt % 10 == 0 {
            start_service()?;
        }
        if let Ok(client) = connect(&pipe).await {
            match exchange(client, arguments.clone()).await {
                Ok(response) => return Ok(response),
                Err(ExchangeFailure::Retryable(error)) if is_stale_pipe(&error) => {}
                Err(error) => return Err(error.into_error()),
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    Err(anyhow!(
        "core_service_unavailable: 无法连接 SiaoCut Core 服务"
    ))
}

fn is_stale_pipe(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.raw_os_error() == Some(233))
    })
}

async fn connect(pipe: &str) -> Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    ClientOptions::new()
        .open(pipe)
        .with_context(|| format!("无法连接本机命名管道：{pipe}"))
}

async fn exchange(
    mut client: tokio::net::windows::named_pipe::NamedPipeClient,
    arguments: Vec<String>,
) -> Result<Response, ExchangeFailure> {
    let request = serde_json::to_string(&Request { arguments })
        .map_err(|error| ExchangeFailure::Ambiguous(error.into()))?;
    // A disconnected-pipe failure while writing the body or delimiter means
    // the server side is already gone, so the request was not handled. Once
    // both writes finish, every later error is ambiguous and must never replay
    // a mutating command.
    client
        .write_all(request.as_bytes())
        .await
        .map_err(|error| {
            ExchangeFailure::Retryable(
                anyhow::Error::new(error).context("core_pipe_body_write_failed"),
            )
        })?;
    client.write_all(b"\n").await.map_err(|error| {
        ExchangeFailure::Retryable(
            anyhow::Error::new(error).context("core_pipe_delimiter_write_failed"),
        )
    })?;
    client.flush().await.map_err(|error| {
        ExchangeFailure::Ambiguous(anyhow::Error::new(error).context("core_pipe_flush_failed"))
    })?;
    let mut response = String::new();
    let mut reader = BufReader::new(client);
    reader.read_line(&mut response).await.map_err(|error| {
        ExchangeFailure::Ambiguous(
            anyhow::Error::new(error).context("core_pipe_response_read_failed"),
        )
    })?;
    // Acknowledge receipt so the server does not disconnect while Windows is
    // still delivering the response buffer.
    let mut client = reader.into_inner();
    let _ = client.write_all(b"\n").await;
    if response.trim().is_empty() {
        return Err(ExchangeFailure::Ambiguous(anyhow!(
            "core_service_no_response: Core 服务未返回结果"
        )));
    }
    serde_json::from_str(response.trim()).map_err(|error| ExchangeFailure::Ambiguous(error.into()))
}

fn start_service() -> Result<()> {
    crate::util::spawn_detached_current(&["__service"]).context("无法启动 SiaoCut Core 服务")?;
    Ok(())
}

pub async fn serve() -> Result<()> {
    serve_with(pipe_name(), idle_timeout(), Arc::new(crate::execute_args)).await
}

async fn serve_with(pipe: String, idle: Duration, executor: Executor) -> Result<()> {
    let mut first_instance = true;
    let mut handlers = JoinSet::new();
    loop {
        let server = create_server(&pipe, first_instance)?;
        first_instance = false;
        let connected = if handlers.is_empty() {
            match timeout(idle, server.connect()).await {
                Ok(result) => Some(result),
                Err(_) => return Ok(()),
            }
        } else {
            tokio::select! {
                result = server.connect() => Some(result),
                completed = handlers.join_next() => {
                    report_handler_result(completed);
                    None
                }
            }
        };
        if let Some(result) = connected {
            result?;
            handlers.spawn(handle(server, Arc::clone(&executor)));
        }
    }
}

fn report_handler_result(
    completed: Option<std::result::Result<Result<()>, tokio::task::JoinError>>,
) {
    match completed {
        Some(Ok(Ok(()))) | None => {}
        Some(Ok(Err(error))) => eprintln!("SiaoCut Core request: {error:#}"),
        Some(Err(error)) => eprintln!("SiaoCut Core request task: {error}"),
    }
}

fn idle_timeout() -> Duration {
    env::var("SIAOCUT_SERVICE_IDLE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_IDLE_TIMEOUT)
}

fn create_server(pipe: &str, first_instance: bool) -> Result<NamedPipeServer> {
    let mut options = ServerOptions::new();
    if first_instance {
        options.first_pipe_instance(true);
    }
    options
        .create(pipe)
        .with_context(|| format!("无法创建本机命名管道：{pipe}"))
}

async fn handle(mut server: NamedPipeServer, executor: Executor) -> Result<()> {
    let mut request = String::new();
    timeout(
        REQUEST_TIMEOUT,
        BufReader::new(&mut server).read_line(&mut request),
    )
    .await
    .map_err(|_| anyhow!("core_request_timeout: 本机请求读取超时"))??;
    let request: Request = serde_json::from_str(request.trim())?;
    let response = tokio::task::spawn_blocking(move || executor(request.arguments))
        .await
        .context("core_request_worker_failed: Core 请求工作线程异常退出")?;
    server
        .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
        .await?;
    server.flush().await?;
    let mut acknowledgement = [0_u8; 1];
    let _ = timeout(
        Duration::from_secs(1),
        server.read_exact(&mut acknowledgement),
    )
    .await;
    server.disconnect()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Instant};

    #[test]
    fn pipe_name_is_stable_and_local() {
        let first = pipe_name();
        let second = pipe_name();
        assert_eq!(first, second);
        assert!(first.starts_with(r"\\.\pipe\siaocut-core-"));
        assert_eq!(first.len(), r"\\.\pipe\siaocut-core-".len() + 16);
    }

    #[test]
    fn only_a_disconnected_pipe_is_safe_to_retry() {
        assert!(is_stale_pipe(&io::Error::from_raw_os_error(233).into()));
        assert!(!is_stale_pipe(&io::Error::from_raw_os_error(109).into()));
    }

    async fn test_request(pipe: &str, arguments: Vec<String>) -> Response {
        for _ in 0..50 {
            if let Ok(client) = connect(pipe).await {
                return exchange(client, arguments)
                    .await
                    .map_err(ExchangeFailure::into_error)
                    .unwrap();
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("test Core service did not create its named pipe");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serves_status_while_a_long_request_is_running() {
        let pipe = format!(
            r"\\.\pipe\siaocut-core-test-{}",
            uuid::Uuid::new_v4().simple()
        );
        let executor: Executor = Arc::new(|arguments| {
            if arguments.first().map(String::as_str) == Some("slow") {
                thread::sleep(Duration::from_millis(500));
            }
            Response {
                exit_code: 0,
                output: arguments.first().cloned().unwrap_or_default(),
            }
        });
        let service_pipe = pipe.clone();
        let service = tokio::spawn(async move {
            serve_with(service_pipe, Duration::from_millis(100), executor).await
        });
        let slow_pipe = pipe.clone();
        let slow = tokio::spawn(async move { test_request(&slow_pipe, vec!["slow".into()]).await });
        sleep(Duration::from_millis(50)).await;

        let started = Instant::now();
        let fast = timeout(
            Duration::from_millis(250),
            test_request(&pipe, vec!["status".into()]),
        )
        .await
        .expect("status query was blocked behind the long request");
        assert_eq!(fast.output, "status");
        assert!(started.elapsed() < Duration::from_millis(250));
        assert_eq!(slow.await.unwrap().output, "slow");
        service.await.unwrap().unwrap();
    }
}
