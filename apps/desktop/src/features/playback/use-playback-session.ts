import { useRef, useState, type SyntheticEvent } from "react";
import { authorizeArtifact, authorizeMedia } from "../../domains/desktop-platform-client";
import type { CutPreview, Project } from "../../types";

export async function resolveCanvasMedia(
  projectId: string,
  authorizePreview: typeof authorizeArtifact = authorizeArtifact,
  authorizeSource: typeof authorizeMedia = authorizeMedia,
) {
  let warning: string | null = null;
  try {
    const preview = await authorizePreview(projectId, "preview");
    if (preview)
      return { mediaUrl: preview, warning: null };
  }
  catch (cause) {
    warning = cause instanceof Error ? cause.message : String(cause);
  }
  try {
    return { mediaUrl: await authorizeSource(projectId), warning };
  }
  catch (cause) {
    const sourceWarning = cause instanceof Error ? cause.message : String(cause);
    return { mediaUrl: null, warning: warning ? `${warning}; ${sourceWarning}` : sourceWarning };
  }
}

export async function resolveImportedProjectMedia(
  projectId: string,
  authorizeProjectArtifact: typeof authorizeArtifact = authorizeArtifact,
  authorizeProjectSource: typeof authorizeMedia = authorizeMedia,
) {
  const [canvas, waveform] = await Promise.all([
    resolveCanvasMedia(projectId, authorizeProjectArtifact, authorizeProjectSource),
    authorizeProjectArtifact(projectId, "waveform")
      .then((waveformUrl) => ({ waveformUrl, warning: null as string | null }))
      .catch((cause) => ({
        waveformUrl: null,
        warning: cause instanceof Error ? cause.message : String(cause),
      })),
  ]);
  return {
    mediaUrl: canvas.mediaUrl,
    waveformUrl: waveform.waveformUrl,
    warning: [canvas.warning, waveform.warning].filter(Boolean).join("; ") || null,
  };
}

export function resolvePlaybackDuration(mediaDuration: number, fallbackDuration: number | null | undefined) {
  return Number.isFinite(mediaDuration) && mediaDuration > 0 ? mediaDuration : fallbackDuration ?? 0;
}

export function usePlaybackSession(project: Project | null) {
  const [mediaUrl, setMediaUrl] = useState<string | null>(null);
  const [waveformUrl, setWaveformUrl] = useState<string | null>(null);
  const [cutPreview, setCutPreview] = useState<CutPreview | null>(null);
  const [playback, setPlayback] = useState({ playing: false, currentTime: 0, duration: 0 });
  const videoRef = useRef<HTMLVideoElement>(null);
  const handleVideoTimeUpdate = () => {
    const video = videoRef.current;
    if (!video || !project)
      return;
    if (cutPreview) {
      if (video.currentTime >= cutPreview.cutStart && video.currentTime < cutPreview.cutEnd - 0.01) {
        video.currentTime = cutPreview.cutEnd;
        return;
      }
      if (video.currentTime >= cutPreview.previewEnd) {
        video.pause();
        setCutPreview(null);
        return;
      }
    }
    const cut = project.timeline.cuts.find((candidate) => video.currentTime >= candidate.sourceStart && video.currentTime < candidate.sourceEnd - 0.01);
    if (cut)
      video.currentTime = cut.sourceEnd;
    setPlayback((current) => ({
      ...current,
      currentTime: video.currentTime,
      duration: Number.isFinite(video.duration) ? video.duration : current.duration,
    }));
  };
  const handleVideoLoadedMetadata = (event: SyntheticEvent<HTMLVideoElement>) => {
    // React clears SyntheticEvent.currentTarget after this callback returns. Capture the
    // DOM value before entering a state updater, which React may invoke later.
    const mediaDuration = event.currentTarget.duration;
    const fallbackDuration = project?.media.durationSeconds;
    setPlayback((current) => ({
      ...current,
      duration: resolvePlaybackDuration(mediaDuration, fallbackDuration),
    }));
  };
  const seekTimeline = (time: number) => {
    const duration = playback.duration || project?.media.durationSeconds || project?.timeline.sourceDuration || 0;
    const nextTime = Math.max(0, Math.min(duration, Number.isFinite(time) ? time : 0));
    if (videoRef.current)
      videoRef.current.currentTime = nextTime;
    setPlayback((current) => ({ ...current, currentTime: nextTime }));
  };
  const toggleTimelinePlayback = () => {
    const video = videoRef.current;
    if (!video)
      return;
    if (video.paused)
      void video.play();
    else
      video.pause();
  };
  return { mediaUrl, setMediaUrl, waveformUrl, setWaveformUrl, cutPreview, setCutPreview, playback, setPlayback, videoRef, handleVideoTimeUpdate, handleVideoLoadedMetadata, seekTimeline, toggleTimelinePlayback };
}
