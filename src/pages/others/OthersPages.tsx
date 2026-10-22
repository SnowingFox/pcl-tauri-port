import { useMemo, useState } from "react";
import { Button, Card, Empty } from "../../components/ui";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
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
  const onExit = useGame((s) => s.onExit);
  return (
    <div className="col">
      <h2>{t("others.crash")}</h2>
      {exit?.crashed ? (
        <Card title={t("others.crashTitle")}>
          <p>{t("others.exitCode")}: {exit.code}</p>
          {exit.reason && <p>{t("others.crashReason")}: {exit.reason}</p>}
          <p className="dim">{t("others.crashAdvice")}</p>
        </Card>
      ) : <Empty text={t("common.empty")} />}
      <div><Button onClick={() => onExit({ launchId: "demo", code: 1, crashed: true, reason: "示例：模组冲突" })}>{t("others.testCrash")}</Button></div>
    </div>
  );
}
