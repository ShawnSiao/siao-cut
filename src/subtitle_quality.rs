use crate::model::{
    Segment, SubtitleIssueKind, SubtitleIssueSeverity, SubtitleQualityIssue, SubtitleQualityReport,
    SubtitleQualityStatus, SubtitleQualityThresholds,
};

const TIME_EPSILON: f64 = 0.000_001;

fn issue(
    kind: SubtitleIssueKind,
    severity: SubtitleIssueSeverity,
    segment: &Segment,
    related_segment_id: Option<String>,
    message: String,
    measured_value: Option<f64>,
    threshold: Option<f64>,
) -> SubtitleQualityIssue {
    SubtitleQualityIssue {
        id: format!("quality-{}-{}", kind.as_str(), segment.id),
        kind,
        severity,
        segment_id: segment.id.clone(),
        related_segment_id,
        start: segment.start,
        end: segment.end,
        message,
        measured_value,
        threshold,
    }
}

pub fn inspect(segments: &[Segment], media_duration: Option<f64>) -> SubtitleQualityReport {
    let thresholds = SubtitleQualityThresholds::default();
    let mut issues = Vec::new();
    let mut ordered = segments.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then_with(|| left.end.total_cmp(&right.end))
            .then_with(|| left.id.cmp(&right.id))
    });

    for segment in &ordered {
        let valid_timing = segment.start.is_finite()
            && segment.end.is_finite()
            && segment.start >= 0.0
            && segment.end > segment.start;
        if !valid_timing {
            issues.push(issue(
                SubtitleIssueKind::InvalidTiming,
                SubtitleIssueSeverity::Error,
                segment,
                None,
                "时间范围无效".to_owned(),
                None,
                None,
            ));
        }
        if segment.text.trim().is_empty() {
            issues.push(issue(
                SubtitleIssueKind::EmptyText,
                SubtitleIssueSeverity::Error,
                segment,
                None,
                "字幕文本为空".to_owned(),
                None,
                None,
            ));
        }
        if let Some(duration) = media_duration.filter(|value| value.is_finite())
            && valid_timing
            && segment.end > duration + TIME_EPSILON
        {
            issues.push(issue(
                SubtitleIssueKind::OutOfBounds,
                SubtitleIssueSeverity::Error,
                segment,
                None,
                "字幕结束时间超过原片时长".to_owned(),
                Some(segment.end),
                Some(duration),
            ));
        }
        if !valid_timing {
            continue;
        }
        let duration = segment.end - segment.start;
        if duration > thresholds.max_duration_seconds + TIME_EPSILON {
            issues.push(issue(
                SubtitleIssueKind::DurationTooLong,
                SubtitleIssueSeverity::Warning,
                segment,
                None,
                "单条字幕持续时间过长".to_owned(),
                Some(duration),
                Some(thresholds.max_duration_seconds),
            ));
        }
        let max_line_characters = segment
            .text
            .lines()
            .map(|line| {
                line.chars()
                    .filter(|character| !character.is_whitespace())
                    .count()
            })
            .max()
            .unwrap_or_default();
        if max_line_characters > thresholds.max_line_characters {
            issues.push(issue(
                SubtitleIssueKind::LineTooLong,
                SubtitleIssueSeverity::Warning,
                segment,
                None,
                "单行字幕字符过多".to_owned(),
                Some(max_line_characters as f64),
                Some(thresholds.max_line_characters as f64),
            ));
        }
        let visible_characters = segment
            .text
            .chars()
            .filter(|character| !character.is_whitespace())
            .count();
        let characters_per_second = visible_characters as f64 / duration;
        if characters_per_second > thresholds.max_characters_per_second + TIME_EPSILON {
            issues.push(issue(
                SubtitleIssueKind::ReadingSpeedHigh,
                SubtitleIssueSeverity::Warning,
                segment,
                None,
                "字幕阅读速度过快".to_owned(),
                Some(characters_per_second),
                Some(thresholds.max_characters_per_second),
            ));
        }
    }

    for pair in ordered.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        if !previous.start.is_finite()
            || !previous.end.is_finite()
            || !current.start.is_finite()
            || !current.end.is_finite()
            || previous.end <= previous.start
            || current.end <= current.start
        {
            continue;
        }
        let gap = current.start - previous.end;
        if gap < -TIME_EPSILON {
            issues.push(issue(
                SubtitleIssueKind::Overlap,
                SubtitleIssueSeverity::Warning,
                current,
                Some(previous.id.clone()),
                "与上一条字幕时间重叠".to_owned(),
                Some(-gap),
                Some(0.0),
            ));
        } else if gap + TIME_EPSILON < thresholds.min_gap_seconds {
            issues.push(issue(
                SubtitleIssueKind::GapTooShort,
                SubtitleIssueSeverity::Warning,
                current,
                Some(previous.id.clone()),
                "与上一条字幕间隔过短".to_owned(),
                Some(gap.max(0.0)),
                Some(thresholds.min_gap_seconds),
            ));
        }
    }

    let error_count = issues
        .iter()
        .filter(|item| item.severity == SubtitleIssueSeverity::Error)
        .count();
    let warning_count = issues.len() - error_count;
    let (status, status_label) = if error_count > 0 {
        (
            SubtitleQualityStatus::Error,
            format!("{error_count} 项错误需要处理"),
        )
    } else if warning_count > 0 {
        (
            SubtitleQualityStatus::Warning,
            format!("{warning_count} 项质量提醒"),
        )
    } else {
        (SubtitleQualityStatus::Good, "未发现字幕问题".to_owned())
    };
    SubtitleQualityReport {
        status,
        status_label,
        issue_count: issues.len(),
        error_count,
        warning_count,
        thresholds,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: &str, start: f64, end: f64, text: &str) -> Segment {
        Segment {
            id: id.to_owned(),
            start,
            end,
            text: text.to_owned(),
            confidence: None,
        }
    }

    #[test]
    fn subtitle_quality_reports_every_required_problem_with_locations() {
        let long_line = "字".repeat(50);
        let report = inspect(
            &[
                segment("empty", 0.0, 1.0, " "),
                segment("long", 1.05, 10.0, &long_line),
                segment("fast", 9.5, 10.0, "阅读速度明显过快的字幕"),
                segment("outside", 10.1, 12.0, "越界"),
            ],
            Some(11.0),
        );
        assert_eq!(report.status, SubtitleQualityStatus::Error);
        assert!(report.error_count >= 2);
        for kind in [
            SubtitleIssueKind::EmptyText,
            SubtitleIssueKind::DurationTooLong,
            SubtitleIssueKind::LineTooLong,
            SubtitleIssueKind::ReadingSpeedHigh,
            SubtitleIssueKind::GapTooShort,
            SubtitleIssueKind::Overlap,
            SubtitleIssueKind::OutOfBounds,
        ] {
            assert!(
                report.issues.iter().any(|item| item.kind == kind),
                "missing {kind:?}"
            );
        }
        assert!(report.issues.iter().all(|item| !item.segment_id.is_empty()));
    }

    #[test]
    fn subtitle_quality_accepts_a_clean_transcript() {
        let report = inspect(
            &[
                segment("s1", 0.0, 2.0, "第一句"),
                segment("s2", 2.2, 4.0, "第二句"),
            ],
            Some(5.0),
        );
        assert_eq!(report, SubtitleQualityReport::default());
    }
}
