use super::{PROVIDER_ID, ProviderConfig, ProviderHealth};
use crate::transcription::provider::{ProviderRequest, ProviderSegment, TranscriptionProvider};
use crate::util::now;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{
    Url,
    blocking::{Client, multipart},
};
use serde_json::Value;
use std::time::Duration;

pub(crate) struct MossProvider;

impl TranscriptionProvider for MossProvider {
    fn provider_id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn validate_endpoint(&self, endpoint: &str) -> Result<String> {
        validate_loopback_endpoint(endpoint)
    }

    fn health(&self, config: &ProviderConfig) -> Result<ProviderHealth> {
        let endpoint = self.validate_endpoint(&config.endpoint)?;
        let checked_at = now();
        let result = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?
            .get(format!("{endpoint}/v1/models"))
            .send();
        Ok(match result {
            Ok(response) if response.status().is_success() => ProviderHealth {
                provider_id: config.provider_id.clone(),
                endpoint,
                model_id: config.model_id.clone(),
                state: "healthy".into(),
                detail: "本机 MOSS 服务可用。".into(),
                checked_at,
            },
            Ok(response) => ProviderHealth {
                provider_id: config.provider_id.clone(),
                endpoint,
                model_id: config.model_id.clone(),
                state: "unavailable".into(),
                detail: format!("服务返回 HTTP {}。", response.status()),
                checked_at,
            },
            Err(error) => ProviderHealth {
                provider_id: config.provider_id.clone(),
                endpoint,
                model_id: config.model_id.clone(),
                state: "unavailable".into(),
                detail: format!("无法连接本机 MOSS 服务：{error}"),
                checked_at,
            },
        })
    }

    fn transcribe(&self, request: ProviderRequest<'_>) -> Result<String> {
        let endpoint = self.validate_endpoint(request.endpoint)?;
        let mut prompt = request.prompt.map(str::to_owned);
        if !request.hotwords.is_empty() {
            let suffix = format!("热词提示：{}", request.hotwords.join(", "));
            prompt = Some(match prompt {
                Some(value) => format!("{value}\n{suffix}"),
                None => suffix,
            });
        }
        let mut form = multipart::Form::new()
            .text("model", request.model_id.to_owned())
            .text("response_format", "verbose_json")
            .text("temperature", "0")
            .text("max_new_tokens", "65536")
            .file("file", request.audio_path)
            .context("transcription_import_failed: 无法读取待转写音频")?;
        if let Some(language) = request.language {
            form = form.text("language", language.to_owned());
        }
        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt);
        }
        let response = Client::builder()
            .timeout(Duration::from_secs(2 * 60 * 60))
            .build()?
            .post(format!("{endpoint}/v1/audio/transcriptions"))
            .multipart(form)
            .send()
            .context("transcription_provider_unavailable: MOSS 请求失败")?;
        let status = response.status();
        let body = response
            .text()
            .context("transcription_response_invalid: 无法读取 MOSS 响应")?;
        if !status.is_success() {
            let detail = body.chars().take(500).collect::<String>();
            bail!("transcription_provider_unavailable: MOSS 返回 HTTP {status}：{detail}")
        }
        Ok(body)
    }

    fn parse(&self, raw: &str) -> Result<Vec<ProviderSegment>> {
        parse_response(raw)
    }
}

pub(crate) fn validate_loopback_endpoint(endpoint: &str) -> Result<String> {
    let mut url =
        Url::parse(endpoint.trim()).context("transcription_provider_invalid: MOSS 服务地址无效")?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("transcription_provider_invalid: 仅允许无凭据、无查询参数的本机 HTTP 地址")
    }
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        bail!("transcription_provider_invalid: MOSS 首版只允许连接本机回环地址")
    }
    if !matches!(url.path(), "" | "/") {
        bail!("transcription_provider_invalid: 服务地址不能包含 API 路径")
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn parse_response(raw: &str) -> Result<Vec<ProviderSegment>> {
    let payload: Value = serde_json::from_str(raw)
        .context("transcription_response_invalid: MOSS 未返回有效 JSON")?;
    if let Some(text) = payload.get("text").and_then(Value::as_str)
        && let Ok(segments) = parse_compact_transcript(text)
        && !segments.is_empty()
    {
        return Ok(segments);
    }
    let entries = payload
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow!("transcription_response_invalid: MOSS 响应缺少可解析的 text 或 segments")
        })?;
    let mut result = Vec::new();
    for entry in entries {
        let start = entry
            .get("start")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("transcription_response_invalid: 分段缺少 start"))?;
        let end = entry
            .get("end")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("transcription_response_invalid: 分段缺少 end"))?;
        let mut text = entry
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let mut speaker = ["speaker", "speaker_id", "speaker_label"]
            .into_iter()
            .find_map(|key| entry.get(key).and_then(Value::as_str))
            .map(normalize_speaker);
        if speaker.is_none()
            && text.starts_with("[S")
            && let Some(end_index) = text.find(']')
        {
            speaker = Some(normalize_speaker(&text[1..end_index]));
            text = text[end_index + 1..].trim().to_owned();
        }
        let speaker =
            speaker.ok_or_else(|| anyhow!("transcription_response_invalid: 分段缺少说话人标签"))?;
        result.push(ProviderSegment {
            start,
            end,
            speaker,
            text,
        });
    }
    validate_segments(&result, None)?;
    Ok(result)
}

fn parse_compact_transcript(input: &str) -> Result<Vec<ProviderSegment>> {
    let mut cursor = 0usize;
    let mut result = Vec::new();
    while let Some(start_open_rel) = input[cursor..].find('[') {
        let start_open = cursor + start_open_rel;
        let (start_token, after_start) = bracket_token(input, start_open)?;
        let start = start_token
            .trim()
            .parse::<f64>()
            .context("transcription_response_invalid: 起始时间戳无效")?;
        let speaker_open = input[after_start..]
            .find('[')
            .map(|value| after_start + value)
            .ok_or_else(|| anyhow!("transcription_response_invalid: 缺少说话人标签"))?;
        if !input[after_start..speaker_open].trim().is_empty() {
            bail!("transcription_response_invalid: 时间戳与说话人标签之间存在未知内容")
        }
        let (speaker_token, text_start) = bracket_token(input, speaker_open)?;
        let speaker = normalize_speaker(speaker_token);
        if !speaker.starts_with('S') || speaker.len() < 2 {
            bail!("transcription_response_invalid: 说话人标签无效")
        }
        let mut search = text_start;
        let (end, text_end, after_end) = loop {
            let end_open = input[search..]
                .find('[')
                .map(|value| search + value)
                .ok_or_else(|| anyhow!("transcription_response_invalid: 分段缺少结束时间戳"))?;
            let (token, after) = bracket_token(input, end_open)?;
            if let Ok(value) = token.trim().parse::<f64>() {
                break (value, end_open, after);
            }
            search = after;
        };
        result.push(ProviderSegment {
            start,
            end,
            speaker,
            text: input[text_start..text_end].trim().to_owned(),
        });
        cursor = after_end;
    }
    validate_segments(&result, None)?;
    Ok(result)
}

fn bracket_token(input: &str, open: usize) -> Result<(&str, usize)> {
    if input.as_bytes().get(open) != Some(&b'[') {
        bail!("transcription_response_invalid: 标记格式无效")
    }
    let close = input[open + 1..]
        .find(']')
        .map(|value| open + 1 + value)
        .ok_or_else(|| anyhow!("transcription_response_invalid: 标记缺少右括号"))?;
    Ok((&input[open + 1..close], close + 1))
}

fn normalize_speaker(value: &str) -> String {
    let value = value.trim().trim_matches(['[', ']']).to_ascii_uppercase();
    if let Some(number) = value.strip_prefix("SPEAKER_") {
        return format!("S{:02}", number.parse::<u32>().unwrap_or_default() + 1);
    }
    value
}

fn validate_segments(segments: &[ProviderSegment], duration: Option<f64>) -> Result<()> {
    if segments.is_empty() {
        bail!("transcription_response_invalid: MOSS 没有返回转写分段")
    }
    let mut previous_start = -1.0;
    for segment in segments {
        if !segment.start.is_finite()
            || !segment.end.is_finite()
            || segment.start < 0.0
            || segment.end <= segment.start
        {
            bail!("transcription_timing_invalid: MOSS 返回了无效时间范围")
        }
        if segment.start + 0.001 < previous_start {
            bail!("transcription_timing_invalid: MOSS 分段未按时间排序")
        }
        if duration.is_some_and(|duration| segment.end > duration + 2.0) {
            bail!("transcription_timing_invalid: MOSS 分段超出媒体时长")
        }
        if segment.text.trim().is_empty() {
            bail!("transcription_response_invalid: MOSS 返回了空字幕段")
        }
        previous_start = segment.start;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };
    use tempfile::tempdir;

    #[test]
    fn parses_official_compact_shape() {
        let value = r#"{"text":"[0.48][S01]Welcome everyone[1.66][12.26][S02]Ready[13.81]"}"#;
        let segments = parse_response(value).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].speaker, "S01");
        assert_eq!(segments[1].start, 12.26);
    }

    #[test]
    fn automatic_language_omits_multipart_language_field() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            let header_end;
            loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                if let Some(position) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    header_end = position + 4;
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(str::trim)
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            sender.send(request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"text\":\"ok\"}",
                )
                .unwrap();
        });
        let temp = tempdir().unwrap();
        let audio = temp.path().join("sample.wav");
        std::fs::write(&audio, b"wav").unwrap();

        MossProvider
            .transcribe(ProviderRequest {
                endpoint: &endpoint,
                model_id: "moss-test",
                language: None,
                prompt: None,
                hotwords: &[],
                audio_path: &audio,
            })
            .unwrap();

        server.join().unwrap();
        let request = String::from_utf8_lossy(&receiver.recv().unwrap()).into_owned();
        assert!(request.contains("name=\"model\""));
        assert!(!request.contains("name=\"language\""));
    }
}
