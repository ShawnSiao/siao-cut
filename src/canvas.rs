use crate::model::{CanvasAspectRatio, CanvasFraming, CanvasSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTarget {
    Preview,
    Export,
}

pub fn append_transform(
    filters: &mut Vec<String>,
    input: &str,
    output: &str,
    settings: CanvasSettings,
    target: CanvasTarget,
) {
    match settings.aspect_ratio {
        CanvasAspectRatio::Source => match target {
            CanvasTarget::Preview => filters.push(format!(
                "[{input}]scale=w=min(1280\\,iw):h=-2[{output}]"
            )),
            CanvasTarget::Export => filters.push(format!("[{input}]null[{output}]")),
        },
        CanvasAspectRatio::Vertical => match settings.framing {
            CanvasFraming::ContainBlur => {
                filters.push(format!(
                    "[{input}]split=2[{output}_background_source][{output}_foreground_source]"
                ));
                filters.push(format!(
                    "[{output}_background_source]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,gblur=sigma=30,setsar=1[{output}_background]"
                ));
                filters.push(format!(
                    "[{output}_foreground_source]scale=1080:1920:force_original_aspect_ratio=decrease:force_divisible_by=2,setsar=1[{output}_foreground]"
                ));
                filters.push(format!(
                    "[{output}_background][{output}_foreground]overlay=(W-w)/2:(H-h)/2,format=yuv420p[{output}]"
                ));
            }
            CanvasFraming::CoverCenter => filters.push(format!(
                "[{input}]scale=1080:1920:force_original_aspect_ratio=increase:force_divisible_by=2,crop=1080:1920,setsar=1,format=yuv420p[{output}]"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_transform_preserves_source_preview_without_cropping() {
        let mut filters = Vec::new();
        append_transform(
            &mut filters,
            "0:v",
            "preview",
            CanvasSettings::default(),
            CanvasTarget::Preview,
        );
        assert_eq!(filters, ["[0:v]scale=w=min(1280\\,iw):h=-2[preview]"]);
    }

    #[test]
    fn contain_blur_keeps_foreground_and_builds_vertical_background() {
        let mut filters = Vec::new();
        append_transform(
            &mut filters,
            "vcat",
            "canvas",
            CanvasSettings {
                aspect_ratio: CanvasAspectRatio::Vertical,
                framing: CanvasFraming::ContainBlur,
            },
            CanvasTarget::Export,
        );
        let expression = filters.join(";");
        assert!(expression.contains("split=2"));
        assert!(expression.contains("force_original_aspect_ratio=decrease"));
        assert!(expression.contains("overlay=(W-w)/2:(H-h)/2"));
        assert!(expression.contains("format=yuv420p[canvas]"));
    }

    #[test]
    fn cover_center_fills_vertical_canvas_without_padding() {
        let mut filters = Vec::new();
        append_transform(
            &mut filters,
            "vcat",
            "canvas",
            CanvasSettings {
                aspect_ratio: CanvasAspectRatio::Vertical,
                framing: CanvasFraming::CoverCenter,
            },
            CanvasTarget::Export,
        );
        let expression = filters.join(";");
        assert!(expression.contains("force_original_aspect_ratio=increase"));
        assert!(expression.contains("crop=1080:1920"));
        assert!(!expression.contains("pad="));
    }
}
