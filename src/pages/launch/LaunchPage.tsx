import { useEffect, useRef, useState } from "react";
import { Play, Square, UserPlus, Trash2 } from "lucide-react";
import { Link } from "react-router-dom";
import { Button, Card, Empty, ProgressBar } from "../../components/ui";
import { useAccounts } from "../../store/accounts";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
import { t } from "../../i18n";
import { formatDate } from "../../components/format";
import { LoginDialog } from "./LoginDialog";

export default function LaunchPage() {
  const { accounts, selectedId, select, remove } = useAccounts();
  const selected = accounts.find((a) => a.id === selectedId);
  const { instances, selectedInstanceId, selectInstance, phase, progress, logs, start, kill, clearLogs } = useGame();
  const inst = instances.find((i) => i.id === selectedInstanceId);
  const toast = useUi((s) => s.toast);
  const [login, setLogin] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs.length]);

  const onLaunch = async () => {
    if (!selected) return toast(t("launch.needAccount"), "error");
    if (!inst) return toast(t("launch.needInstance"), "error");
    try { await start(selected.id); } catch (e) { toast(String((e as Error).message ?? e), "error"); }
  };

  const busy = phase !== "idle";
  return (
    <div className="content grow" style={{ width: "100%" }}>
      <div className="launch-hero">
        <Card title={t("launch.account")} actions={<Button onClick={() => setLogin(true)}><UserPlus size={14} /> {t("launch.addAccount")}</Button>}>
          {accounts.length === 0 ? (
            <Empty text={t("launch.noAccount")} />
          ) : (
            <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }} role="radiogroup" aria-label={t("launch.account")}>
              {accounts.map((a) => (
                <li key={a.id} className="list-item">
                  <input type="radio" name="account" id={`acc-${a.id}`} checked={a.id === selectedId} onChange={() => select(a.id)} />
                  <span className="avatar" aria-hidden>{a.name[0]?.toUpperCase()}</span>
                  <label htmlFor={`acc-${a.id}`} className="grow">
                    <div>{a.name}</div>
                    <div className="dim" style={{ fontSize: 12 }}>{t(`account.${a.kind}`)}</div>
                  </label>
                  <Button variant="ghost" aria-label={`${t("account.remove")} ${a.name}`} onClick={() => remove(a.id)}><Trash2 size={14} /></Button>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card title={t("launch.instance")}>
          {instances.length === 0 ? (
            <Empty text={t("launch.noInstance")} />
          ) : (
            <div className="col">
              <select className="input" aria-label={t("launch.instance")} value={selectedInstanceId ?? ""} onChange={(e) => selectInstance(e.target.value)} disabled={busy}>
                {instances.map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
              </select>
              {inst && (
                <div className="row wrap">
                  <span className="chip">{inst.mcVersion}</span>
                  <span className="chip">{inst.loader}</span>
                  <span className="dim" style={{ fontSize: 12 }}>
                    {t("launch.lastPlayed")}: {inst.lastPlayed ? formatDate(inst.lastPlayed) : t("launch.never")}
                  </span>
                </div>
              )}
              <Link to="/download/game">{t("nav.download")}</Link>
            </div>
          )}
        </Card>
      </div>

      <Card>
        <div className="row">
          {phase === "idle" ? (
            <Button variant="primary" big onClick={onLaunch}><Play size={18} /> {t("launch.launchBtn")}</Button>
          ) : (
            <Button variant="danger" big onClick={() => kill()}><Square size={18} /> {t("launch.kill")}</Button>
          )}
          <div className="grow col" style={{ gap: 6 }}>
            {busy ? (
              <>
                <div aria-live="polite">{phase === "running" ? t("launch.running") : progress ? `${t(`launch.stage.${progress.stage}`)} · ${progress.message}` : t("launch.launching")}</div>
                <ProgressBar value={progress?.progress ?? 0} label={t("launch.launching")} />
              </>
            ) : (
              <span className="dim">{inst ? `${inst.name}` : t("launch.needInstance")}</span>
            )}
          </div>
        </div>
      </Card>

      <Card title={t("launch.log")} actions={<Button variant="ghost" onClick={clearLogs}>{t("launch.clearLog")}</Button>}>
        <pre className="logpane" ref={logRef} tabIndex={0} aria-label={t("launch.log")}>
          {logs.length === 0 ? <span className="log-debug">{t("others.logEmpty")}</span> : logs.map((l, i) => <div key={i} className={`log-${l.level}`}>{l.line}</div>)}
        </pre>
      </Card>

      {login && <LoginDialog onClose={() => setLogin(false)} />}
    </div>
  );
}
