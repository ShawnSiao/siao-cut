import { readdirSync,readFileSync,statSync } from "node:fs";
import { join,resolve } from "node:path";
import ts from "typescript";
import { describe,expect,it } from "vitest";

const sourceRoot = resolve(process.cwd(), "src");

function sourceFiles(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.(ts|tsx)$/.test(entry) && !/\.test\.(ts|tsx)$/.test(entry) ? [path] : [];
  });
}

describe("desktop architecture boundaries", () => {
  it("keeps resource lifecycle operations on the generated application contract", () => {
    const resources = readFileSync(join(sourceRoot, "domains/local-resource-client.ts"), "utf8");
    expect(resources).not.toMatch(/\brunCore\s*\(/);
    expect(resources).toContain("desktopControl(");
    const controller = readFileSync(join(sourceRoot, "workbench/workbench-controller.tsx"), "utf8");
    expect(controller).toContain("useSourceImportSession(");
    expect(controller).not.toContain("const withSourceBusy =");
    expect(controller).not.toContain("backgroundTaskClient.inspectSource(");
  });
  it("keeps migrated desktop reads on the generated structured query contract", () => {
    const forbidden = /runCore\(\["(?:model",\s*"(?:list|jobs|status)|source",\s*"(?:jobs|status)|video",\s*"(?:list|status)|speaker",\s*"(?:package|jobs|job-status|track)|agent",\s*"(?:health|list|status)|resources",\s*"(?:status|plan|job|jobs))"/;
    for (const file of sourceFiles(join(sourceRoot,"domains"))) {
      expect(readFileSync(file,"utf8"), file).not.toMatch(forbidden);
    }
    expect(readFileSync(join(sourceRoot,"core.ts"),"utf8")).toContain("export type StructuredCoreRequest = DesktopRequest");
  });
  it("checks the actual workbench and one Project owner, not only the App wrapper", () => {
    const controller=readFileSync(join(sourceRoot,"workbench/workbench-controller.tsx"),"utf8");
    for(const session of ["useProjectSession","useEditingSession","usePlaybackSession","useAiReviewSession","useBackgroundSession","useExportSession"]) expect(controller).toContain(`${session}(`);
    expect(controller).not.toMatch(/useState<Project(?:\[\])?\s*\|?\s*null?>|setInterval\s*\(/);
    expect(controller.split(/\r?\n/).length).toBeLessThan(1500);
    expect(controller).toContain("useEditingSession(project, acknowledgeEdit)");
    for (const module of ["useWorkbenchStartup", "useRuntimeSession", "useResourceCompletion", "useSpeakerSession", "useTranscriptionReviewSession", "useTranscriptionStartSession", "useSubtitleImportSession", "createTranscriptCommands", "createPresentationCommands"]) expect(controller).toContain(`${module}(`);
    expect(controller).not.toMatch(/editing\.session\.mutate|const initialize = useCallback|const transcribe =|const installModel =|const createWordCut =/);

    const owners=sourceFiles(sourceRoot).filter((path)=>/useState<Project\s*\|\s*null>/.test(readFileSync(path,"utf8")));
    expect(owners.map((path)=>path.replaceAll("\\","/").split("/src/")[1])).toEqual(["features/project-session/use-project-session.ts"]);
  });

  it("routes components through domain ports and rejects static runtime dependency cycles",()=>{
    const files=sourceFiles(sourceRoot),known=new Set(files),graph=new Map<string,string[]>();
    for(const file of files){
      const source=ts.createSourceFile(file,readFileSync(file,"utf8"),ts.ScriptTarget.Latest,true);
      const edges:string[]=[];
      for(const statement of source.statements){
        if(!ts.isImportDeclaration(statement)||!ts.isStringLiteral(statement.moduleSpecifier)||statement.importClause?.isTypeOnly)continue;
        const specifier=statement.moduleSpecifier.text;
        if(file.endsWith(".tsx")) expect(specifier,`${file} imports transport`).not.toMatch(/(?:^|\/)core$|@tauri-apps\/api\/core/);
        if(!specifier.startsWith("."))continue;
        const base=resolve(file,"..",specifier),dependency=[`${base}.ts`,`${base}.tsx`,join(base,"index.ts")].find((path)=>known.has(path));
        if(dependency)edges.push(dependency);
      }
      graph.set(file,edges);
    }
    const visited=new Set<string>();
    const visit=(file:string,path:string[])=>{expect(path.includes(file),`Dependency cycle: ${[...path,file].join(" -> ")}`).toBe(false);if(visited.has(file))return;for(const dependency of graph.get(file)??[])visit(dependency,[...path,file]);visited.add(file);};
    for(const file of files)visit(file,[]);
  });
  it("keeps App.tsx as a small top-level assembly module", () => {
    const app = readFileSync(join(sourceRoot, "App.tsx"), "utf8");
    expect(app.split(/\r?\n/).length).toBeLessThanOrEqual(500);
    expect(app).toContain("./workbench/workbench-controller");
  });

  it("allows raw runCore calls only in the low-level adapter and typed domain clients", () => {
    const allowed = new Set([join(sourceRoot,"core.ts").replaceAll("\\","/")]);
    const violations = sourceFiles(sourceRoot)
      .map((path) => path.replaceAll("\\", "/"))
      .filter((path) => !allowed.has(path))
      .filter((path) => /\brunCore\s*\(/.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(sourceRoot.replaceAll("\\", "/").length + 1));

    expect(violations).toEqual([]);
  });

  it("keeps technical resource identifiers out of normal resource and URL-import surfaces", () => {
    const normalSurfaces = [
      join(sourceRoot, "components/local-resource-ui.tsx"),
      join(sourceRoot, "components/source-import-dialog.tsx"),
    ];
    const forbidden = /FFmpeg|FFprobe|yt-dlp|whisper\.cpp|SIAOCUT_[A-Z_]+|SHA-?256/;
    const violations = normalSurfaces
      .filter((path) => forbidden.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(sourceRoot.length + 1));

    expect(violations).toEqual([]);
  });
});
