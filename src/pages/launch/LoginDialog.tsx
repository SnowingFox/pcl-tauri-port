import { useCallback, useEffect, useRef, useState } from "react";
import { Copy, ExternalLink } from "lucide-react";
import { accountsApi, appApi, normalizeError, type DeviceCode } from "../../api";
import { Button, Dialog, Field, Segmented } from "../../components/ui";
import { useAccounts } from "../../store/accounts";
import { useUi } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { validPlayerName } from "../../lib/path";
import { describeError } from "../../api/errors";
import { t } from "../../i18n";

type Tab = "offline" | "microsoft" | "authlib";
type MsState =
  | { s: "idle" }
  | { s: "starting" }
  | { s: "code"; code: DeviceCode; deadline: number }
  | { s: "error"; message: string; code: string };

export function LoginDialog({ onClose }: { onClose(): void }) {
  const [tab, setTab] = useState<Tab>("offline");
  const toast = useUi((s) => s.toast);
  const { addOffline, adopt } = useAccounts();
  const [name, setName] = useState("");
  const [nameErr, setNameErr] = useState(false);
  const [ms, setMs] = useState<MsState>({ s: "idle" });
  const [now, setNow] = useState(() => Date.now());
  const [clientId, setClientId] = useState("");
  const [server, setServer] = useState("");
  const [serverName, setServerName] = useState<string | null>(null);
  const [user, setUser] = useState("");
  const [pwd, setPwd] = useState("");
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  const waiting = ms.s === "starting" || ms.s === "code";

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  // device code arrives through an event while accounts_microsoft_login is still pending
  useEffect(() => {
    let un: (() => void) | undefined;
    let dead = false;
    accountsApi.onDeviceCode((code) => {
      if (alive.current) setMs({ s: "code", code, deadline: Date.now() + code.expiresIn * 1000 });
    }).then((u) => (dead ? u() : (un = u))).catch(() => {});
    return () => { dead = true; un?.(); };
  }, []);

  useEffect(() => {
    if (ms.s !== "code") return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [ms.s]);

  const startMs = useCallback(async () => {
    setMs({ s: "starting" });
    try {
      const acc = await accountsApi.microsoftLogin();
      if (!alive.current) { await useAccounts.getState().refresh().catch(() => {}); return; }
      await adopt(acc);
      toast(t("account.added", { name: acc.name }), "success");
      onClose();
    } catch (e) {
      const err = normalizeError(e);
      if (!alive.current || err.code === "CANCELLED") { if (alive.current) setMs({ s: "idle" }); return; }
      setMs({ s: "error", message: describeError(err).text, code: err.code });
    }
  }, [adopt, onClose, toast]);

  const cancelMs = useCallback(async () => {
    await accountsApi.cancelLogin().catch(() => {});
  }, []);

  const close = () => { if (waiting) void cancelMs(); onClose(); };

  const submitOffline = async () => {
    if (!validPlayerName(name)) return setNameErr(true);
    setBusy(true);
    try { await addOffline(name); onClose(); } catch (e) { reportError(e, t("account.loginFailed")); } finally { setBusy(false); }
  };
  const saveClientId = async () => {
    try { await accountsApi.setMsClientId(clientId.trim() || null); setMs({ s: "idle" }); toast(t("settings.saved"), "success"); } catch (e) { reportError(e); }
  };
  const copyCode = async (c: string) => {
    try { await navigator.clipboard.writeText(c); toast(t("common.copied"), "success", 1500); } catch { toast(t("common.error"), "error"); }
  };
  const checkServer = async () => {
    setBusy(true);
    try { setServerName((await accountsApi.serverInfo(server)).name); } catch (e) { setServerName(null); reportError(e, t("account.loginFailed")); } finally { setBusy(false); }
  };
  const submitAuthlib = async () => {
    setBusy(true);
    try {
      const accs = await accountsApi.addYggdrasil(server, user, pwd);
      await adopt(...accs);
      toast(t("account.added", { name: accs.map((a) => a.name).join("、") }), "success");
      onClose();
    } catch (e) { reportError(e, t("account.loginFailed")); } finally { setBusy(false); }
  };

  const remain = ms.s === "code" ? Math.max(0, Math.round((ms.deadline - now) / 1000)) : 0;
  const expired = ms.s === "code" && remain === 0;

  return (
    <Dialog title={t("launch.loginTitle")} onClose={close} actions={<Button onClick={close}>{t("common.close")}</Button>}>
      <Segmented<Tab> label={t("launch.loginTitle")} value={tab} onChange={setTab}
        options={[{ value: "offline", label: t("account.offline") }, { value: "microsoft", label: t("account.microsoft") }, { value: "authlib", label: t("account.authlib") }]} />
      {tab === "offline" && (
        <form className="col" onSubmit={(e) => { e.preventDefault(); void submitOffline(); }}>
          <Field label={t("account.name")}>
            {(id) => <input id={id} className="input" value={name} maxLength={16} aria-invalid={nameErr} aria-describedby="name-hint" onChange={(e) => { setName(e.target.value); setNameErr(false); }} />}
          </Field>
          <span id="name-hint" className={nameErr ? "" : "dim"} style={nameErr ? { color: "var(--danger)" } : undefined} role={nameErr ? "alert" : undefined}>
            {nameErr ? t("account.invalidName") : t("account.nameHint")}
          </span>
          <Button variant="primary" type="submit" disabled={busy}>{t("account.login")}</Button>
        </form>
      )}
      {tab === "microsoft" && (
        <div className="col">
          <p className="dim" style={{ margin: 0 }}>{t("account.msHint")}</p>
          {ms.s === "code" && (
            <>
              <div className="field">
                <span className="label">{t("account.msCode")}</span>
                <strong className="selectable" style={{ fontSize: 28, letterSpacing: 4 }} aria-live="polite">{ms.code.userCode}</strong>
              </div>
              <div className="row wrap">
                <Button variant="primary" disabled={expired} onClick={() => appApi.openUrl(ms.code.verificationUri).catch((e) => reportError(e))}><ExternalLink size={14} /> {t("account.msOpen")}</Button>
                <Button disabled={expired} onClick={() => void copyCode(ms.code.userCode)}><Copy size={14} /> {t("account.msCopy")}</Button>
              </div>
              <span className="dim selectable" style={{ fontSize: 12, wordBreak: "break-all" }}>{ms.code.verificationUri}</span>
              {expired
                ? <span role="alert" style={{ color: "var(--danger)" }}>{t("account.msExpired")}</span>
                : <span className="dim" role="status">{t("account.msWaiting")} {t("account.msExpiresIn", { m: Math.floor(remain / 60), s: String(remain % 60).padStart(2, "0") })}</span>}
              <div className="row">
                {expired
                  ? <Button variant="primary" onClick={() => { void cancelMs().then(() => startMs()); }}>{t("account.msRestart")}</Button>
                  : <Button onClick={() => void cancelMs()}>{t("account.msCancel")}</Button>}
              </div>
            </>
          )}
          {ms.s === "starting" && (
            <div className="row"><span className="dim" role="status">{t("common.loading")}</span><Button onClick={() => void cancelMs()}>{t("account.msCancel")}</Button></div>
          )}
          {ms.s === "error" && (
            <>
              <span role="alert" style={{ color: "var(--danger)" }}>{ms.message}</span>
              {ms.code === "MS_CLIENT_ID_MISSING" && (
                <form className="col" onSubmit={(e) => { e.preventDefault(); void saveClientId(); }}>
                  <p className="dim" style={{ margin: 0 }}>{t("account.msClientIdHint")}</p>
                  <Field label={t("account.msClientIdTitle")}>{(id) => <input id={id} className="input" value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" />}</Field>
                  <Button type="submit" disabled={!clientId.trim()}>{t("account.msClientIdSave")}</Button>
                </form>
              )}
              <Button variant="primary" onClick={() => void startMs()}>{t("account.msRestart")}</Button>
            </>
          )}
          {ms.s === "idle" && <Button variant="primary" onClick={() => void startMs()}>{t("account.msStart")}</Button>}
        </div>
      )}
      {tab === "authlib" && (
        <form className="col" onSubmit={(e) => { e.preventDefault(); void submitAuthlib(); }}>
          <Field label={t("account.server")}>
            {(id) => (
              <span className="row">
                <input id={id} className="input" value={server} onChange={(e) => { setServer(e.target.value); setServerName(null); }} placeholder="https://" />
                <Button onClick={checkServer} disabled={busy || !server.trim()}>{t("account.fetchServer")}</Button>
              </span>
            )}
          </Field>
          {serverName && <span className="dim" role="status">{t("account.serverName")}：{serverName}</span>}
          <Field label={t("account.username")}>{(id) => <input id={id} className="input" value={user} autoComplete="username" onChange={(e) => setUser(e.target.value)} />}</Field>
          <Field label={t("account.password")}>{(id) => <input id={id} className="input" type="password" autoComplete="current-password" value={pwd} onChange={(e) => setPwd(e.target.value)} />}</Field>
          <Button variant="primary" type="submit" disabled={busy || !server.trim() || !user || !pwd}>{t("account.login")}</Button>
        </form>
      )}
    </Dialog>
  );
}
