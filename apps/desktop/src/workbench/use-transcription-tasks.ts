import { useCallback, useEffect, useRef, useState } from "react";
import { backgroundTaskClient } from "../domains/background-task-client";
import type { TranscriptionJob } from "../types";

const active = (job: TranscriptionJob) => ["queued", "running", "finalizing"].includes(job.status);

export function useTranscriptionTasks(onApplied: (job: TranscriptionJob) => Promise<void>) {
  const [jobs, setJobs] = useState<TranscriptionJob[]>([]);
  const [error, setError] = useState<string | null>(null);
  const current = useRef(jobs), callback = useRef(onApplied);
  const wake = useRef(() => {});
  const reload = useRef(() => {});
  callback.current = onApplied;
  const track = useCallback((job: TranscriptionJob | null) => {
    if (!job) return;
    const previous = current.current.find((item) => item.id === job.id);
    if (previous && previous.attemptCount === job.attemptCount && !active(previous) && active(job)) return;
    if (previous && (previous.attemptCount > job.attemptCount || Date.parse(previous.updatedAt) > Date.parse(job.updatedAt))) return;
    current.current = [...current.current.filter((item) => item.id !== job.id), job];
    setJobs(current.current);
    wake.current();
  }, []);
  useEffect(() => {
    let disposed = false, running = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => { if (!disposed && !timer && !running && current.current.some(active)) timer = setTimeout(() => void poll(), 800); };
    const poll = async () => {
      timer = undefined;
      if (disposed || running) return;
      running = true;
      try {
        for (const job of current.current.filter(active)) {
          if (disposed) break;
          const result = await backgroundTaskClient.getTranscriptionJob(job.id);
          if (disposed) break;
          if (result.transcriptionJob) {
            track(result.transcriptionJob);
            if (result.transcriptionJob.status === "completed") await callback.current(result.transcriptionJob);
          }
        }
        if (!disposed) setError(null);
      } catch (cause) { if (!disposed) setError(String(cause)); }
      finally { running = false; schedule(); }
    };
    wake.current = schedule;
    reload.current = () => {
      if (running || disposed) return;
      clearTimeout(timer); timer = undefined; running = true;
      void backgroundTaskClient.listTranscriptions().then((result) => {
        if (!disposed) { for (const job of result.transcriptionJobs ?? []) track(job); setError(null); }
      }).catch((cause) => { if (!disposed) setError(String(cause)); }).finally(() => { running = false; schedule(); });
    };
    reload.current();
    schedule();
    return () => { disposed = true; clearTimeout(timer); wake.current = () => {}; reload.current = () => {}; };
  }, []);
  return { jobs, track, error, refresh: () => reload.current() };
}
