import { forwardRef, useEffect, useRef } from "react";
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode, RefObject } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "agent" | "danger";
export type StatusTone = "neutral" | "brand" | "agent" | "success" | "info" | "warning" | "danger";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ variant = "secondary", className = "", ...props }, ref) {
  return <button ref={ref} className={`ui-button ${className}`.trim()} data-variant={variant} {...props} />;
});

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  shortcut?: string;
};

export function IconButton({ label, shortcut, className = "", ...props }: IconButtonProps) {
  const tooltip = shortcut ? `${label} · ${shortcut}` : label;
  return <Tooltip label={tooltip}><button className={`ui-icon-button ${className}`.trim()} aria-label={label} {...props} /></Tooltip>;
}

type StatusBadgeProps = HTMLAttributes<HTMLSpanElement> & {
  tone?: StatusTone;
};

export function StatusBadge({ tone = "neutral", className = "", ...props }: StatusBadgeProps) {
  return <span className={`ui-status ${className}`.trim()} data-tone={tone} {...props} />;
}

export function Panel({ className = "", ...props }: HTMLAttributes<HTMLElement>) {
  return <section className={`ui-panel ${className}`.trim()} {...props} />;
}

export function Tooltip({ label, children }: { label: string; children: ReactNode }) {
  return <span className="ui-tooltip" data-tooltip={label}>{children}</span>;
}

type DialogProps = {
  label: string;
  className?: string;
  onClose: () => void;
  returnFocusRef?: RefObject<HTMLElement | null>;
  children: ReactNode;
};

export function Dialog({ label, className = "", onClose, returnFocusRef, children }: DialogProps) {
  const panelRef = useRef<HTMLElement>(null);
  const onCloseRef = useRef(onClose);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const panel = panelRef.current;
    const focusable = () => Array.from(panel?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])') ?? []);
    window.requestAnimationFrame(() => (panel?.querySelector<HTMLElement>("[autofocus]") ?? focusable()[0] ?? panel)?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusable();
      if (!items.length) return;
      const first = items[0];
      const last = items.at(-1)!;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      (returnFocusRef?.current ?? previous)?.focus();
    };
  }, [returnFocusRef]);

  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section ref={panelRef} className={className} role="dialog" aria-modal="true" aria-label={label} tabIndex={-1}>{children}</section>
  </div>;
}
