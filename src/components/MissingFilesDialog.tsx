import { appApi, instancesApi } from "../api";
import { Button, Dialog } from "./ui";
import { reportError } from "../lib/notify";
import { isHttpUrl } from "../api/util";
import { useTasks } from "../store/tasks";
import { useGame } from "../store/game";
import { t } from "../i18n";

/**
 * CurseForge files whose authors forbid third-party downloads. The modpack install does not fail; the user
 * downloads these by hand. Links open through app_open_url (http/https only).
 */
export function MissingFilesDialog() {
  const report = useTasks((s) => s.report);
  const dismiss = useTasks((s) => s.dismissReport);
  const instances = useGame((s) => s.instances);
  if (!report) return null;
  const inst = instances.find((i) => i.id === report.instanceId);
  return (
    <Dialog title={t("missing.title", { n: report.missingFiles.length })} onClose={dismiss} actions={
      <>
        {inst && <Button onClick={() => instancesApi.openIn(inst, "mods").catch((e) => reportError(e))}>{t("missing.openFolder")}</Button>}
        <Button variant="primary" onClick={dismiss}>{t("common.ok")}</Button>
      </>
    }>
      <p style={{ margin: 0 }} className="dim">{t("missing.body")}</p>
      <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0, maxHeight: 260, overflow: "auto" }}>
        {report.missingFiles.map((f) => (
          <li key={`${f.projectId}-${f.fileId}`} className="list-item">
            <span className="grow selectable" style={{ wordBreak: "break-all" }}>{f.path.split("/").pop()}</span>
            {isHttpUrl(f.pageUrl) && <Button onClick={() => appApi.openUrl(f.pageUrl).catch((e) => reportError(e))}>{t("missing.open")}</Button>}
          </li>
        ))}
      </ul>
    </Dialog>
  );
}
