import { Component, type ErrorInfo, type ReactNode } from "react";

type AppErrorBoundaryProps = {
  children: ReactNode;
};

type AppErrorBoundaryState = {
  error: Error | null;
};

export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("SiaoCut renderer failed", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;

    const english = document.documentElement.lang === "en-US";
    return (
      <main className="startup-error" role="alert">
        <div className="startup-error-mark">S</div>
        <p>{english ? "The interface could not be displayed" : "界面未能正常显示"}</p>
        <h1>{english ? "SiaoCut encountered a renderer error" : "SiaoCut 遇到前端渲染错误"}</h1>
        <p>{english ? "Reload the interface. Your project and source media will not be deleted." : "可以重新加载界面；项目和原始媒体不会被删除。"}</p>
        <details>
          <summary>{english ? "Technical details" : "技术详情"}</summary>
          <code>{this.state.error.message}</code>
        </details>
        <button type="button" onClick={() => window.location.reload()}>{english ? "Reload" : "重新加载"}</button>
      </main>
    );
  }
}
