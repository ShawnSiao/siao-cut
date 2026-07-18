use crate::model::{
    SpeechEvidence, SpeechEvidenceKind, SpeechInsightStatus, SpeechInsightThresholds,
    SpeechInsights, SpeechPause, SpeechPauseSeverity, Transcript, Word,
};

pub const ANALYZER_VERSION: &str = "rhythm-v1";
pub const PAUSE_SECONDS: f64 = 0.8;
pub const LONG_PAUSE_SECONDS: f64 = 1.5;
pub const LOW_CONFIDENCE: f64 = 0.75;

pub fn normalized_token(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn is_filler_token(text: &str) -> bool {
    matches!(
        normalized_token(text).as_str(),
        "嗯" | "呃" | "额" | "唔" | "uh" | "um" | "erm" | "hmm"
    )
}

pub fn analyze(transcript: &Transcript) -> SpeechInsights {
    let thresholds = SpeechInsightThresholds {
        pause_seconds: PAUSE_SECONDS,
        long_pause_seconds: LONG_PAUSE_SECONDS,
        low_confidence: LOW_CONFIDENCE,
    };
    let mut words = transcript
        .words
        .iter()
        .filter(|word| valid_word(word))
        .collect::<Vec<_>>();
    words.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then(left.end.total_cmp(&right.end))
            .then(left.id.cmp(&right.id))
    });
    if words.is_empty() {
        return SpeechInsights {
            status: SpeechInsightStatus::InsufficientEvidence,
            analyzer_version: ANALYZER_VERSION.into(),
            thresholds,
            ..SpeechInsights::default()
        };
    }

    let mut pauses = Vec::new();
    let mut evidence = Vec::new();
    let mut spoken_duration = 0.0;
    let mut range_start = words[0].start;
    let mut range_end = words[0].end;

    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            if word.start <= range_end {
                range_end = range_end.max(word.end);
            } else {
                spoken_duration += range_end - range_start;
                range_start = word.start;
                range_end = word.end;
            }
            let previous = words[index - 1];
            let gap = word.start - previous.end;
            if gap >= PAUSE_SECONDS {
                pauses.push(SpeechPause {
                    start: rounded(previous.end, 3),
                    end: rounded(word.start, 3),
                    duration: rounded(gap, 3),
                    previous_word_id: previous.id.clone(),
                    next_word_id: word.id.clone(),
                    severity: if gap >= LONG_PAUSE_SECONDS {
                        SpeechPauseSeverity::LongPause
                    } else {
                        SpeechPauseSeverity::Pause
                    },
                });
            }
        }

        let token = normalized_token(&word.text);
        let is_uh_oh = token == "uh"
            && words
                .get(index + 1)
                .is_some_and(|next| normalized_token(&next.text) == "oh");
        if is_filler_token(&word.text) && !is_uh_oh {
            evidence.push(SpeechEvidence {
                kind: SpeechEvidenceKind::Filler,
                word_id: word.id.clone(),
                segment_id: word.segment_id.clone(),
                start: rounded(word.start, 3),
                end: rounded(word.end, 3),
                text: word.text.clone(),
                confidence: word.confidence,
            });
        }
        if word
            .confidence
            .is_some_and(|confidence| confidence < LOW_CONFIDENCE)
        {
            evidence.push(SpeechEvidence {
                kind: SpeechEvidenceKind::LowConfidence,
                word_id: word.id.clone(),
                segment_id: word.segment_id.clone(),
                start: rounded(word.start, 3),
                end: rounded(word.end, 3),
                text: word.text.clone(),
                confidence: word.confidence,
            });
        }
    }
    spoken_duration += range_end - range_start;

    let span_duration = words.last().unwrap().end - words[0].start;
    let filler_count = evidence
        .iter()
        .filter(|item| item.kind == SpeechEvidenceKind::Filler)
        .count();
    let low_confidence_count = evidence
        .iter()
        .filter(|item| item.kind == SpeechEvidenceKind::LowConfidence)
        .count();
    let total_pause_duration = pauses.iter().map(|pause| pause.duration).sum::<f64>();
    let long_pause_count = pauses
        .iter()
        .filter(|pause| pause.severity == SpeechPauseSeverity::LongPause)
        .count();

    SpeechInsights {
        status: SpeechInsightStatus::Ready,
        analyzer_version: ANALYZER_VERSION.into(),
        thresholds,
        span_duration_seconds: rounded(span_duration, 3),
        spoken_duration_seconds: rounded(spoken_duration, 3),
        token_count: words.len(),
        tokens_per_minute: rounded(words.len() as f64 * 60.0 / spoken_duration, 1),
        pause_count: pauses.len(),
        long_pause_count,
        total_pause_duration_seconds: rounded(total_pause_duration, 3),
        filler_count,
        low_confidence_count,
        pauses,
        evidence,
    }
}

fn valid_word(word: &Word) -> bool {
    word.start.is_finite()
        && word.end.is_finite()
        && word.start >= 0.0
        && word.end > word.start
        && !normalized_token(&word.text).is_empty()
}

fn rounded(value: f64, decimals: i32) -> f64 {
    let scale = 10_f64.powi(decimals);
    (value * scale).round() / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(id: &str, segment_id: &str, start: f64, end: f64, text: &str, confidence: f64) -> Word {
        Word {
            id: id.into(),
            segment_id: segment_id.into(),
            start,
            end,
            text: text.into(),
            confidence: Some(confidence),
        }
    }

    #[test]
    fn reports_insufficient_evidence_without_word_times() {
        let insights = analyze(&Transcript {
            source_language: "zh".into(),
            segments: Vec::new(),
            words: Vec::new(),
        });
        assert_eq!(insights.status, SpeechInsightStatus::InsufficientEvidence);
        assert_eq!(insights.analyzer_version, ANALYZER_VERSION);
        assert_eq!(insights.token_count, 0);
        assert!(insights.pauses.is_empty());
    }

    #[test]
    fn measures_rhythm_pauses_fillers_and_low_confidence() {
        let insights = analyze(&Transcript {
            source_language: "zh".into(),
            segments: Vec::new(),
            words: vec![
                word("w1", "s1", 0.0, 0.5, "嗯", 0.5),
                word("w2", "s1", 1.0, 1.4, "今天", 0.9),
                word("w3", "s2", 3.2, 3.6, "开始", 0.7),
            ],
        });
        assert_eq!(insights.status, SpeechInsightStatus::Ready);
        assert_eq!(insights.token_count, 3);
        assert_eq!(insights.spoken_duration_seconds, 1.3);
        assert_eq!(insights.span_duration_seconds, 3.6);
        assert_eq!(insights.tokens_per_minute, 138.5);
        assert_eq!(insights.pause_count, 1);
        assert_eq!(insights.long_pause_count, 1);
        assert_eq!(insights.total_pause_duration_seconds, 1.8);
        assert_eq!(insights.filler_count, 1);
        assert_eq!(insights.low_confidence_count, 2);
        assert_eq!(insights.pauses[0].severity, SpeechPauseSeverity::LongPause);
    }

    #[test]
    fn does_not_double_count_overlapping_word_ranges_or_uh_oh() {
        let insights = analyze(&Transcript {
            source_language: "en".into(),
            segments: Vec::new(),
            words: vec![
                word("w1", "s1", 0.0, 0.8, "uh", 0.9),
                word("w2", "s1", 0.5, 1.0, "oh", 0.9),
            ],
        });
        assert_eq!(insights.spoken_duration_seconds, 1.0);
        assert_eq!(insights.filler_count, 0);
    }
}
