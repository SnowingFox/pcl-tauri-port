import { useCallback, useEffect, useMemo, useState } from "react";
import { downloadApi, type GameVersion, type GameVersionType, type InstallLoaderKind, type LoaderKind, type LoaderVersion } from "../../api";
import { Button, Card, Empty, Field, LoadError, Loading, Segmented } from "../../components/ui";
import { TaskPanel } from "../../components/TaskPanel";
import { useTasks } from "../../store/tasks";
import { useUi } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { validInstanceName } from "../../lib/path";
import { t } from "../../i18n";

const LOADERS: LoaderKind[] = ["vanilla", "fabric", "forge", "neoforge", "quilt"];
type Filter = "all" | GameVersionType;

export default function GamePage() {
  const toast = useUi((s) => s.toast);
  const track = useTasks((s) => s.track);
  const [versions, setVersions] = useState<GameVersion[] | null>(null);
  const [versionsErr, setVersionsErr] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("release");
  const [mc, setMc] = useState<string>("");
  const [loader, setLoader] = useState<LoaderKind>("vanilla");
  const [loaderVersions, setLoaderVersions] = useState<LoaderVersion[] | null>(null);
  const [loaderErr, setLoaderErr] = useState(false);
  const [loaderVersion, setLoaderVersion] = useState("");
  const [name, setName] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [starting, setStarting] = useState(false);

  const loadVersions = useCallback((force = false) => {
    setVersions(null);
    setVersionsErr(null);
    downloadApi.gameVersions(force).then(setVersions).catch((e) => { setVersionsErr(String(e.message ?? e)); reportError(e, t("download.versionsFailed")); });
  }, []);
  useEffect(() => { loadVersions(); }, [loadVersions]);

  useEffect(() => {
    if (!mc || loader === "vanilla") { setLoaderVersions(null); setLoaderVersion(""); setLoaderErr(false); return; }
    let live = true;
    setLoaderVersions(null);
    setLoaderErr(false);
    downloadApi.loaderVersions(loader, mc)
      .then((v) => { if (live) { setLoaderVersions(v); setLoaderVersion((v.find((x) => x.stable !== false) ?? v[0])?.version ?? ""); } })
      .catch((e) => { if (live) { setLoaderVersions([]); setLoaderErr(true); reportError(e, t("download.loaderFailed")); } });
    return () => { live = false; };
  }, [loader, mc]);

  const shown = useMemo(() => (versions ?? []).filter((v) => filter === "all" || v.type === filter), [versions, filter]);

  // suggest a name until the user types their own
  useEffect(() => {
    if (nameTouched || !mc) return;
    setName(loader === "vanilla" ? mc : `${mc}-${loader}`);
  }, [mc, loader, nameTouched]);

  const nameOk = validInstanceName(name);
  const ready = !!mc && nameOk && !starting && (loader === "vanilla" || !!loaderVersion);

  const install = async () => {
    setStarting(true);
    try {
      const id = await downloadApi.installGame({
        mcVersion: mc,
        loader: loader === "vanilla" ? undefined : { kind: loader as InstallLoaderKind, version: loaderVersion },
        instanceName: name,
      });
      track(id, name);
      toast(t("download.installedOk"), "info");
    } catch (e) { reportError(e, t("download.failed")); } finally { setStarting(false); }
  };

  return (
    <div className="col">
      <Card title={t("download.chooseVersion")} actions={
        <Segmented<Filter> label={t("download.chooseVersion")} value={filter} onChange={setFilter}
          options={[{ value: "release", label: t("download.release") }, { value: "snapshot", label: t("download.snapshot") }, { value: "old_beta", label: t("download.old_beta") }, { value: "old_alpha", label: t("download.old_alpha") }, { value: "all", label: t("download.all") }]} />
      }>
        {versionsErr ? <LoadError text={`${t("download.versionsFailed")}：${versionsErr}`} onRetry={() => loadVersions(true)} />
          : !versions ? <Loading />
          : shown.length === 0 ? <Empty text={t("common.empty")} /> : (
          <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0, maxHeight: 300, overflow: "auto" }}>
            {shown.map((v) => (
              <li key={v.id}>
                <button type="button" className="list-item btn ghost" style={{ width: "100%", justifyContent: "flex-start" }} aria-pressed={mc === v.id}
                  onClick={() => setMc(v.id)}>
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
          {loader !== "vanilla" && mc && (
            loaderVersions === null ? <Loading />
            : loaderVersions.length === 0 ? <p className="dim" style={{ margin: 0 }} role={loaderErr ? "alert" : undefined}>{loaderErr ? t("download.loaderFailed") : t("download.noLoaderVersions")}</p>
            : (
              <Field label={t("download.loaderVersion")}>
                {(id) => <select id={id} className="input" value={loaderVersion} onChange={(e) => setLoaderVersion(e.target.value)}>{loaderVersions.map((v) => <option key={v.version} value={v.version}>{v.version}{v.stable === false ? " (测试版)" : ""}</option>)}</select>}
              </Field>
            )
          )}
          <Field label={t("download.instanceName")}>
            {(id) => <input id={id} className="input" value={name} aria-invalid={!!name && !nameOk} onChange={(e) => { setName(e.target.value); setNameTouched(true); }} />}
          </Field>
          {!!name && !nameOk && <span role="alert" style={{ color: "var(--danger)", fontSize: 12 }}>{t("instance.invalidName")}</span>}
          <div className="row">
            <Button variant="primary" disabled={!ready} onClick={install}>{t("download.installBtn")}</Button>
          </div>
        </div>
      </Card>
      <TaskPanel />
    </div>
  );
}
