import type { SpeakerTrack } from "../../types";

export const emptySpeakerTrack = (): SpeakerTrack => ({
  status: "not_analyzed",
  runtimeVersion: "sherpa-onnx 1.13.2",
  segmentationModel: "pyannote segmentation 3.0 int8",
  embeddingModel: "3D-Speaker ERes2Net Base 16 kHz",
  providerId: "legacy_diarization",
  modelId: "",
  sourceKind: "cascade",
  generatedAt: null,
  speakers: [],
  turns: [],
  associations: [],
});

export const analyzedSpeakerTrack = (): SpeakerTrack => {
  const createdAt = new Date().toISOString();
  return {
    ...emptySpeakerTrack(),
    status: "ready",
    generatedAt: createdAt,
    speakers: [
      { id: "voice-a", sourceLabel: "speaker_00", label: "说话人 1", colorIndex: 0, createdAt },
      { id: "voice-b", sourceLabel: "speaker_01", label: "说话人 2", colorIndex: 1, createdAt },
    ],
    turns: [
      { id: "turn-a", speakerId: "voice-a", start: 12.4, end: 18.6, confidence: null, source: "sherpa-onnx", modelVersion: "sherpa-onnx 1.13.2", createdAt },
      { id: "turn-b", speakerId: "voice-b", start: 18.6, end: 27.2, confidence: null, source: "sherpa-onnx", modelVersion: "sherpa-onnx 1.13.2", createdAt },
    ],
    associations: [
      { segmentId: "s1", speakerId: "voice-a", source: "overlap", confidence: 1, updatedAt: createdAt },
      { segmentId: "s2", speakerId: "voice-a", source: "overlap", confidence: 1, updatedAt: createdAt },
      { segmentId: "s3", speakerId: "voice-b", source: "overlap", confidence: 1, updatedAt: createdAt },
      { segmentId: "s4", speakerId: "voice-b", source: "overlap", confidence: 1, updatedAt: createdAt },
    ],
  };
};
