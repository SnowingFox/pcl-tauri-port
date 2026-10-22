import { useNavigate } from "react-router-dom";
import { Button, Dialog } from "../../components/ui";
import { CrashSummary } from "./CrashSummary";
import { useGame } from "../../store/game";
import { t } from "../../i18n";

/** Global dialog shown when the game exits with a crash. */
export function CrashDialog() {
  const exit = useGame((s) => s.lastExit);
  const instance = useGame((s) => s.launchedInstance);
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
      <CrashSummary exit={exit} instance={instance} onNavigate={() => dismiss()} />
    </Dialog>
  );
}
