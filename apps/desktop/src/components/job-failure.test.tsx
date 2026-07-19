import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { changeUiLocale } from "../i18n";
import { JobFailureDetails, jobErrorSummary } from "./job-failure";

afterEach(() => {
  cleanup();
  changeUiLocale("zh-CN");
});

describe("background job error localization", () => {
  it("maps stable error codes before falling back to the job context", () => {
    changeUiLocale("en-US");

    expect(jobErrorSummary("model", "failed", "disk_space_low")).toContain("disk space");
    expect(jobErrorSummary("source", "failed", "future_error_code")).toContain("URL video import");
    expect(jobErrorSummary("audio", "failed", null)).toContain("local audio analysis");
    expect(jobErrorSummary("speaker", "interrupted", null)).toContain("interrupted");
  });

  it("keeps raw Core details collapsed in English mode", () => {
    changeUiLocale("en-US");
    render(<JobFailureDetails context="export" status="failed" errorCode="job_failed" errorMessage="视频导出失败：无法启动进程"/>);

    expect(screen.getByText("The background job did not complete. You can retry it.")).toBeVisible();
    const rawDetails = screen.getByText("视频导出失败：无法启动进程");
    expect(rawDetails.closest("details")).not.toHaveAttribute("open");
    expect(rawDetails).not.toBeVisible();
    expect(screen.getByText("Technical details")).toBeVisible();
  });

  it("uses Chinese summaries in Chinese mode", () => {
    changeUiLocale("zh-CN");
    render(<JobFailureDetails context="agent" status="failed" errorMessage="worker failed"/>);

    expect(screen.getByText("Agent 任务未完成，可以重试。")).toBeVisible();
    expect(screen.getByText("技术详情")).toBeVisible();
  });
});
