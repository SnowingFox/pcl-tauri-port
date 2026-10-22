import { useNavigate } from "react-router-dom";
import { Button, Dialog } from "../../components/ui";
import { useGame } from "../../store/game";
import { t } from "../../i18n";

/** Global dialog shown when the game exits with a crash. */
export function CrashDialog() {
  const exit = useGame((s) => s.lastExit);
  const dismiss = useGame((s) => s.dismissExit);
  const nav = useNavigate();
  if (!exit?.crashed) return null;
  return (
    <Dialog title={t("others.crashTitle")} onClose={dismiss} actions={
      <>
        <Button onClick={() => { dismiss(); nav("/others/logs"); }}>{t("others.logs")}</Button>
        <Button variant="primary" onClick={dismiss}>{t("common.ok")}</Button>
      </>
    }>
      <p style={{ margin: 0 }}>{t("others.exitCode")}: {exit.code}</p>
      {exit.reason && <p style={{ margin: 0 }}>{t("others.crashReason")}: {exit.reason}</p>}
      <p className="dim" style={{ margin: 0 }}>{t("others.crashAdvice")}</p>
    </Dialog>
  );
}
