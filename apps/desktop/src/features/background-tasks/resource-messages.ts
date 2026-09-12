import { tr } from "../../i18n";
import type { LocalCapabilityId } from "../../types";
export const RESOURCE_SETUP_DEFERRED_KEY = "siaocut.localResourcesSetupDeferred.v1";
export function localCapabilityLabel(capability: LocalCapabilityId) {
    return {
        basic_media: tr("app.resources.capability.basic_media"),
        url_import: tr("app.resources.capability.url_import"),
        local_transcription: tr("app.resources.capability.local_transcription"),
        speaker_identity: tr("app.resources.capability.speaker_identity"),
    }[capability];
}

export function localResourceError(error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    const code = message.split(":", 1)[0];
    return ({
        resource_setup_required: tr("app.resources.error.locationRequired"),
        resource_root_unavailable: tr("app.resources.error.locationUnavailable"),
        resource_root_not_writable: tr("app.resources.error.locationUnavailable"),
        resource_root_low_space: tr("app.resources.error.lowSpace"),
        resource_insufficient_space: tr("app.resources.error.lowSpace"),
        resource_job_active: tr("app.resources.error.active"),
        resource_move_target_not_empty: tr("app.resources.error.locationNotEmpty"),
        resource_move_target_invalid: tr("app.resources.error.locationNested"),
    } as Record<string, string>)[code] ?? tr("app.resources.error.generic");
}
