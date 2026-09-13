import type { AutoWorkflow } from "../../types";
export function upsertById<T extends { id: string }>(items: T[], next: T): T[] {
    const index = items.findIndex((item) => item.id === next.id);
    if (index < 0)
        return [next, ...items];
    return items.map((item) => item.id === next.id ? next : item);
}

export function upsertAutoWorkflowSnapshot(items: AutoWorkflow[], next: AutoWorkflow): AutoWorkflow[] {
    const current = items.find((item) => item.id === next.id);
    if (current && Date.parse(current.updatedAt) > Date.parse(next.updatedAt))
        return items;
    return upsertById(items, next);
}

export function selectAutoWorkflowSnapshot(current: AutoWorkflow | null, next: AutoWorkflow): AutoWorkflow | null {
    if (current?.id !== next.id)
        return current;
    return Date.parse(current.updatedAt) > Date.parse(next.updatedAt) ? current : { ...next };
}

export const AUTO_WORKFLOW_DISMISSED_STORAGE_KEY = "siaocut.dismissedAutoWorkflows.v1";
export const ACTIVE_AUTO_WORKFLOW_STATUSES = new Set(["queued", "running", "needs_agent", "awaiting_authorization", "needs_review"]);
export const ACTIONABLE_AUTO_WORKFLOW_STATUSES = new Set([...ACTIVE_AUTO_WORKFLOW_STATUSES, "failed", "interrupted"]);
export const TERMINAL_AUTO_WORKFLOW_STATUSES = new Set(["completed", "cancelled", "failed", "interrupted"]);

export function parseDismissedAutoWorkflowIds(value: string | null): string[] {
    if (!value)
        return [];
    try {
        const parsed: unknown = JSON.parse(value);
        if (!Array.isArray(parsed))
            return [];
        return Array.from(new Set(parsed
            .filter((item): item is string => typeof item === "string")
            .map((item) => item.trim())
            .filter(Boolean)));
    }
    catch {
        return [];
    }
}
