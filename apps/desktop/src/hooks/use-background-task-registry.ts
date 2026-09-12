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
  const running = useRef(new Map<string,{interval:number;stop:()=>void}>());
  const requests = useRef(new Map<string,Promise<unknown>>());
  useEffect(() => {
    for (const [key,loop] of running.current) {
      if (latest.current.get(key)?.intervalMs !== loop.interval) { loop.stop(); running.current.delete(key); }
    }
    for (const registration of latest.current.values()) {
      if (running.current.has(registration.key)) continue;
      const poll = () => {
        const pending=requests.current.get(registration.key); if (pending) return pending;
        const request=Promise.resolve().then(()=>latest.current.get(registration.key)?.poll()).finally(()=>{
          if(requests.current.get(registration.key)===request) requests.current.delete(registration.key);
        });
        requests.current.set(registration.key,request);return request;
      };
      running.current.set(registration.key,{interval:registration.intervalMs,stop:startSerialPolling(poll,registration.intervalMs)});
    }
  },[registryKey]);
  useEffect(()=>()=>{for(const loop of running.current.values())loop.stop();running.current.clear();},[]);
}
