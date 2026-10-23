import { useNavigate } from "react-router-dom";
import { FolderOpen } from "lucide-react";
import { appApi, instancesApi, type CrashAnalysis, type Instance, type LaunchExit } from "../../api";
import { Button } from "../../components/ui";
import { planCrashAction } from "../../lib/crashActions";
import { reportError } from "../../lib/notify";
import { formatDuration } from "../../components/format";
import { t } from "../../i18n";

/** Exit code, crash reasons (from the crash module) and the suggested actions. All backend text is rendered as plain text. */
export function CrashSummary({ exit, instance, onNavigate }: { exit: LaunchExit; instance: Instance | null; onNavigate?(): void }) {
  const nav = useNavigate();
  const crash: CrashAnalysis | null = exit.crash;
  const run = (id: string) => {
    const plan = planCrashAction(id);
    if (plan.kind === "route") { onNavigate?.(); nav(plan.route); }
    else if (plan.kind === "open" && instance) instancesApi.openIn(instance, plan.target).catch((e) => reportError(e));
  };
  return (
    <div className="col selectable">
      <p style={{ margin: 0 }}>
        {t("others.exitCode")}: {exit.code ?? "—"}
        {exit.durationMs !== undefined && <span className="dim"> · {t("others.crashDuration")} {formatDuration(exit.durationMs)}</span>}
      </p>
      {crash && crash.reasons.length > 0 ? (
        crash.reasons.map((r) => (
          <div key={r.id} className="card col" style={{ gap: 6 }}>
            <strong>{t("others.crashReason")}</strong>
            <div style={{ whiteSpace: "pre-wrap" }}>{r.message}</div>
            {r.details.length > 0 && (
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 12 }} aria-label={t("others.crashDetails")}>
                {r.details.slice(0, 12).map((d, i) => <li key={i}>{d}</li>)}
              </ul>
            )}
            {r.actions.length > 0 && (
              <div className="row wrap" role="group" aria-label={t("others.crashActions")}>
                {r.actions.map((a) => <Button key={a.id} onClick={() => run(a.id)}>{a.label}</Button>)}
              </div>
            )}
          </div>
        ))
      ) : (
        <p className="dim" style={{ margin: 0 }}>{t("others.crashNoReason")}</p>
      )}
      {crash?.primaryFile && (
        <div className="row">
          <Button onClick={() => appApi.reveal(crash.primaryFile!).catch((e) => reportError(e))}><FolderOpen size={14} /> {t("others.crashViewOutput")}</Button>
          <span className="dim" style={{ fontSize: 12 }}>{t("others.crashShareHint")}</span>
        </div>
      )}
    </div>
  );
}
