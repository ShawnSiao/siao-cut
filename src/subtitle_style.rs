use crate::{
    model::{
        CanvasAspectRatio, CanvasSettings, Project, SubtitlePosition, SubtitleStyle,
        SubtitleStylePreset,
    },
    project,
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSubtitleStyle {
    preset: SubtitleStylePreset,
    position: SubtitlePosition,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleStylePresetOption {
    pub id: SubtitleStylePreset,
    pub label: &'static str,
    pub description: &'static str,
}

pub fn catalog() -> Vec<SubtitleStylePresetOption> {
    vec![
        SubtitleStylePresetOption {
            id: SubtitleStylePreset::Compact,
            label: "紧凑",
            description: "42 px，适合信息密度较高的双语字幕",
        },
        SubtitleStylePresetOption {
            id: SubtitleStylePreset::Standard,
            label: "清晰",
            description: "52 px，默认口播字幕，兼顾可读性和画面占用",
        },
        SubtitleStylePresetOption {
            id: SubtitleStylePreset::Emphasis,
            label: "强调",
            description: "60 px，加粗描边，适合短句和重点表达",
        },
    ]
}

pub fn resolve(preset: SubtitleStylePreset, position: SubtitlePosition) -> SubtitleStyle {
    let mut style = SubtitleStyle {
        preset,
        position,
        ..SubtitleStyle::default()
    };
    match preset {
        SubtitleStylePreset::Compact => {
            style.font_size = 42;
            style.secondary_font_size = 32;
            style.outline_width = 2;
            style.shadow_depth = 1;
            style.safe_margin_percent = 6;
        }
        SubtitleStylePreset::Standard => {}
        SubtitleStylePreset::Emphasis => {
            style.font_size = 60;
            style.secondary_font_size = 46;
            style.outline_width = 4;
            style.shadow_depth = 2;
            style.safe_margin_percent = 10;
        }
    }
    style
}

pub fn from_storage(value: &str) -> Result<SubtitleStyle> {
    let stored: StoredSubtitleStyle = serde_json::from_str(value)
        .map_err(|error| anyhow!("subtitle_style_snapshot_invalid: {error}"))?;
    Ok(resolve(stored.preset, stored.position))
}

pub fn storage_json(style: &SubtitleStyle) -> Result<String> {
    Ok(serde_json::to_string(&StoredSubtitleStyle {
        preset: style.preset,
        position: style.position,
    })?)
}

pub fn set(db: &mut Connection, project_id: &str, preset: &str, position: &str) -> Result<Project> {
    let preset = SubtitleStylePreset::parse(preset).ok_or_else(|| {
        anyhow!("subtitle_style_preset_invalid: 字幕预设只支持 compact、standard 或 emphasis")
    })?;
    let position = SubtitlePosition::parse(position).ok_or_else(|| {
        anyhow!("subtitle_style_position_invalid: 字幕位置只支持 bottom 或 center")
    })?;
    let next = resolve(preset, position);
    let current = project::load(db, project_id)?;
    if current.subtitle_style == next {
        return Ok(current);
    }
    let transcript = current.transcript.clone();
    let style_json = storage_json(&next)?;
    project::mutate_with_snapshot(db, project_id, "更新字幕样式", |tx| {
        tx.execute(
            "UPDATE projects SET subtitle_style_json=?2,updated_at=?3 WHERE id=?1",
            params![project_id, style_json, crate::util::now()],
        )?;
        Ok(())
    })?;
    let updated = project::load(db, project_id)?;
    if updated.transcript != transcript {
        bail!("subtitle_style_content_changed: 字幕样式设置不得修改正文或时间")
    }
    Ok(updated)
}

fn ass_color(value: &str) -> Result<String> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("subtitle_style_color_invalid: ASS 颜色必须使用 #RRGGBB")
    }
    Ok(format!("&H00{}{}{}", &hex[4..6], &hex[2..4], &hex[0..2]))
}

pub fn play_resolution(canvas: CanvasSettings) -> (u16, u16) {
    match canvas.aspect_ratio {
        CanvasAspectRatio::Source => (1920, 1080),
        CanvasAspectRatio::Vertical => (1080, 1920),
    }
}

pub fn ass_header(style: &SubtitleStyle, canvas: CanvasSettings) -> Result<String> {
    let (play_res_x, play_res_y) = play_resolution(canvas);
    let margin_v = if style.position == SubtitlePosition::Bottom {
        u32::from(play_res_y) * u32::from(style.safe_margin_percent) / 100
    } else {
        0
    };
    let alignment = if style.position == SubtitlePosition::Bottom {
        2
    } else {
        5
    };
    let bold = if style.bold { -1 } else { 0 };
    let primary = ass_color(&style.primary_color)?;
    let secondary = ass_color(&style.secondary_color)?;
    let outline = ass_color(&style.outline_color)?;
    let format = "Format: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding";
    let primary_style = format!(
        "Style: Primary,{},{},{},{},{},&H80000000,{bold},0,0,0,100,100,0,0,1,{},{},{alignment},80,80,{margin_v},1",
        style.font_family,
        style.font_size,
        primary,
        primary,
        outline,
        style.outline_width,
        style.shadow_depth
    );
    let secondary_style = format!(
        "Style: Secondary,{},{},{},{},{},&H80000000,{bold},0,0,0,100,100,0,0,1,{},{},{alignment},80,80,{margin_v},1",
        style.font_family,
        style.secondary_font_size,
        secondary,
        secondary,
        outline,
        style.outline_width,
        style.shadow_depth
    );
    Ok(format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {play_res_x}\nPlayResY: {play_res_y}\nScaledBorderAndShadow: yes\nWrapStyle: 2\n\n[V4+ Styles]\n{format}\n{primary_style}\n{secondary_style}\n\n[Events]\nFormat: Layer,Start,End,Style,Text"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn subtitle_style_presets_resolve_to_a_finite_token_set() {
        let compact = resolve(SubtitleStylePreset::Compact, SubtitlePosition::Bottom);
        let standard = resolve(SubtitleStylePreset::Standard, SubtitlePosition::Bottom);
        let emphasis = resolve(SubtitleStylePreset::Emphasis, SubtitlePosition::Center);
        assert_eq!((compact.font_size, compact.outline_width), (42, 2));
        assert_eq!((standard.font_size, standard.safe_margin_percent), (52, 8));
        assert_eq!(
            (emphasis.font_size, emphasis.position),
            (60, SubtitlePosition::Center)
        );
        assert_eq!(
            from_storage(&storage_json(&emphasis).unwrap()).unwrap(),
            emphasis
        );
        assert_eq!(catalog().len(), 3);
    }

    #[test]
    fn subtitle_style_renders_deterministic_ass_tokens_for_each_canvas() {
        let style = resolve(SubtitleStylePreset::Emphasis, SubtitlePosition::Bottom);
        let source = ass_header(&style, CanvasSettings::default()).unwrap();
        assert!(source.contains("PlayResX: 1920\nPlayResY: 1080"));
        assert!(source.contains("Style: Primary,Microsoft YaHei UI,60,&H00F5F4F2"));
        assert!(source.contains("Style: Secondary,Microsoft YaHei UI,46,&H00C6BEB5"));
        assert!(source.contains(",4,2,2,80,80,108,1"));
        let vertical = ass_header(
            &style,
            CanvasSettings {
                aspect_ratio: CanvasAspectRatio::Vertical,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(vertical.contains("PlayResX: 1080\nPlayResY: 1920"));
        assert!(vertical.contains(",4,2,2,80,80,192,1"));
    }

    #[test]
    fn subtitle_style_setting_is_recoverable_and_never_changes_transcript() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("source.wav");
        fs::write(&media, b"source").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("style.db")).unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        project::add_segment(&mut db, &created.id, 0.0, 1.0, "正文".into(), None).unwrap();
        let before = project::load(&db, &created.id).unwrap();
        let updated = set(&mut db, &created.id, "emphasis", "center").unwrap();
        assert_eq!(updated.transcript, before.transcript);
        assert_eq!(updated.subtitle_style.preset, SubtitleStylePreset::Emphasis);
        assert_eq!(updated.subtitle_style.position, SubtitlePosition::Center);
        let undone = project::undo(&mut db, &created.id).unwrap();
        assert_eq!(undone.subtitle_style, SubtitleStyle::default());
        assert_eq!(undone.transcript, before.transcript);
        let redone = project::redo(&mut db, &created.id).unwrap();
        assert_eq!(redone.subtitle_style, updated.subtitle_style);
    }
}
