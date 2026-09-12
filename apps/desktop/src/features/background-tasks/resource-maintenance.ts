import { runtimeInfo } from "../../domains/desktop-platform-client";
import { localResourceClient } from "../../domains/local-resource-client";
import { tr } from "../../i18n";
import type { LocalCapabilityId,LocalResourceStatus,RuntimeInfo } from "../../types";
import { localCapabilityLabel,localResourceError } from "./resource-messages";
type Options = {
  setResourceBusy: (busy: boolean) => void;
  setLocalResources: (status: LocalResourceStatus) => void;
  setRuntime: (runtime: RuntimeInfo) => void;
  setNotice: (notice: string | null) => void;
  setError: (error: string | null) => void;
};
export function resourceMaintenance(options: Options) {
    const removeResourceCapability = async (capability: LocalCapabilityId) => {
        if (!window.confirm(tr("app.resources.removeConfirm", { capability: localCapabilityLabel(capability) })))
            return;
        options.setResourceBusy(true);
        try {
            const envelope = await localResourceClient.remove(capability);
            if (envelope.localResources)
                options.setLocalResources(envelope.localResources);
            options.setRuntime(await runtimeInfo());
            options.setNotice(tr("app.resources.removedNotice", { capability: localCapabilityLabel(capability) }));
        }
        catch (cause) {
            options.setError(localResourceError(cause));
        }
        finally {
            options.setResourceBusy(false);
        }
    };
    const cleanupLocalResources = async () => {
        if (!window.confirm(tr("app.resources.cleanupConfirm")))
            return;
        options.setResourceBusy(true);
        try {
            const envelope = await localResourceClient.cleanup();
            if (envelope.localResources)
                options.setLocalResources(envelope.localResources);
            options.setNotice(tr("app.resources.cleanupNotice"));
        }
        catch (cause) {
            options.setError(localResourceError(cause));
        }
        finally {
            options.setResourceBusy(false);
        }
    };
    const rollbackResourceCapability = async (capability: LocalCapabilityId) => {
        if (!window.confirm(tr("app.resources.rollbackConfirm", { capability: localCapabilityLabel(capability) })))
            return;
        options.setResourceBusy(true);
        try {
            const envelope = await localResourceClient.rollback(capability);
            if (envelope.localResources)
                options.setLocalResources(envelope.localResources);
            options.setRuntime(await runtimeInfo());
            options.setNotice(tr("app.resources.rollbackNotice", { capability: localCapabilityLabel(capability) }));
        }
        catch (cause) {
            options.setError(localResourceError(cause));
        }
        finally {
            options.setResourceBusy(false);
        }
    };
    return {removeResourceCapability, cleanupLocalResources, rollbackResourceCapability};
}
