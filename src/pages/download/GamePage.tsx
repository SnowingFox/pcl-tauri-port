import { useEffect, useMemo, useState } from "react";
import { downloadApi, type GameVersion, type GameVersionType, type LoaderKind } from "../../api";
import { Button, Card, Empty, Field, ProgressBar, Segmented } from "../../components/ui";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
import { t } from "../../i18n";

const LOADERS: LoaderKind[] = ["vanilla", "fabric", "forge", "neoforge", "quilt"];
type Filter = "all" | GameVersionType;

export default function GamePage() {
  const toast = useUi((s) => s.toast);
  const [versions, setVersions] = useState<GameVersion[] | null>(null);
  const [filter, setFilter] = useState<Filter>("release");
  const [mc, setMc] = useState<string>("");
  const [loader, setLoader] = useState<LoaderKind>("vanilla");
  const [loaderVersions, setLoaderVersions] = useState<string[]>([]);
  const [loaderVersion, setLoaderVersion] = useState("");
  const [name, setName] = useState("");
  const [task, setTask] = useState<{ id: string; progress: number; message: string } | null>(null);

  useEffect(() => { downloadApi.gameVersions().then(setVersions).catch((e) => toast(String(e.message ?? e), "error")); }, [toast]);

  useEffect(() => {
    let un: (() => void) | undefined;
    let dead = false;
    downloadApi.onProgress((p) => {
      setTask((cur) => {
        if (!cur || cur.id !== p.taskId) return cur;
        if (p.done) {
          queueMicrotask(() => {
            if (p.error) toast(`${t("download.failed")}: ${p.error}`, "error");
            else { toast(t("download.installed"), "success"); void useGame.getState().loadInstances(); }
          });
          return null;
        }
        return { ...cur, progress: p.progress, message: p.message };
      });
    }).then((u) => (dead ? u() : (un = u)));
    return () => { dead = true; un?.(); };
  }, [toast]);

  useEffect(() => {
    if (!mc || loader === "vanilla") { setLoaderVersions([]); setLoaderVersion(""); return; }
    let live = true;
    downloadApi.loaderVersions(loader, mc).then((v) => { if (live) { setLoaderVersions(v); setLoaderVersion(v[0] ?? ""); } });
    return () => { live = false; };
  }, [loader, mc]);

  const shown = useMemo(() => (versions ?? []).filter((v) => filter === "all" || v.type === filter), [versions, filter]);

  const pick = (v: GameVersion) => { setMc(v.id); setName((n) => (n && !n.startsWith(mc) ? n : loader === "vanilla" ? v.id : `${v.id}-${loader}`)); };

  const install = async () => {
    try {
      const id = await downloadApi.install({ name, mcVersion: mc, loader, loaderVersion: loaderVersion || undefined });
      setTask({ id, progress: 0, message: t("download.installing") });
    } catch (e) { toast(String((e as Error).message ?? e), "error"); }
  };

  return (
    <div className="col">
      <Card title={t("download.chooseVersion")} actions={
        <Segmented<Filter> label={t("download.chooseVersion")} value={filter} onChange={setFilter}
          options={[{ value: "release", label: t("download.release") }, { value: "snapshot", label: t("download.snapshot") }, { value: "old_beta", label: t("download.old_beta") }, { value: "all", label: t("download.all") }]} />
      }>
        {!versions ? <Empty text={t("common.loading")} /> : shown.length === 0 ? <Empty text={t("common.empty")} /> : (
          <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0, maxHeight: 300, overflow: "auto" }}>
            {shown.map((v) => (
              <li key={v.id}>
                <button type="button" className="list-item btn ghost" style={{ width: "100%", justifyContent: "flex-start" }} aria-pressed={mc === v.id}
                  onClick={() => pick(v)}>
                  <strong>{v.id}</strong><span className="chip">{t(`download.${v.type}`)}</span>
                  <span className="dim grow" style={{ textAlign: "right" }}>{v.releaseTime}</span>
                  {mc === v.id && <span className="chip">✓</span>}
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card title={t("download.installBtn")}>
        <div className="col">
          <div className="field">
            <span className="label">{t("download.loader")}</span>
            <Segmented<LoaderKind> label={t("download.loader")} value={loader} onChange={setLoader} options={LOADERS.map((l) => ({ value: l, label: l === "vanilla" ? t("download.vanilla") : l }))} />
          </div>
          {loader !== "vanilla" && (
            <Field label={t("download.loaderVersion")}>
              {(id) => <select id={id} className="input" value={loaderVersion} onChange={(e) => setLoaderVersion(e.target.value)}>{loaderVersions.map((v) => <option key={v}>{v}</option>)}</select>}
            </Field>
          )}
          <Field label={t("download.instanceName")}>{(id) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} />}</Field>
          {task && <><div aria-live="polite">{task.message}</div><ProgressBar value={task.progress} label={t("download.installing")} /></>}
          <div className="row">
            <Button variant="primary" disabled={!mc || !name.trim() || !!task || (loader !== "vanilla" && !loaderVersion)} onClick={install}>{t("download.installBtn")}</Button>
            {task && <Button onClick={() => downloadApi.cancel(task.id)}>{t("common.cancel")}</Button>}
          </div>
        </div>
      </Card>
    </div>
  );
}
