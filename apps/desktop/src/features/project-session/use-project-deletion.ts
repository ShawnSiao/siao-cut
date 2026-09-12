import { useRef,useState } from "react";
import { projectSessionClient } from "../../domains/project-session-client";
import type { ProjectSummary } from "../../generated/core-contract";
import { tr } from "../../i18n";
import type { ProjectDeletionPreflight } from "../../types";

interface Inputs { projects: ProjectSummary[]; onDeleted: (project: ProjectSummary) => Promise<void> }
/** Keeps deletion consent bound to the exact preflight and prevents duplicate commands. */
export function useProjectDeletion(inputs: Inputs) {
    const { projects } = inputs;
    const latest = useRef(inputs); latest.current = inputs;
    const deletingRef = useRef(false);
    const openingRef = useRef(false);
    const preflightGeneration = useRef(0);
    const [deleteCandidate, setDeleteCandidate] = useState<ProjectSummary | null>(null);
    const [deleteBusy, setDeleteBusy] = useState(false);
    const [deleteError, setDeleteError] = useState<string | null>(null);
    const [deletionPreflight, setDeletionPreflight] = useState<ProjectDeletionPreflight | null>(null);
    const [deletePreflightBusy, setDeletePreflightBusy] = useState(false);
    const currentDeleteCandidate = deleteCandidate ? projects.find((item) => item.id === deleteCandidate.id) ?? deleteCandidate : null;
    const deleteBlockMessage = deletionPreflight?.blockers.length
        ? deletionPreflight.blockers.map((blocker) => ({
            agent_task: tr("app.delete.blocker.agent"),
            export: tr("app.delete.blocker.export"),
            audio_analysis: tr("app.delete.blocker.audio"),
            speaker_analysis: tr("app.delete.blocker.speaker"),
            auto_workflow: tr("app.delete.blocker.workflow"),
            transcription: blocker.status === "awaiting_apply" ? tr("app.delete.blocker.transcriptionCandidate") : tr("app.delete.blocker.transcription"),
        }[blocker.kind] ?? tr("app.delete.blocker.unknown", { kind: blocker.kind, status: blocker.status }))).join(" ")
        : null;
    const refreshDeletionPreflight = async (projectId: string) => {
        const generation = ++preflightGeneration.current;
        const envelope = await projectSessionClient.deletePreflight(projectId);
        if (!envelope.deletionPreflight)
            throw new Error(tr("app.delete.preflightMissing"));
        if (generation === preflightGeneration.current) setDeletionPreflight(envelope.deletionPreflight);
        return envelope.deletionPreflight;
    };
    const openDeleteDialog = (candidate: ProjectSummary) => {
        if (deletingRef.current || openingRef.current) return;
        openingRef.current = true;
        setDeleteError(null);
        setDeletionPreflight(null);
        setDeleteCandidate(candidate);
        setDeletePreflightBusy(true);
        void refreshDeletionPreflight(candidate.id).catch((cause) => {
            setDeleteError(cause instanceof Error ? cause.message : String(cause));
        }).finally(() => { openingRef.current = false; setDeletePreflightBusy(false); });
    };
    const closeDeleteDialog = () => {
        if (deleteBusy || deletePreflightBusy)
            return;
        setDeleteCandidate(null);
        setDeleteError(null);
        setDeletionPreflight(null);
    };
    const deleteProject = async () => {
        if (deletingRef.current || !currentDeleteCandidate || deletePreflightBusy || !deletionPreflight || deletionPreflight.projectId !== currentDeleteCandidate.id)
            return;
        const deleting = currentDeleteCandidate;
        const confirmedPreflight = deletionPreflight;
        deletingRef.current = true;
        setDeleteBusy(true);
        setDeleteError(null);
        try {
            if (!confirmedPreflight.deletable)
                return;
            await projectSessionClient.deleteProject(deleting.id, confirmedPreflight.expectedVersionId);
            setDeleteCandidate(null);
            await latest.current.onDeleted(deleting);
        }
        catch (cause) {
            const message = cause instanceof Error ? cause.message : String(cause);
            const versionMismatch = message.includes("project_delete_version_mismatch");
            setDeleteError(versionMismatch
                ? tr("app.projectDelete.versionMismatch")
                : message.replace(/^project_busy:\s*/, ""));
            if (versionMismatch)
                setDeletionPreflight(null);
            else
                await refreshDeletionPreflight(deleting.id).catch(() => undefined);
        }
        finally {
            deletingRef.current = false;
            setDeleteBusy(false);
        }
    };
    return { currentDeleteCandidate, deleteBusy, deleteError, deletionPreflight, deletePreflightBusy, deleteBlockMessage, openDeleteDialog, closeDeleteDialog, deleteProject };
}
