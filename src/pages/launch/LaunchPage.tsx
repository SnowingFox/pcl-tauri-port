import { useEffect, useRef, useState } from "react";
import { Play, Square, UserPlus, Trash2 } from "lucide-react";
import { Link } from "react-router-dom";
import { Button, Card, ConfirmDialog, Empty, LoadError, Loading, ProgressBar } from "../../components/ui";
import { useAccounts } from "../../store/accounts";
import { useGame } from "../../store/game";
import { usePrompt } from "../../store/prompt";
import { useUi } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { t } from "../../i18n";
import { LoginDialog } from "./LoginDialog";
import { LaunchPromptDialog } from "./LaunchPromptDialog";

export default function LaunchPage() {
  const { accounts, selectedId, loaded: accountsLoaded, select, remove } = useAccounts();
  const selected = accounts.find((a) => a.id === selectedId);
  const { instances, instancesLoaded, instancesError, loadInstances, selectedInstanceId, selectInstance, phase, progress, logs, kill, clearLogs } = useGame();
  const inst = instances.find((i) => i.id === selectedInstanceId);
  const requestLaunch = usePrompt((s) => s.requestLaunch);
  const toast = useUi((s) => s.toast);
  const [login, setLogin] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const logRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs.length]);

  const onLaunch = () => {
    if (!selected) return toast(t("launch.needAccount"), "error");
    if (!inst) return toast(t("launch.needInstance"), "error");
    if (inst.error) return toast(t("launch.brokenInstance"), "error");
    void requestLaunch(selected);
  };

  const busy = phase !== "idle";
  const removeTarget = accounts.find((a) => a.id === removing);
  return (
    <div className="content grow" style={{ width: "100%" }}>
      <div className="launch-hero">
        <Card title={t("launch.account")} actions={<Button onClick={() => setLogin(true)}><UserPlus size={14} /> {t("launch.addAccount")}</Button>}>
          {!accountsLoaded ? (
            <Loading />
          ) : accounts.length === 0 ? (
            <Empty text={t("launch.noAccount")} action={<Button variant="primary" onClick={() => setLogin(true)}>{t("launch.addAccount")}</Button>} />
          ) : (
            <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }} role="radiogroup" aria-label={t("launch.account")}>
              {accounts.map((a) => (
                <li key={a.id} className="list-item">
                  <input type="radio" name="account" id={`acc-${a.id}`} checked={a.id === selectedId} disabled={busy} onChange={() => select(a.id).catch((e) => reportError(e))} />
                  <span className="avatar" aria-hidden>{a.name[0]?.toUpperCase()}</span>
                  <label htmlFor={`acc-${a.id}`} className="grow">
                    <div>{a.name}</div>
                    <div className="dim" style={{ fontSize: 12 }}>{t(`account.${a.kind}`)}{a.serverName ? ` · ${a.serverName}` : ""}</div>
                  </label>
                  <Button variant="ghost" disabled={busy} aria-label={`${t("account.remove")} ${a.name}`} onClick={() => setRemoving(a.id)}><Trash2 size={14} /></Button>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card title={t("launch.instance")}>
          {!instancesLoaded ? (
            <Loading />
          ) : instancesError ? (
            <LoadError text={`${t("launch.instancesFailed")}：${instancesError}`} onRetry={() => void loadInstances().catch((e) => reportError(e))} />
          ) : instances.length === 0 ? (
            <Empty text={t("launch.noInstance")} action={<Link to="/download/game">{t("launch.goDownload")}</Link>} />
          ) : (
            <div className="col">
              <select className="input" aria-label={t("launch.instance")} value={selectedInstanceId ?? ""} onChange={(e) => selectInstance(e.target.value)} disabled={busy}>
                {instances.map((i) => <option key={i.id} value={i.id}>{i.name}{i.error ? " ⚠" : ""}</option>)}
              </select>
              {inst && (
                <div className="col" style={{ gap: 4 }}>
                  <div className="row wrap">
                    {inst.mcVersion && <span className="chip">{inst.mcVersion}</span>}
                    {inst.loader !== "vanilla" && <span className="chip">{inst.loader}{inst.loaderVersion ? ` ${inst.loaderVersion}` : ""}</span>}
                  </div>
                  {inst.error
                    ? <span role="alert" style={{ color: "var(--danger)", fontSize: 12 }}>{t("launch.brokenInstance")}：{inst.error}</span>
                    : <span className="dim" style={{ fontSize: 12 }}>{inst.description}</span>}
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
            <Button variant="primary" big onClick={onLaunch} disabled={!inst || !!inst.error}><Play size={18} /> {t("launch.launchBtn")}</Button>
          ) : (
            <Button variant="danger" big onClick={() => kill().catch((e) => reportError(e))}><Square size={18} /> {t("launch.kill")}</Button>
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
      {removeTarget && (
        <ConfirmDialog danger title={t("account.remove")} message={t("account.confirmRemove", { name: removeTarget.name })} onClose={() => setRemoving(null)}
          onConfirm={() => { void remove(removeTarget.id).catch((e) => reportError(e)); }} />
      )}
      <LaunchPromptDialog />
    </div>
  );
}
