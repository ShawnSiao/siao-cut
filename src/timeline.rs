use crate::model::{Edit, Project, TimelineCut, TimelineMap, TimelineRange};

const EPSILON: f64 = 0.001;

pub fn build(project: &Project) -> TimelineMap {
    let source_duration = project
        .media
        .duration_seconds
        .unwrap_or_else(|| {
            project
                .transcript
                .segments
                .iter()
                .map(|segment| segment.end)
                .fold(0.0, f64::max)
        })
        .max(0.0);
    build_from_edits(source_duration, &project.edits)
}

pub fn build_from_edits(source_duration: f64, edits: &[Edit]) -> TimelineMap {
    let mut raw = edits
        .iter()
        .filter(|edit| {
            edit.status == "applied"
                && matches!(edit.kind.as_str(), "cut" | "word_cut" | "semantic_cut")
        })
        .filter_map(|edit| {
            let start = edit.start.clamp(0.0, source_duration);
            let end = edit.end.clamp(0.0, source_duration);
            (end - start > EPSILON).then(|| (start, end, edit.id.clone()))
        })
        .collect::<Vec<_>>();
    raw.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.total_cmp(&right.1)));

    let mut merged: Vec<(f64, f64, Vec<String>)> = Vec::new();
    for (start, end, id) in raw {
        if let Some(last) = merged.last_mut()
            && start <= last.1 + EPSILON
        {
            last.1 = last.1.max(end);
            last.2.push(id);
            continue;
        }
        merged.push((start, end, vec![id]));
    }

    let mut cursor = 0.0;
    let mut output_cursor = 0.0;
    let mut kept_ranges = Vec::new();
    let mut cuts = Vec::new();
    for (start, end, edit_ids) in merged {
        if start > cursor + EPSILON {
            let duration = start - cursor;
            kept_ranges.push(TimelineRange {
                source_start: cursor,
                source_end: start,
                output_start: output_cursor,
                output_end: output_cursor + duration,
            });
            output_cursor += duration;
        }
        cuts.push(TimelineCut {
            edit_ids,
            source_start: start,
            source_end: end,
            output_at: output_cursor,
        });
        cursor = cursor.max(end);
    }
    if source_duration > cursor + EPSILON {
        kept_ranges.push(TimelineRange {
            source_start: cursor,
            source_end: source_duration,
            output_start: output_cursor,
            output_end: output_cursor + source_duration - cursor,
        });
        output_cursor += source_duration - cursor;
    }

    TimelineMap {
        source_duration,
        output_duration: output_cursor,
        kept_ranges,
        cuts,
    }
}

pub fn source_to_output(map: &TimelineMap, source_seconds: f64) -> f64 {
    let source = source_seconds.clamp(0.0, map.source_duration);
    if let Some(range) = map
        .kept_ranges
        .iter()
        .find(|range| source >= range.source_start && source <= range.source_end)
    {
        return range.output_start + source - range.source_start;
    }
    map.cuts
        .iter()
        .find(|cut| source >= cut.source_start && source <= cut.source_end)
        .map(|cut| cut.output_at)
        .unwrap_or(map.output_duration)
}

#[allow(dead_code)]
pub fn output_to_source(map: &TimelineMap, output_seconds: f64) -> f64 {
    let output = output_seconds.clamp(0.0, map.output_duration);
    map.kept_ranges
        .iter()
        .find(|range| output >= range.output_start && output <= range.output_end)
        .map(|range| range.source_start + output - range.output_start)
        .unwrap_or(map.source_duration)
}

pub fn retime_interval(map: &TimelineMap, start: f64, end: f64) -> Option<(f64, f64)> {
    if map
        .cuts
        .iter()
        .any(|cut| start >= cut.source_start - EPSILON && end <= cut.source_end + EPSILON)
    {
        return None;
    }
    let output_start = source_to_output(map, start);
    let output_end = source_to_output(map, end);
    (output_end - output_start > EPSILON).then_some((output_start, output_end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Edit;

    fn cut(id: &str, start: f64, end: f64) -> Edit {
        Edit {
            id: id.into(),
            kind: "cut".into(),
            status: "applied".into(),
            segment_id: id.into(),
            start,
            end,
            reason: "test".into(),
            created_at: String::new(),
            cut_range: None,
            suggestion: None,
        }
    }

    #[test]
    fn timeline_map_merges_overlapping_cuts_and_maps_both_directions() {
        let map = build_from_edits(12.0, &[cut("a", 2.0, 4.0), cut("b", 3.5, 5.0)]);
        assert_eq!(map.cuts.len(), 1);
        assert_eq!(map.output_duration, 9.0);
        assert_eq!(source_to_output(&map, 7.0), 4.0);
        assert_eq!(output_to_source(&map, 4.0), 7.0);
        assert_eq!(retime_interval(&map, 2.0, 4.0), None);
        assert_eq!(retime_interval(&map, 7.0, 8.0), Some((4.0, 5.0)));
    }

    #[test]
    fn semantic_cuts_share_the_same_time_map() {
        let mut edit = cut("agent", 1.0, 2.0);
        edit.kind = "semantic_cut".into();
        let map = build_from_edits(3.0, &[edit]);
        assert_eq!(map.output_duration, 2.0);
        assert_eq!(map.kept_ranges.len(), 2);
    }
}
