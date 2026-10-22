import { useMemo, useState } from "react";
import { crashApi, type CrashAnalysis } from "../../api";
import { Button, Card, Empty } from "../../components/ui";
import { CrashSummary } from "./CrashSummary";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { t, tRaw } from "../../i18n";
import type { LogLevel } from "../../api";

export function HelpPage() {
  const items = tRaw<{ q: string; a: string }[]>("help.items") ?? [];
  return (
    <div className="col">
      <h2>{t("others.help")}</h2>
      {items.map((it) => (
        <details key={it.q} className="card"><summary className="help-q">{it.q}</summary><p>{it.a}</p></details>
      ))}
    </div>
  );
}

export function LogPage() {
  const logs = useGame((s) => s.logs);
  const clear = useGame((s) => s.clearLogs);
  const toast = useUi((s) => s.toast);
  const [level, setLevel] = useState<LogLevel | "all">("all");
  const [kw, setKw] = useState("");
  const shown = useMemo(() => logs.filter((l) => (level === "all" || l.level === level) && l.line.toLowerCase().includes(kw.toLowerCase())), [logs, level, kw]);
  const copy = async () => {
    try { await navigator.clipboard.writeText(shown.map((l) => l.line).join("\n")); toast(t("common.copied"), "success"); } catch { toast(t("common.error"), "error"); }
  };
  return (
    <div className="col">
      <h2>{t("others.logs")}</h2>
      <div className="row">
        <input className="input grow" aria-label={t("others.filter")} placeholder={t("others.filter")} value={kw} onChange={(e) => setKw(e.target.value)} />
        <select className="input" style={{ width: 120 }} aria-label={t("others.level")} value={level} onChange={(e) => setLevel(e.target.value as LogLevel | "all")}>
          {["all", "info", "warn", "error", "debug"].map((l) => <option key={l}>{l}</option>)}
        </select>
        <Button onClick={copy}>{t("others.exportLog")}</Button>
        <Button onClick={clear}>{t("launch.clearLog")}</Button>
      </div>
      <pre className="logpane tall" tabIndex={0} aria-label={t("others.logs")}>
        {shown.length === 0 ? <span className="log-debug">{t("others.logEmpty")}</span> : shown.map((l, i) => <div key={i} className={`log-${l.level}`}>{l.line}</div>)}
      </pre>
    </div>
  );
}

export function CrashPage() {
  const exit = useGame((s) => s.lastExit);
  const instance = useGame((s) => s.launchedInstance);
  const logs = useGame((s) => s.logs);
  const [text, setText] = useState("");
  const [result, setResult] = useState<CrashAnalysis | null>(null);
  const [busy, setBusy] = useState(false);

  const analyze = async (run: () => Promise<CrashAnalysis>) => {
    setBusy(true);
    try { setResult(await run()); } catch (e) { reportError(e); } finally { setBusy(false); }
  };

  return (
    <div className="col">
      <h2>{t("others.crash")}</h2>
      {exit?.crashed ? (
        <Card title={t("others.crashTitle")}><CrashSummary exit={exit} instance={instance} /></Card>
      ) : <Empty text={t("others.crashNone")} />}
      <Card title={t("others.analyzeLatest")}>
        <div className="col">
          <div className="row">
            <Button disabled={busy || !instance} onClick={() => instance && analyze(() => crashApi.analyzeInstance(instance, logs.slice(-300).map((l) => l.line)))}>
              {busy ? t("others.analyzing") : t("others.analyzeLatest")}
            </Button>
          </div>
          <textarea className="input" rows={4} aria-label={t("others.pasteLog")} placeholder={t("others.pasteLog")} value={text} onChange={(e) => setText(e.target.value)} />
          <div><Button disabled={busy || !text.trim()} onClick={() => analyze(() => crashApi.analyzeText(text))}>{t("others.analyze")}</Button></div>
          {result && (result.reasons.length === 0
            ? <p className="dim" style={{ margin: 0 }}>{t("others.noReason")}</p>
            : result.reasons.map((r) => <div key={r.id} className="card selectable" style={{ whiteSpace: "pre-wrap" }}>{r.message}</div>))}
        </div>
      </Card>
    </div>
  );
}
