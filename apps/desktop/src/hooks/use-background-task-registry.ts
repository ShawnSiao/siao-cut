import { useEffect, useRef } from "react";
import { startSerialPolling } from "../app-view-model";

export type BackgroundTaskRegistration = {
  key: string;
  intervalMs: number;
  poll: () => Promise<unknown>;
};

export function startBackgroundTaskRegistry(registrations: BackgroundTaskRegistration[]) {
  const stops = registrations.map((registration) => startSerialPolling(
    () => registration.poll().then(() => undefined),
    registration.intervalMs,
  ));
  return () => stops.forEach((stop) => stop());
}

export function useBackgroundTaskRegistry(registrations: Array<BackgroundTaskRegistration | null>) {
  const active = registrations.filter((registration): registration is BackgroundTaskRegistration => registration !== null);
  const latest = useRef(new Map<string, BackgroundTaskRegistration>());
  latest.current = new Map(active.map((registration) => [registration.key, registration]));
  const registryKey = active.map((registration) => `${registration.key}:${registration.intervalMs}`).join("|");

  useEffect(() => startBackgroundTaskRegistry(active.map((registration) => ({
    key: registration.key,
    intervalMs: registration.intervalMs,
    poll: () => latest.current.get(registration.key)?.poll() ?? Promise.resolve(),
  }))), [registryKey]);
}
