import { useEffect, useState } from "react";
import { accountsApi, type DeviceCode } from "../../api";
import { Button, Dialog, Field, Segmented } from "../../components/ui";
import { useAccounts } from "../../store/accounts";
import { useUi } from "../../store/ui";
import { settingsApi } from "../../api";
import { t } from "../../i18n";

type Tab = "offline" | "microsoft" | "authlib";

export function LoginDialog({ onClose }: { onClose(): void }) {
  const [tab, setTab] = useState<Tab>("offline");
  const toast = useUi((s) => s.toast);
  const { addOffline, adopt } = useAccounts();
  const [name, setName] = useState("");
  const [nameErr, setNameErr] = useState(false);
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [server, setServer] = useState("");
  const [user, setUser] = useState("");
  const [pwd, setPwd] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let un: (() => void) | undefined;
    let dead = false;
    accountsApi.onMsLogin((e) => {
      if (e.state === "success") { adopt(e.account); toast(`${e.account.name}`, "success"); onClose(); }
      else if (e.state === "error") { setWaiting(false); setCode(null); toast(`${t("account.loginFailed")}: ${e.message}`, "error"); }
    }).then((u) => (dead ? u() : (un = u)));
    return () => { dead = true; un?.(); };
  }, [adopt, onClose, toast]);

  const fail = (e: unknown) => toast(`${t("account.loginFailed")}: ${(e as Error).message ?? e}`, "error");

  const submitOffline = async () => {
    if (!/^\w{3,16}$/.test(name)) return setNameErr(true);
    setBusy(true);
    try { await addOffline(name); onClose(); } catch (e) { fail(e); } finally { setBusy(false); }
  };
  const startMs = async () => {
    setBusy(true);
    try { setCode(await accountsApi.msLoginStart()); setWaiting(true); } catch (e) { fail(e); } finally { setBusy(false); }
  };
  const close = () => { if (waiting) accountsApi.msLoginCancel().catch(() => {}); onClose(); };
  const submitAuthlib = async () => {
    setBusy(true);
    try { adopt(await accountsApi.authlibLogin(server, user, pwd)); onClose(); } catch (e) { fail(e); } finally { setBusy(false); }
  };

  return (
    <Dialog title={t("launch.loginTitle")} onClose={close} actions={<Button onClick={close}>{t("common.close")}</Button>}>
      <Segmented<Tab> label={t("launch.loginTitle")} value={tab} onChange={setTab}
        options={[{ value: "offline", label: t("account.offline") }, { value: "microsoft", label: t("account.microsoft") }, { value: "authlib", label: t("account.authlib") }]} />
      {tab === "offline" && (
        <form className="col" onSubmit={(e) => { e.preventDefault(); void submitOffline(); }}>
          <Field label={t("account.name")}>
            {(id) => <input id={id} className="input" value={name} aria-invalid={nameErr} aria-describedby="name-hint" onChange={(e) => { setName(e.target.value); setNameErr(false); }} />}
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
          {code ? (
            <>
              <div className="field"><span className="label">{t("account.msCode")}</span><strong className="selectable" style={{ fontSize: 24, letterSpacing: 4 }}>{code.userCode}</strong></div>
              <Button onClick={() => settingsApi.openUrl(code.verificationUri)}>{t("account.msOpen")}</Button>
              <span className="dim" role="status">{t("account.msWaiting")}</span>
            </>
          ) : (
            <Button variant="primary" onClick={startMs} disabled={busy}>{t("account.msStart")}</Button>
          )}
        </div>
      )}
      {tab === "authlib" && (
        <form className="col" onSubmit={(e) => { e.preventDefault(); void submitAuthlib(); }}>
          <Field label={t("account.server")}>{(id) => <input id={id} className="input" value={server} onChange={(e) => setServer(e.target.value)} placeholder="https://" />}</Field>
          <Field label={t("account.username")}>{(id) => <input id={id} className="input" value={user} onChange={(e) => setUser(e.target.value)} />}</Field>
          <Field label={t("account.password")}>{(id) => <input id={id} className="input" type="password" value={pwd} onChange={(e) => setPwd(e.target.value)} />}</Field>
          <Button variant="primary" type="submit" disabled={busy || !server || !user}>{t("account.login")}</Button>
        </form>
      )}
    </Dialog>
  );
}
