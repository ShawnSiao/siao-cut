use super::*;

#[test]
fn merges_a_zero_duration_word_into_the_next_word_with_the_same_start() {
    let item = serde_json::json!({
        "timestamps":{"from":"00:06:03,320","to":"00:06:04,000"},
        "text":" And or the best",
        "tokens":[
            {"text":" And","timestamps":{"from":"00:06:03,320","to":"00:06:03,560"}},
            {"text":" or","timestamps":{"from":"00:06:03,680","to":"00:06:03,680"}},
            {"text":" the","timestamps":{"from":"00:06:03,680","to":"00:06:03,780"}},
            {"text":" best","timestamps":{"from":"00:06:03,780","to":"00:06:04,000"}}
        ]
    });

    let segments = whisper_item_segments(&item, 365.0).unwrap();

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, "And or the best");
    assert_eq!(segments[0].words.len(), 3);
    assert_eq!(segments[0].words[1].text, "or the");
    assert_eq!(segments[0].words[1].start, 363.68);
    assert_eq!(segments[0].words[1].end, 363.78);
}

#[test]
fn merges_a_terminal_zero_duration_word_into_the_previous_word() {
    let item = serde_json::json!({
        "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
        "text":" hello world",
        "tokens":[
            {"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,800"}},
            {"text":" world","timestamps":{"from":"00:00:01,000","to":"00:00:01,000"}}
        ]
    });

    let segments = whisper_item_segments(&item, 2.0).unwrap();

    assert_eq!(segments[0].text, "hello world");
    assert_eq!(segments[0].words.len(), 1);
    assert_eq!(segments[0].words[0].text, "hello world");
    assert_eq!(segments[0].words[0].end, 0.8);
}

#[test]
fn uses_the_parent_range_when_all_content_words_are_terminal_zero_duration() {
    let item = serde_json::json!({
        "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
        "text":" one two",
        "tokens":[
            {"text":" one","timestamps":{"from":"00:00:01,000","to":"00:00:01,000"}},
            {"text":" two","timestamps":{"from":"00:00:01,000","to":"00:00:01,000"}}
        ]
    });

    let segments = whisper_item_segments(&item, 2.0).unwrap();

    assert_eq!(segments[0].text, "one two");
    assert_eq!(segments[0].words.len(), 1);
    assert_eq!(segments[0].words[0].text, "one two");
    assert_eq!(segments[0].words[0].start, 0.0);
    assert_eq!(segments[0].words[0].end, 1.0);
}

#[test]
fn rejects_a_terminal_zero_duration_word_outside_its_parent_item() {
    let item = serde_json::json!({
        "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
        "text":" hello stray",
        "tokens":[
            {"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,800"}},
            {"text":" stray","timestamps":{"from":"00:00:02,000","to":"00:00:02,000"}}
        ]
    });

    let error = whisper_item_segments(&item, 3.0).unwrap_err().to_string();

    assert!(error.contains("transcription_timing_invalid"));
    assert!(error.contains("所属字幕段"));
}

#[test]
fn ignores_empty_control_only_items_without_valid_segment_timing() {
    let raw = serde_json::json!({
        "transcription":[
            {
                "text":"",
                "tokens":[{"text":"[_BEG_]"}]
            },
            {
                "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
                "text":" hello",
                "tokens":[{"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]
            }
        ]
    });

    let validated = validate_whisper_transcript(&raw, 2.0).unwrap();

    assert_eq!(validated.segments.len(), 1);
    assert_eq!(validated.segments[0].text, "hello");
}

#[test]
fn attaches_a_punctuation_only_item_to_the_previous_timed_item() {
    let raw = serde_json::json!({
        "transcription":[
            {
                "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
                "text":" Hello",
                "tokens":[{"text":" Hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]
            },
            {
                "timestamps":{"from":"00:00:00,000","to":"00:00:00,000"},
                "text":"。",
                "tokens":[{"text":"。","timestamps":{"from":"00:00:00,000","to":"00:00:00,000"}}]
            },
            {
                "timestamps":{"from":"00:00:01,000","to":"00:00:02,000"},
                "text":" Next",
                "tokens":[{"text":" Next","timestamps":{"from":"00:00:01,100","to":"00:00:01,900"}}]
            }
        ]
    });

    let validated = validate_whisper_transcript(&raw, 3.0).unwrap();

    assert_eq!(validated.segments.len(), 2);
    assert_eq!(validated.segments[0].text, "Hello。");
    assert_eq!(validated.segments[0].words[0].text, "Hello。");
}

#[test]
fn accepts_a_single_long_word_for_quality_review_instead_of_rejecting_import() {
    let item = serde_json::json!({
        "timestamps":{"from":"00:00:00,000","to":"00:00:09,500"},
        "text":" elongated",
        "tokens":[
            {"text":" elongated","timestamps":{"from":"00:00:00,100","to":"00:00:09,100"}}
        ]
    });

    let segments = whisper_item_segments(&item, 10.0).unwrap();

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, "elongated");
    assert_eq!(segments[0].end - segments[0].start, 9.0);
}
