import { runCore } from "../core";

export const translationClient = {
  editSegment: (
    projectId: string,
    segmentId: string,
    language: string,
    text: string,
    expectedVersion: string,
  ) => runCore([
    "translation", "edit", projectId, segmentId,
    "--lang", language,
    "--text", text,
    "--expected-version", expectedVersion,
  ]),
  replaceGlossary: (
    projectId: string,
    language: string,
    expectedVersion: number,
    entries: Array<{ source: string; target: string }>,
  ) => runCore([
    "glossary", "replace", projectId,
    "--lang", language,
    "--expected-version", String(expectedVersion),
    ...entries.flatMap((entry) => ["--entry", `${entry.source}=${entry.target}`]),
  ]),
};
