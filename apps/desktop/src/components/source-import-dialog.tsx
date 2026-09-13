import { CircleAlert, Download, LoaderCircle, RefreshCw, Search, ShieldCheck, X } from "lucide-react";
import type { RefObject } from "react";
import { formatBytes, formatTime, isHttpsSourceUrl, sourcePlatformLabel, sourceStatusLabel } from "../app-view-model";
import { getUiLocale, tr } from "../i18n";
import type { SourceBrowser, SourceImportJob, SourcePreview } from "../types";
import { JobFailureDetails } from "./job-failure";
import { Dialog } from "./ui";

type SourceImportDialogProps = {
  returnFocusRef: RefObject<HTMLElement | null>;
  sourceUrl: string;
  sourcePreview: SourcePreview | null;
  sourceJob: SourceImportJob | null;
  sourceAuthorized: boolean;
  sourceAuthMode: "anonymous" | "browser";
  sourceBrowser: SourceBrowser;
  sourceBrowserAuthorized: boolean;
  sourceBusy: string | null;
  sourceError: string | null;
  onClose: () => void;
  onSourceUrlChange: (value: string) => void;
  onAuthorizedChange: (value: boolean) => void;
  onAuthModeChange: (value: "anonymous" | "browser") => void;
  onBrowserChange: (value: SourceBrowser) => void;
  onBrowserAuthorizedChange: (value: boolean) => void;
  onInspect: () => void;
  onStart: () => void;
  onCancel: () => void;
  onResume: () => void;
  onReset: () => void;
};

const sourceCopy = {
  "zh-CN": {
    authTitle: "访问方式",
    anonymous: "公开访问",
    anonymousDescription: "不读取浏览器登录信息，适用于匿名可访问的视频。",
    browser: "使用浏览器登录态",
    browserDescription: "临时读取所选浏览器的登录 Cookie，适用于账号可正常播放的视频。",
    browserLabel: "已登录浏览器",
    browserConsent: "允许本次预检和下载临时读取所选浏览器的登录 Cookie。SiaoCut 不导出 Cookie 文件，也不把 Cookie 写入日志、数据库或 Agent 任务。",
    browserDisclosure: "登录态导入仅在预检和下载子进程中临时读取浏览器 Cookie。X 提取失败时，仅将帖子 ID 发送给 FxTwitter 获取公开媒体地址。SiaoCut 不导出 Cookie 文件，也不把 Cookie 交给 Agent。",
    loginRequired: "公开访问没有返回此 X 视频。可改用浏览器登录态重试。",
    browserAuthFailed: "无法读取有效的浏览器登录态。请确认所选浏览器已登录 X。",
    browserMediaUnavailable: "当前登录账号仍无法取得此视频。请确认该账号可以正常播放。",
    resolverFailed: "X 公开解析服务未返回可下载视频。请确认帖子仍可访问，或稍后重试。",
  },
  "en-US": {
    authTitle: "Access method",
    anonymous: "Public access",
    anonymousDescription: "Does not read browser sign-in data. Use for anonymously accessible videos.",
    browser: "Use browser session",
    browserDescription: "Temporarily reads cookies from the selected browser for videos the account can play.",
    browserLabel: "Signed-in browser",
    browserConsent: "Allow this inspection and download to temporarily read sign-in cookies from the selected browser. SiaoCut does not export cookies or write them to logs, its database, or Agent tasks.",
    browserDisclosure: "Browser-session import reads cookies only inside the inspection and download subprocesses. If X extraction fails, only the post ID is sent to FxTwitter to obtain a public media URL. SiaoCut does not export cookies or give them to an Agent.",
    loginRequired: "Public access did not return this X video. Try again with a browser session.",
    browserAuthFailed: "A valid browser session could not be read. Confirm that the selected browser is signed in to X.",
    browserMediaUnavailable: "The current signed-in account still cannot retrieve this video. Confirm that the account can play it.",
    resolverFailed: "The public X resolver did not return a downloadable video. Confirm that the post is still accessible or try again later.",
  },
} as const;

function sourceInspectionSummary(error: string, copy: typeof sourceCopy["zh-CN"] | typeof sourceCopy["en-US"]): string | null {
  if (error.startsWith("source_login_required:")) return copy.loginRequired;
  if (error.startsWith("source_browser_auth_failed:")) return copy.browserAuthFailed;
  if (error.startsWith("source_browser_media_unavailable:")) return copy.browserMediaUnavailable;
  if (error.startsWith("source_x_resolver_failed:")) return copy.resolverFailed;
  return null;
}

export default function SourceImportDialog({
  returnFocusRef,
  sourceUrl,
  sourcePreview,
  sourceJob,
  sourceAuthorized,
  sourceAuthMode,
  sourceBrowser,
  sourceBrowserAuthorized,
  sourceBusy,
  sourceError,
  onClose,
  onSourceUrlChange,
  onAuthorizedChange,
  onAuthModeChange,
  onBrowserChange,
  onBrowserAuthorizedChange,
  onInspect,
  onStart,
  onCancel,
  onResume,
  onReset,
}: SourceImportDialogProps) {
  const copy = sourceCopy[getUiLocale()];
  return <Dialog label={tr("app.s0513")} className="runtime-dialog source-dialog" onClose={onClose} returnFocusRef={returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label={tr("app.s0514")} title={tr("app.s0515")} onClick={onClose}><X size={18}/></button>
    <p className="eyebrow">{tr("app.s0516")}</p><h2>{tr("app.s0517")}</h2><p className="dialog-copy">{tr("app.s0518")}</p>
    {!sourceJob && <form className="source-form" onSubmit={(event) => { event.preventDefault(); onInspect(); }}>
      <fieldset className="source-auth-options"><legend>{copy.authTitle}</legend>
        <label><input type="radio" name="source-auth" value="anonymous" checked={sourceAuthMode === "anonymous"} disabled={Boolean(sourceBusy)} onChange={() => onAuthModeChange("anonymous")}/><span><strong>{copy.anonymous}</strong><small>{copy.anonymousDescription}</small></span></label>
        <label><input type="radio" name="source-auth" value="browser" checked={sourceAuthMode === "browser"} disabled={Boolean(sourceBusy)} onChange={() => onAuthModeChange("browser")}/><span><strong>{copy.browser}</strong><small>{copy.browserDescription}</small></span></label>
      </fieldset>
      {sourceAuthMode === "browser" && <div className="source-browser-auth"><label><span>{copy.browserLabel}</span><select aria-label={copy.browserLabel} value={sourceBrowser} disabled={Boolean(sourceBusy)} onChange={(event) => onBrowserChange(event.target.value as SourceBrowser)}><option value="chrome">Chrome</option><option value="edge">Edge</option><option value="firefox">Firefox</option></select></label><label className="source-consent"><input type="checkbox" checked={sourceBrowserAuthorized} disabled={Boolean(sourceBusy)} onChange={(event) => onBrowserAuthorizedChange(event.target.checked)}/><span>{copy.browserConsent}</span></label></div>}
      <label><span>{tr("app.s0487")}</span><input autoComplete="url" aria-label={tr("app.s0487")} placeholder="https://x.com/…" value={sourceUrl} disabled={Boolean(sourceBusy)} onChange={(event) => onSourceUrlChange(event.target.value)}/></label><button className="button primary" type="submit" disabled={Boolean(sourceBusy) || !isHttpsSourceUrl(sourceUrl) || (sourceAuthMode === "browser" && !sourceBrowserAuthorized)}>{sourceBusy && !sourcePreview ? <LoaderCircle className="spin" size={14}/> : <Search size={14}/>}{tr("app.s0489")}</button>
    </form>}
    {sourcePreview && !sourceJob && <section className="source-preview" aria-label={tr("app.s0519")}><header><span><small>{sourcePlatformLabel(sourcePreview.extractor)} · {sourcePreview.authMode === "browser" ? `${sourcePreview.browser ?? ""} ${getUiLocale() === "zh-CN" ? "登录态" : "session"}` : copy.anonymous}</small><strong>{sourcePreview.title}</strong></span><ShieldCheck size={19}/></header><dl><div><dt>{tr("app.s0491")}</dt><dd>{formatTime(sourcePreview.durationSeconds)}</dd></div><div><dt>{sourcePreview.fileSizeKnown ? tr("app.s0520") : tr("app.s0521")}</dt><dd>{formatBytes(sourcePreview.fileSizeBytes)}</dd></div><div><dt>{tr("app.s0492")}</dt><dd>{sourcePreview.siteMediaId}</dd></div><div><dt>{tr("app.s0522")}</dt><dd>{tr("app.resources.urlEngine")}</dd></div></dl><p className="source-url" title={sourcePreview.webpageUrl}>{sourcePreview.webpageUrl}</p><label className="source-consent"><input type="checkbox" checked={sourceAuthorized} onChange={(event) => onAuthorizedChange(event.target.checked)}/><span>{tr("app.s0523")}</span></label><button className="button primary full" disabled={!sourceAuthorized || Boolean(sourceBusy)} onClick={onStart}>{sourceBusy ? <LoaderCircle className="spin" size={14}/> : <Download size={14}/>}{tr("app.s0524")}</button></section>}
    {sourceJob && <section className="source-job" aria-label={tr("app.s0525")}><header><span className={`source-state ${sourceJob.status}`}><i />{sourceStatusLabel(sourceJob.status)}</span><strong>{sourceJob.title}</strong><small>{tr("app.composite.sourceAttempt", { attempt: sourceJob.attemptCount, mediaId: sourceJob.siteMediaId })}</small></header><div className="source-job-progress"><progress value={sourceJob.progress} max={1}/><span>{Math.round(sourceJob.progress * 100)}% · {formatBytes(sourceJob.bytesDownloaded)} / {formatBytes(sourceJob.totalBytes ?? sourceJob.fileSizeBytes)}</span></div><dl><div><dt>{tr("app.s0528")}</dt><dd>{tr("app.resources.urlEngine")}</dd></div><div><dt>{tr("app.s0239")}</dt><dd>{sourceJob.projectId ?? tr("app.s0529")}</dd></div></dl>{["failed", "interrupted"].includes(sourceJob.status) && <JobFailureDetails className="source-job-error" context="source" status={sourceJob.status} errorCode={sourceJob.errorCode} errorMessage={sourceJob.errorMessage}/>}<div className="source-job-actions">{["queued", "running"].includes(sourceJob.status) && <button disabled={Boolean(sourceBusy) || Boolean(sourceJob.cancelRequestedAt)} onClick={onCancel}>{sourceJob.cancelRequestedAt ? tr("app.s0317") : tr("app.s0530")}</button>}{["cancelled", "failed", "interrupted"].includes(sourceJob.status) && <button className="primary" disabled={Boolean(sourceBusy)} onClick={onResume}><RefreshCw size={13}/>{tr("app.s0279")}</button>}{!["queued", "running", "finalizing"].includes(sourceJob.status) && <button onClick={onReset}>{tr("app.s0531")}</button>}</div></section>}
    {sourceError && <div className="source-error" role="alert"><CircleAlert size={15}/>{sourceJob?.status === "completed" ? <div className="job-failure-details"><span className="job-failure-summary">{tr("app.error.sourceImportOpenFailed")}</span><details className="job-failure-technical"><summary>{tr("app.error.technicalDetails")}</summary><code>{sourceError}</code></details></div> : sourceInspectionSummary(sourceError, copy) ? <div className="job-failure-details"><span className="job-failure-summary">{sourceInspectionSummary(sourceError, copy)}</span><details className="job-failure-technical"><summary>{tr("app.error.technicalDetails")}</summary><code>{sourceError}</code></details></div> : <JobFailureDetails context="source" status="failed" errorMessage={sourceError}/>}</div>}
    <p className="runtime-disclosure">{sourceAuthMode === "browser" ? copy.browserDisclosure : tr("app.resources.urlDisclosure")}</p>
  </Dialog>;
}
