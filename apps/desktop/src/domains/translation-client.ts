import { runCore } from "../core";

export const translationClient = {
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
