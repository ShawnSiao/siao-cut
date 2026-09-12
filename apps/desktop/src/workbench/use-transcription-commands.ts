import { useRef } from "react";
import { backgroundTaskClient } from "../domains/background-task-client";

// Keep the identifier after an ambiguous transport failure; retry the same operation.
export function useTranscriptionCommands() {
  const ids = useRef(new Map<string, string>());
  const id = (key: string) => {
    if (!ids.current.has(key)) ids.current.set(key, crypto.randomUUID());
    return ids.current.get(key)!;
  };
  return {
    start: (projectId: string, model: string, language: string, version: string) => backgroundTaskClient.startWhisper(projectId, model, language, version, id(JSON.stringify(["start", projectId, model, language, version]))),
    retry: (jobId: string, attempt: number) => backgroundTaskClient.resumeTranscription(jobId, id(`retry:${jobId}:${attempt}`)),
    apply: (jobId: string, version: string) => backgroundTaskClient.applyTranscription(jobId, version, id(`apply:${jobId}:${version}`)),
    discard: (jobId: string) => backgroundTaskClient.discardTranscription(jobId, id(`discard:${jobId}`)),
  };
}
