//! URL import proxy and bounded child-process transport.
use super::*;

pub(super) fn add_proxy_argument(arguments: &mut Vec<String>, proxy_url: &str) {
    let index = arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap_or(arguments.len());
    arguments.splice(index..index, ["--proxy".to_owned(), proxy_url.to_owned()]);
}

pub(crate) struct SafeConnectProxy {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SafeConnectProxy {
    pub(crate) fn start() -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .context("source_proxy_failed: 无法启动 URL 安全代理")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if stream.set_nonblocking(false).is_err() {
                            continue;
                        }
                        thread::spawn(move || {
                            let _ = handle_proxy_connection(stream);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }

    pub(crate) fn url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for SafeConnectProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn handle_proxy_connection(mut client: TcpStream) -> Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(5)))?;
    client.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut request = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];
    while request.len() < 16 * 1024 && !request.ends_with(b"\r\n\r\n") {
        if client.read(&mut byte)? == 0 {
            return Ok(());
        }
        request.push(byte[0]);
    }
    let first_line = String::from_utf8_lossy(&request)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let mut parts = first_line.split_whitespace();
    if parts.next() != Some("CONNECT") {
        let _ = std::io::Write::write_all(
            &mut client,
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return Ok(());
    }
    let authority = parts
        .next()
        .ok_or_else(|| anyhow!("source_proxy_target_invalid: CONNECT 缺少目标"))?;
    let target = Url::parse(&format!("https://{authority}/"))
        .context("source_proxy_target_invalid: CONNECT 目标无效")?;
    let host = target
        .host_str()
        .ok_or_else(|| anyhow!("source_proxy_target_invalid: CONNECT 缺少主机"))?;
    let port = target.port_or_known_default().unwrap_or(443);
    let addresses = public_socket_addresses(host, port);
    let Ok(addresses) = addresses else {
        let _ = std::io::Write::write_all(
            &mut client,
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return Ok(());
    };
    let mut upstream = None;
    for address in addresses {
        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(8)) {
            upstream = Some(stream);
            break;
        }
    }
    let Some(mut upstream) = upstream else {
        let _ = std::io::Write::write_all(
            &mut client,
            b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return Ok(());
    };
    client.set_read_timeout(Some(Duration::from_secs(45)))?;
    client.set_write_timeout(Some(Duration::from_secs(45)))?;
    upstream.set_read_timeout(Some(Duration::from_secs(45)))?;
    upstream.set_write_timeout(Some(Duration::from_secs(45)))?;
    std::io::Write::write_all(&mut client, b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
    let mut client_reader = client.try_clone()?;
    let mut upstream_writer = upstream.try_clone()?;
    let upload = thread::spawn(move || {
        let _ = std::io::copy(&mut client_reader, &mut upstream_writer);
        let _ = upstream_writer.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut upstream, &mut client);
    let _ = client.shutdown(Shutdown::Write);
    let _ = upload.join();
    Ok(())
}

#[derive(Debug)]
pub(super) struct BoundedOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
}

pub(super) fn output_with_timeout(
    command: Command,
    timeout: Duration,
    timeout_message: &str,
) -> Result<BoundedOutput> {
    output_with_timeout_after_isolation(command, timeout, timeout_message, || Ok(()))
}

pub(super) fn output_with_timeout_after_isolation(
    mut command: Command,
    timeout: Duration,
    timeout_message: &str,
    after_isolation: impl FnOnce() -> Result<()>,
) -> Result<BoundedOutput> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut process_job = Some(assign_source_process_job(
        &mut child,
        "source_process_isolation_failed: 无法隔离工具子进程",
    )?);
    if let Err(error) = after_isolation() {
        terminate_source_process(&mut child, &mut process_job);
        return Err(error);
    }
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("无法读取子进程标准输出"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("无法读取子进程错误输出"))?;
    let stdout_reader =
        thread::spawn(move || read_bounded(&mut stdout, MAX_SUBPROCESS_STDOUT_BYTES));
    let stderr_reader =
        thread::spawn(move || read_bounded(&mut stderr, MAX_SUBPROCESS_STDERR_BYTES));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            terminate_source_process(&mut child, &mut process_job);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            bail!("{timeout_message}")
        }
        thread::sleep(Duration::from_millis(50));
    };
    release_source_process_job(&mut process_job);
    Ok(BoundedOutput {
        status,
        stdout: stdout_reader
            .join()
            .map_err(|_| anyhow!("无法汇总子进程标准输出"))?,
        stderr: stderr_reader
            .join()
            .map_err(|_| anyhow!("无法汇总子进程错误输出"))?,
    })
}

pub(super) fn assign_source_process_job(
    child: &mut Child,
    error_message: &str,
) -> Result<KillOnCloseJob> {
    // Supported Windows versions allow nested jobs. If a restrictive host policy
    // rejects assignment, continuing would make timeouts and cancellation
    // unenforceable for descendants, so return a diagnosable error instead.
    match KillOnCloseJob::assign(child) {
        Ok(job) => Ok(job),
        Err(error) => {
            crate::util::terminate_process_tree(child);
            Err(error).with_context(|| error_message.to_owned())
        }
    }
}

pub(super) fn release_source_process_job(process_job: &mut Option<KillOnCloseJob>) {
    drop(process_job.take());
}

pub(super) fn terminate_source_process(
    child: &mut Child,
    process_job: &mut Option<KillOnCloseJob>,
) {
    if process_job.is_some() {
        release_source_process_job(process_job);
        let _ = child.wait();
    } else {
        crate::util::terminate_process_tree(child);
    }
}

pub(super) fn read_bounded(reader: &mut impl Read, limit: usize) -> Vec<u8> {
    let mut stored = Vec::with_capacity(limit.min(64 * 1024));
    let mut chunk = [0_u8; 64 * 1024];
    while let Ok(length) = reader.read(&mut chunk) {
        if length == 0 {
            break;
        }
        let remaining = limit.saturating_sub(stored.len());
        stored.extend_from_slice(&chunk[..length.min(remaining)]);
    }
    stored
}
pub(super) fn public_socket_addresses(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".home.arpa")
    {
        bail!("source_private_network: 已拒绝本机或私网地址")
    }
    let addresses = if let Ok(ip) = host.parse::<IpAddr>() {
        ensure_public_ip(ip)?;
        vec![ip]
    } else {
        let addresses = (host.as_str(), port)
            .to_socket_addrs()
            .context("source_dns_failed: 无法解析 URL 主机")?
            .map(|address| address.ip())
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            bail!("source_dns_failed: URL 主机没有可用地址")
        }
        let addresses = if addresses.iter().all(|address| fake_tunnel_ip(*address)) {
            resolve_public_dns_over_https(&host)?
        } else {
            addresses
        };
        for address in &addresses {
            ensure_public_ip(*address)?;
        }
        addresses
    };
    Ok(addresses
        .into_iter()
        .map(|address| SocketAddr::new(address, port))
        .collect())
}
