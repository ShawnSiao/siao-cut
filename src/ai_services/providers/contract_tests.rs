use tempfile::tempdir;

use super::{anthropic, gemini, openai, openai_compatible, test_support::*};
use crate::ai_services::types::{AiProtocol, AiProviderId};

#[test]
fn openai_responses_contract() {
    let temp = tempdir().unwrap();
    let server = serve_once(
        200,
        r#"{"id":"resp_1","output":[{"content":[{"type":"output_text","text":"{\"ok\":true}"}]}],"usage":{"total_tokens":3}}"#,
    );
    let output = openai::generate(
        &mock_service(
            AiProviderId::Openai,
            AiProtocol::OpenaiResponses,
            &server.url,
        ),
        &network(temp.path()),
        &generation_input(),
    )
    .unwrap();
    let request = server.finish();
    assert!(request.contains("POST /responses"));
    assert!(request.contains("\"store\":false"));
    assert_eq!(output.output_text, r#"{"ok":true}"#);
}

#[test]
fn anthropic_messages_contract() {
    let temp = tempdir().unwrap();
    let server = serve_once(
        200,
        r#"{"id":"msg_1","content":[{"type":"text","text":"{\"ok\":true}"}],"usage":{"input_tokens":2}}"#,
    );
    anthropic::generate(
        &mock_service(
            AiProviderId::Anthropic,
            AiProtocol::AnthropicMessages,
            &server.url,
        ),
        &network(temp.path()),
        &generation_input(),
    )
    .unwrap();
    let request = server.finish().to_ascii_lowercase();
    assert!(request.contains("post /v1/messages"));
    assert!(request.contains("x-api-key: test-key"));
}

#[test]
fn gemini_generate_content_contract() {
    let temp = tempdir().unwrap();
    let server = serve_once(
        200,
        r#"{"candidates":[{"content":{"parts":[{"text":"{\"ok\":true}"}]}}],"usageMetadata":{"totalTokenCount":3}}"#,
    );
    gemini::generate(
        &mock_service(
            AiProviderId::Gemini,
            AiProtocol::GeminiGenerateContent,
            &server.url,
        ),
        &network(temp.path()),
        &generation_input(),
    )
    .unwrap();
    let request = server.finish();
    assert!(request.contains("POST /models/test-model:generateContent"));
    assert!(request.contains("responseJsonSchema"));
}

#[test]
fn openai_compatible_contract() {
    let temp = tempdir().unwrap();
    let server = serve_once(
        200,
        r#"{"id":"chat_1","choices":[{"message":{"content":"{\"ok\":true}"}}],"usage":{"total_tokens":3}}"#,
    );
    openai_compatible::generate(
        &mock_service(
            AiProviderId::Deepseek,
            AiProtocol::OpenaiChatCompletions,
            &server.url,
        ),
        &network(temp.path()),
        &generation_input(),
    )
    .unwrap();
    let request = server.finish();
    assert!(request.contains("POST /chat/completions"));
    assert!(request.contains("json_object"));
}
