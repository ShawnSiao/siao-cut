use super::*;
use std::{fs, process::Command};
use tempfile::tempdir;

#[test]
fn real_media_embeds_container_compatible_text_subtitle_tracks() {
    if !crate::media::command_available("ffmpeg") || !crate::media::command_available("ffprobe") {
        return;
    }
    let temp = tempdir().unwrap();
    let source = temp.path().join("embedded-source.mp4");
    let generated = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=0x202020:s=320x180:r=24:d=2",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=stereo",
            "-t",
            "2",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(generated.success());
    let subtitle = temp.path().join("captions.srt");
    fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:01,000\nHello\n\n2\n00:00:01,000 --> 00:00:02,000\n世界\n",
    )
    .unwrap();
    let map = TimelineMap {
        source_duration: 2.0,
        output_duration: 2.0,
        kept_ranges: vec![crate::model::TimelineRange {
            source_start: 0.0,
            source_end: 2.0,
            output_start: 0.0,
            output_end: 2.0,
        }],
        cuts: Vec::new(),
    };

    for (delivery, extension, expected_codec) in [
        (SubtitleDelivery::EmbeddedMp4, "mp4", "mov_text"),
        (SubtitleDelivery::EmbeddedMkv, "mkv", "subrip"),
    ] {
        let output = temp.path().join(format!("embedded.{extension}"));
        let status = build_command(CommandSpec {
            ffmpeg: "ffmpeg",
            source: &source,
            output: &output,
            map: &map,
            has_video: true,
            has_audio: true,
            subtitle_path: Some(&subtitle),
            subtitle_delivery: delivery,
            subtitle_language: Some("zh"),
            encoder: "mpeg4",
            canvas_settings: Default::default(),
        })
        .unwrap()
        .status()
        .unwrap();
        assert!(
            status.success(),
            "failed to create {extension} subtitle track"
        );
        let probed = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "s:0",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(probed.status.success());
        assert_eq!(
            String::from_utf8_lossy(&probed.stdout).trim(),
            expected_codec
        );
    }
}
