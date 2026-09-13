use super::*;
use serde_json::json;

#[test]
fn shared_execution_only_adds_vad_arguments_when_a_verified_model_is_supplied() {
    for vad in [None, Some("models/vad model.bin")] {
        let command = whisper_command(
            "selected-whisper.exe",
            Path::new("model.bin"),
            Path::new("audio.wav"),
            Path::new("out/transcript"),
            Some("en"),
            vad,
        );
        assert_eq!(command.get_program(), "selected-whisper.exe");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(args.contains(&"--vad"), vad.is_some());
        assert_eq!(args.contains(&"-vm"), vad.is_some());
        if let Some(path) = vad {
            assert!(args.windows(2).any(|pair| pair == ["-vm", path]));
        }
        assert!(args.windows(2).any(|pair| pair == ["-l", "en"]));
        assert!(args.contains(&"-ojf"));
        assert!(args.contains(&"-sow"));
    }
}

fn speech_silence_speech() -> Value {
    json!({"transcription": [
        {"timestamps":{"from":"00:00:00,200","to":"00:00:01,000"}, "text":" first",
         "tokens":[{"text":" first","timestamps":{"from":"00:00:00,200","to":"00:00:00,900"}}]},
        {"timestamps":{"from":"00:00:12,500","to":"00:00:13,500"}, "text":" last",
         "tokens":[{"text":" last","timestamps":{"from":"00:00:12,500","to":"00:00:13,400"}}]}
    ]})
}

#[test]
fn background_candidates_keep_original_timestamps_and_actual_vad_mode() {
    for mode in [
        TranscriptionTimingMode::no_vad(),
        TranscriptionTimingMode::verified_vad(),
    ] {
        let candidate = normalized_whisper_candidate(&speech_silence_speech(), 14.0, mode).unwrap();
        assert_eq!(candidate["timingValidation"]["mode"], mode.mode);
        assert_eq!(candidate["timingValidation"]["vadUsed"], mode.vad_used);
        assert_eq!(
            candidate["timingValidation"]["timeDomain"],
            "original_media"
        );
        assert_eq!(candidate["timingValidation"]["segmentCount"], 2);
        assert_eq!(candidate["timingValidation"]["wordCount"], 2);
        assert_eq!(candidate["segments"][1]["start"], 12.5);
        assert_eq!(candidate["segments"][1]["words"][0]["start"], 12.5);
    }
}

#[test]
fn background_candidates_reject_compressed_vad_timing_before_persistence() {
    let mut raw = speech_silence_speech();
    raw["transcription"][1]["tokens"][0]["timestamps"] =
        json!({"from":"00:00:08,000","to":"00:00:08,900"});
    for mode in [
        TranscriptionTimingMode::no_vad(),
        TranscriptionTimingMode::verified_vad(),
    ] {
        let error = normalized_whisper_candidate(&raw, 14.0, mode).unwrap_err();
        assert!(error.to_string().contains("transcription_timing_invalid"));
    }
}
