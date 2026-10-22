import { useEffect, useId, useRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import { CheckCircle2, Info, XCircle, X } from "lucide-react";
import { useUi } from "../store/ui";
import { t } from "../i18n";

export function Button({ variant, big, className = "", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "danger" | "ghost"; big?: boolean }) {
  return <button type="button" {...rest} className={`btn ${variant ?? ""} ${big ? "big" : ""} ${className}`} />;
}

export function Card({ title, children, actions }: { title?: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <section className="card">
      {(title || actions) && (
        <div className="row" style={{ marginBottom: 10 }}>
          {title && <h3 className="grow">{title}</h3>}
          {actions}
        </div>
      )}
      {children}
    </section>
  );
}

export function Switch({ checked, onChange, label }: { checked: boolean; onChange(v: boolean): void; label: string }) {
  return <input type="checkbox" role="switch" className="switch" aria-label={label} checked={checked} onChange={(e) => onChange(e.target.checked)} />;
}

export function SettingRow({ title, desc, children }: { title: string; desc?: string; children: ReactNode }) {
  return (
    <div className="setting">
      <div>
        <div>{title}</div>
        {desc && <div className="desc">{desc}</div>}
      </div>
      {children}
    </div>
  );
}

export function Field({ label, children }: { label: string; children: (id: string) => ReactNode }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {children(id)}
    </div>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string }[]; onChange(v: T): void; label: string }) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={value === o.value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function ProgressBar({ value, label }: { value: number; label: string }) {
  const pct = Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div className="progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
      <i style={{ width: `${pct}%` }} />
    </div>
  );
}

/** Modal dialog: focus trap-lite (focus on open, restore on close), Esc closes. */
export function Dialog({ title, onClose, children, actions, dismissable = true }: { title: string; onClose(): void; children: ReactNode; actions?: ReactNode; dismissable?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("input, button, select, textarea")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && dismissable) closeRef.current();
      if (e.key === "Tab" && ref.current) {
        const f = ref.current.querySelectorAll<HTMLElement>("input, button, select, textarea, a[href]");
        if (!f.length) return;
        const first = f[0], last = f[f.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("keydown", onKey); prev?.focus(); };
  }, [dismissable]);
  return createPortal(
    <div className="overlay" onMouseDown={(e) => dismissable && e.target === e.currentTarget && closeRef.current()}>
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} ref={ref}>
        <h2 id={titleId}>{title}</h2>
        {children}
        {actions && <div className="actions">{actions}</div>}
      </div>
    </div>,
    document.body,
  );
}

export function ConfirmDialog({ title, message, danger, onConfirm, onClose }: { title: string; message: string; danger?: boolean; onConfirm(): void; onClose(): void }) {
  return (
    <Dialog
      title={title}
      onClose={onClose}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant={danger ? "danger" : "primary"} onClick={() => { onConfirm(); onClose(); }}>{t("common.ok")}</Button>
        </>
      }
    >
      <p style={{ margin: 0 }}>{message}</p>
    </Dialog>
  );
}

export function Toasts() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismissToast);
  const nav = useNavigate();
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((x) => (
        <div key={x.id} className={`toast ${x.kind}`}>
          {x.kind === "success" ? <CheckCircle2 size={18} /> : x.kind === "error" ? <XCircle size={18} /> : <Info size={18} />}
          <span className="grow selectable">{x.text}</span>
          {x.action && <button className="btn" style={{ padding: "4px 10px" }} onClick={() => { nav(x.action!.route); dismiss(x.id); }}>{x.action.label}</button>}
          <button className="btn ghost" style={{ padding: 2 }} aria-label={t("common.close")} onClick={() => dismiss(x.id)}><X size={14} /></button>
        </div>
      ))}
    </div>
  );
}

export function Empty({ text, action }: { text: string; action?: ReactNode }) {
  return (
    <div style={{ textAlign: "center", padding: 24 }}>
      <p className="dim" style={{ margin: 0 }}>{text}</p>
      {action && <div style={{ marginTop: 10 }}>{action}</div>}
    </div>
  );
}

/** Failed list load: message plus retry. */
export function LoadError({ text, onRetry }: { text: string; onRetry(): void }) {
  return <Empty text={text} action={<Button onClick={onRetry}>{t("common.retry")}</Button>} />;
}

/** Spinner-less loading placeholder with a polite live region. */
export function Loading() {
  return <p className="dim" role="status" style={{ textAlign: "center", padding: 24, margin: 0 }}>{t("common.loading")}</p>;
}
