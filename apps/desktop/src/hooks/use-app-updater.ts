import { useCallback, useEffect, useState } from "react";
import { checkForUpdate, installUpdate } from "../core";
import { tr } from "../i18n";
import type { UpdateMetadata, UpdatePolicy } from "../types";
import { shouldCheckForUpdates } from "../app-view-model";

const LAST_CHECKED_AT_KEY = "siaocut.updateLastCheckedAt";

export function useAppUpdater(onNoUpdate: (message: string) => void) {
  const [updatePolicy, setUpdatePolicy] = useState<UpdatePolicy | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<UpdateMetadata | null>(null);
  const [updateBusy, setUpdateBusy] = useState<string | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  const checkUpdates = useCallback(async (automatic = false) => {
    if (!updatePolicy?.enabled)
      return;
    setUpdateBusy(tr("app.s0047"));
    setUpdateError(null);
    try {
      const candidate = await checkForUpdate();
      localStorage.setItem(LAST_CHECKED_AT_KEY, new Date().toISOString());
      setAvailableUpdate(candidate);
      if (!automatic && !candidate)
        onNoUpdate(tr("app.s0048"));
    }
    catch (cause) {
      if (!automatic)
        setUpdateError(cause instanceof Error ? cause.message : String(cause));
    }
    finally {
      setUpdateBusy(null);
    }
  }, [onNoUpdate, updatePolicy?.enabled]);

  useEffect(() => {
    if (!updatePolicy || !shouldCheckForUpdates(localStorage.getItem(LAST_CHECKED_AT_KEY), Date.now(), updatePolicy.enabled))
      return;
    void checkUpdates(true);
  }, [checkUpdates, updatePolicy]);

  const confirmUpdateInstall = useCallback(async () => {
    if (!availableUpdate)
      return;
    setUpdateBusy(tr("app.s0049"));
    setUpdateError(null);
    try {
      await installUpdate((event) => {
        if (event.event === "Verifying")
          setUpdateBusy(tr("app.s0050"));
      });
    }
    catch (cause) {
      setUpdateError(cause instanceof Error ? cause.message : String(cause));
      setUpdateBusy(null);
    }
  }, [availableUpdate]);

  return {
    updatePolicy,
    setUpdatePolicy,
    availableUpdate,
    updateBusy,
    updateError,
    checkUpdates,
    confirmUpdateInstall,
  };
}
