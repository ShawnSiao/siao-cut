export type AiExecutionSelection =
  | { kind: "codex" }
  | { kind: "copy_prompt" }
  | {
      kind: "api";
      serviceConfigId: string;
      serviceRevision: number;
      networkRevision: number;
      modelId: string;
    };
