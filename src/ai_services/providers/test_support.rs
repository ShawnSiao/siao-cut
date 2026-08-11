use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

use serde_json::json;

use super::GenerationInput;
use crate::ai_services::{
    network::NetworkStore,
    types::{AiProtocol, AiProviderId, ResolvedAiService},
};

pub struct TestServer {
    pub url: String,
    request: mpsc::Receiver<String>,
}

pub fn serve_once(status: u16, body: &'static str) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buffer = vec![0_u8; 64 * 1024];
        let size = stream.read(&mut buffer).expect("read");
        sender
            .send(String::from_utf8_lossy(&buffer[..size]).into_owned())
            .expect("send");
        let reason = if status == 200 { "OK" } else { "Error" };
        write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("write");
    });
    TestServer {
        url: format!("http://{address}"),
        request: receiver,
    }
}

impl TestServer {
    pub fn finish(self) -> String {
        self.request.recv().expect("request")
    }
}

pub fn mock_service(
    provider_id: AiProviderId,
    protocol: AiProtocol,
    base_url: &str,
) -> ResolvedAiService {
    ResolvedAiService {
        service_config_id: None,
        provider_id,
        protocol,
        base_url: base_url.to_owned(),
        model_id: Some("test-model".into()),
        api_key: "test-key".into(),
    }
}

pub fn network(home: &std::path::Path) -> NetworkStore {
    NetworkStore::for_home(home)
}

pub fn generation_input() -> GenerationInput {
    GenerationInput {
        model_id: "test-model".into(),
        system: "system".into(),
        prompt: "prompt".into(),
        schema_name: "result".into(),
        schema: json!({"type":"object"}),
    }
}
