import { useState } from "react";
import { hasMeaningfulSubtitleText, type StructureEditMode } from "../../app-view-model";
import { tr } from "../../i18n";
import type { CoreEnvelope, Project, Segment, SubtitleStructureEdit } from "../../types";
import type { EditingSession } from "./editing-session";
type Inputs = { project: Project | null; selectedSegments: Segment[]; editing: EditingSession; setSelectedId: (id: string | null) => void; setSelectedSegmentIds: (ids: string[]) => void; setSelectionAnchorId: (id: string | null) => void; setNotice: (value: string | null) => void; withBusy: (label: string, action: () => Promise<void>) => Promise<void>; onSaveField: (segment: Segment, text: string) => Promise<void>; onApplied: (result: SubtitleStructureEdit) => Promise<void> };
export function useStructureEditing({ project, selectedSegments, editing, setSelectedId, setSelectedSegmentIds, setSelectionAnchorId, setNotice, withBusy, onSaveField, onApplied }: Inputs) {
  const [structureEditMode, setStructureEditMode] = useState<StructureEditMode | null>(null);
  const [structureStart, setStructureStart] = useState("");
  const [structureEnd, setStructureEnd] = useState("");
  const [structureTextOffset, setStructureTextOffset] = useState("");
  const [structureDelta, setStructureDelta] = useState("0.100");
  const [structureBusy, setStructureBusy] = useState(false);
  const [structureError, setStructureError] = useState<string | null>(null);
  const firstSelectedIndex = project?.transcript.segments.findIndex((segment) => segment.id === selectedSegments[0]?.id) ?? -1;
  const secondSelectedIndex = project?.transcript.segments.findIndex((segment) => segment.id === selectedSegments[1]?.id) ?? -1;
  const mergeCandidatesAdjacent = selectedSegments.length === 2 && firstSelectedIndex >= 0 && secondSelectedIndex === firstSelectedIndex + 1;
  const splitTextOffset = Number(structureTextOffset);
  const splitCharacters = Array.from(selectedSegments[0]?.text ?? "");
  const splitLeftText = splitCharacters.slice(0, Number.isInteger(splitTextOffset) ? splitTextOffset : 0).join("").trim();
  const splitRightText = splitCharacters.slice(Number.isInteger(splitTextOffset) ? splitTextOffset : 0).join("").trim();
  const splitInputsValid = Number.isInteger(splitTextOffset)
    && splitTextOffset > 0
    && splitTextOffset < splitCharacters.length
    && Number(structureStart) > (selectedSegments[0]?.start ?? Number.POSITIVE_INFINITY)
    && Number(structureStart) < (selectedSegments[0]?.end ?? Number.NEGATIVE_INFINITY)
    && hasMeaningfulSubtitleText(splitLeftText)
    && hasMeaningfulSubtitleText(splitRightText);
  const timingStart = Number(structureStart);
  const timingEnd = Number(structureEnd);
  const timingInputsValid = Number.isFinite(timingStart)
    && Number.isFinite(timingEnd)
    && timingStart >= 0
    && timingEnd > timingStart;
  const timingChanged = Boolean(selectedSegments[0])
    && (Math.abs(timingStart - selectedSegments[0].start) >= 0.0005 || Math.abs(timingEnd - selectedSegments[0].end) >= 0.0005);
  const structureSubmitDisabled = structureBusy || (structureEditMode === "split" && !splitInputsValid)
    || (structureEditMode === "merge" && !mergeCandidatesAdjacent)
    || (structureEditMode === "timing" && (!timingInputsValid || !timingChanged))
    || (structureEditMode === "offset" && (!Number.isFinite(Number(structureDelta)) || Number(structureDelta) === 0));
  const openStructureEdit = (mode: StructureEditMode, targetOverride?: Segment, textOffsetOverride?: number, useWordTiming = true) => {
    const target = targetOverride ?? selectedSegments[0];
    if (!project || !target)
      return;
    if (targetOverride) {
      setSelectedId(target.id);
      setSelectedSegmentIds([target.id]);
      setSelectionAnchorId(target.id);
    }
    setStructureError(null);
    if (mode === "split") {
      const characterCount = Array.from(target.text).length;
      const requestedOffset = Math.max(1, Math.min(characterCount - 1, textOffsetOverride ?? Math.floor(characterCount / 2)));
      const targetWords = useWordTiming ? project.transcript.words
        .filter((word) => word.segmentId === target.id && Number.isFinite(word.start) && Number.isFinite(word.end) && word.start >= target.start && word.end <= target.end && word.end > word.start)
        .sort((left, right) => left.start - right.start) : [];
      let scanFrom = 0;
      const wordBoundaries = targetWords.slice(0, -1).flatMap((word) => {
        const index = target.text.indexOf(word.text, scanFrom);
        if (index < 0)
          return [];
        scanFrom = index + word.text.length;
        return [{ textOffset: Array.from(target.text.slice(0, scanFrom)).length, at: word.end }];
      });
      const credibleBoundary = wordBoundaries
        .filter((boundary) => boundary.textOffset > 0 && boundary.textOffset < characterCount && boundary.at > target.start && boundary.at < target.end)
        .sort((left, right) => Math.abs(left.textOffset - requestedOffset) - Math.abs(right.textOffset - requestedOffset))[0];
      setStructureTextOffset(String(credibleBoundary?.textOffset ?? requestedOffset));
      setStructureStart(credibleBoundary ? credibleBoundary.at.toFixed(3) : "");
    }
    else if (mode === "timing") {
      setStructureStart(target.start.toFixed(3));
      setStructureEnd(target.end.toFixed(3));
    }
    else if (mode === "offset") {
      setStructureDelta("0.100");
    }
    setStructureEditMode(mode);
  };
  const saveBeforeStructureEdit = async (segment: Segment, draft: string) => {
    const text = draft.trim();
    if (!project || text === segment.text)
      return { saved: true, segment };
    let saved = false;
    await withBusy(tr("app.s0153"), async () => {
      await onSaveField(segment, text);
      setNotice(tr("app.s0154"));
      saved = true;
    });
    return { saved, segment: { ...segment, text } };
  };
  const splitSegmentFromEditor = async (segment: Segment, draft: string, textOffset: number) => {
    const changed = draft.trim() !== segment.text;
    const result = await saveBeforeStructureEdit(segment, draft);
    if (!result.saved)
      return;
    openStructureEdit("split", result.segment, textOffset, !changed);
  };
  const mergePreviousFromEditor = async (segment: Segment, draft: string) => {
    if (!project)
      return;
    const result = await saveBeforeStructureEdit(segment, draft);
    if (!result.saved)
      return;
    const index = project.transcript.segments.findIndex((candidate) => candidate.id === segment.id);
    const previous = project.transcript.segments[index - 1];
    if (!previous) {
      setNotice(tr("app.creator.editor.noPrevious"));
      return;
    }
    setSelectedId(previous.id);
    setSelectedSegmentIds([previous.id, segment.id]);
    setSelectionAnchorId(previous.id);
    setStructureError(null);
    setStructureEditMode("merge");
  };
  const applyStructureEdit = async () => {
    if (!project || !structureEditMode || !selectedSegments.length)
      return;
    setStructureBusy(true);
    setStructureError(null);
    try {
      let request: Promise<CoreEnvelope>;
      if (structureEditMode === "split") {
        const textOffset = Number(structureTextOffset);
        const at = Number(structureStart);
        if (!Number.isInteger(textOffset) || textOffset <= 0 || !Number.isFinite(at))
          throw new Error(tr("app.s0158"));
        if (!hasMeaningfulSubtitleText(splitLeftText) || !hasMeaningfulSubtitleText(splitRightText))
          throw new Error(tr("app.structure.splitMeaningful"));
        request = editing.mutate(project.id, { kind: "split", segmentId: selectedSegments[0].id, textOffset, at });
      }
      else if (structureEditMode === "merge") {
        if (!mergeCandidatesAdjacent)
          throw new Error(tr("app.s0159"));
        request = editing.mutate(project.id, { kind: "merge", firstId: selectedSegments[0].id, secondId: selectedSegments[1].id });
      }
      else if (structureEditMode === "timing") {
        const start = Number(structureStart);
        const end = Number(structureEnd);
        if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end <= start)
          throw new Error(tr("app.s0160"));
        if (!timingChanged)
          throw new Error(tr("app.structure.timingUnchanged"));
        request = editing.mutate(project.id, { kind: "timing", segmentId: selectedSegments[0].id, start, end });
      }
      else {
        const delta = Number(structureDelta);
        if (!Number.isFinite(delta) || delta === 0)
          throw new Error(tr("app.s0161"));
        request = editing.mutate(project.id, { kind: "offset", segmentIds: selectedSegments.map((segment) => segment.id), delta });
      }
      const envelope = await request;
      if (!envelope.structureEdit?.project)
        throw new Error(tr("app.s0162"));
      const result = envelope.structureEdit;
      const nextProject = result.project;
      await onApplied(result);
      setStructureEditMode(null);
      const messages: Record<StructureEditMode, string> = {
        split: tr("app.s0163"),
        merge: tr("app.s0164"),
        timing: tr("app.s0165"),
        offset: tr("app.s0166", { "0": selectedSegments.length, "1": Number(structureDelta) > 0 ? "+" : "", "2": Number(structureDelta).toFixed(3) }),
      };
      setNotice(messages[structureEditMode]);
    }
    catch (cause) {
      setStructureError(cause instanceof Error ? cause.message : String(cause));
    }
    finally {
      setStructureBusy(false);
    }
  };
  return { structureEditMode, setStructureEditMode, structureStart, setStructureStart, structureEnd, setStructureEnd, structureTextOffset, setStructureTextOffset, structureDelta, setStructureDelta, structureBusy, setStructureBusy, structureError, setStructureError, firstSelectedIndex, secondSelectedIndex, mergeCandidatesAdjacent, splitTextOffset, splitCharacters, splitLeftText, splitRightText, splitInputsValid, timingStart, timingEnd, timingInputsValid, timingChanged, structureSubmitDisabled, openStructureEdit, splitSegmentFromEditor, mergePreviousFromEditor, applyStructureEdit };
}
