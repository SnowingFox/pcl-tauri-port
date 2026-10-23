import { useEffect, useState } from "react";
import { Navigate, useParams } from "react-router-dom";
import { Download as DlIcon, ExternalLink, Search } from "lucide-react";
import { appApi, resourcesApi, type ResourceItem, type ResourceKind, type ResourcePage as Page, type ResourceSourceFilter } from "../../api";
import { Button, Dialog, Empty, Field, LoadError, Loading, Segmented } from "../../components/ui";
import { TaskPanel } from "../../components/TaskPanel";
import { formatCount } from "../../components/format";
import { useGame } from "../../store/game";
import { useTasks } from "../../store/tasks";
import { useUi } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { validInstanceName } from "../../lib/path";
import { t } from "../../i18n";

const KINDS: ResourceKind[] = ["mod", "modpack", "resourcepack", "shader"];
const PAGE = 20;

export default function ResourcePage() {
  const { kind } = useParams();
  if (!KINDS.includes(kind as ResourceKind)) return <Navigate to="/download/game" replace />;
  return <Inner key={kind} kind={kind as ResourceKind} />;
}

function Inner({ kind }: { kind: ResourceKind }) {
  const toast = useUi((s) => s.toast);
  const track = useTasks((s) => s.track);
  const { instances, selectedInstanceId } = useGame();
  const [q, setQ] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [source, setSource] = useState<ResourceSourceFilter>("all");
  const [offset, setOffset] = useState(0);
  const [data, setData] = useState<Page | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [target, setTarget] = useState(selectedInstanceId ?? "");
  const [pack, setPack] = useState<ResourceItem | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const inst = instances.find((i) => i.id === target);

  useEffect(() => { if (!target && selectedInstanceId) setTarget(selectedInstanceId); }, [selectedInstanceId, target]);

  useEffect(() => {
    let live = true;
    setData(null);
    setErr(null);
    resourcesApi.search({
      kind, query: submitted, source, offset, limit: PAGE,
      // modpacks bring their own game version and loader, so they are never filtered by the selected instance
      mcVersion: kind === "modpack" ? undefined : inst?.mcVersion || undefined,
      loader: kind === "modpack" || kind === "resourcepack" || kind === "shader" ? undefined : inst && inst.loader !== "vanilla" ? inst.loader : undefined,
    }).then((p) => live && setData(p)).catch((e) => { if (live) { setErr(String(e.message ?? e)); reportError(e); } });
    return () => { live = false; };
  }, [kind, submitted, source, offset, inst?.mcVersion, inst?.loader, reload]);

  const installInto = async (it: ResourceItem) => {
    if (kind === "modpack") return setPack(it);
    if (!inst) return toast(t("download.needInstanceForRes"), "error");
    setBusyId(it.id);
    try {
      const taskId = await resourcesApi.installResource({
        platform: it.source, projectId: it.id, kind: kind as Exclude<ResourceKind, "modpack">, instanceId: inst.id,
      });
      track(taskId, it.title);
      toast(`${it.title} ${t("download.installedOk")}`, "info");
    } catch (e) { reportError(e, it.title); } finally { setBusyId(null); }
  };

  return (
    <div className="col">
      <h2>{t(`download.${kind}`)}</h2>
      <form className="row wrap" role="search" onSubmit={(e) => { e.preventDefault(); setOffset(0); setSubmitted(q); }}>
        <input className="input grow" aria-label={t("common.search")} placeholder={t("download.searchPlaceholder")} value={q} onChange={(e) => setQ(e.target.value)} />
        <Segmented<ResourceSourceFilter> label={t("download.source")} value={source} onChange={(s) => { setOffset(0); setSource(s); }}
          options={[{ value: "all", label: t("download.srcAll") }, { value: "modrinth", label: t("download.srcModrinth") }, { value: "curseforge", label: t("download.srcCurseforge") }]} />
        {kind !== "modpack" && (
          <select className="input" style={{ width: 200 }} aria-label={t("download.targetInstance")} value={target} onChange={(e) => { setOffset(0); setTarget(e.target.value); }}>
            <option value="">{t("download.targetInstance")}</option>
            {instances.filter((i) => !i.error).map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
          </select>
        )}
        <Button variant="primary" type="submit"><Search size={14} /> {t("common.search")}</Button>
      </form>
      {kind === "mod" && inst && inst.loader === "vanilla" && <p className="dim" style={{ margin: 0 }} role="status">{t("download.noModLoader")}</p>}
      {data && data.warnings.length > 0 && (
        <p className="dim" style={{ margin: 0, fontSize: 12 }} role="status">{t("download.sourceWarn")}：{data.warnings.join("；")}</p>
      )}
      {err ? <LoadError text={err} onRetry={() => setReload((n) => n + 1)} />
        : !data ? <Loading />
        : data.items.length === 0 ? <Empty text={t("download.noResult")} /> : (
        <ul className="col" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {data.items.map((it) => (
            <li key={`${it.source}-${it.id}`} className="card row">
              {it.iconUrl
                ? <img src={it.iconUrl} alt="" width={40} height={40} loading="lazy" referrerPolicy="no-referrer" style={{ borderRadius: 8, flex: "none", objectFit: "cover" }} />
                : <span className="avatar" aria-hidden>{it.title[0]}</span>}
              <div className="grow">
                {/* all remote text is rendered as plain text, never as HTML */}
                <strong>{it.title}</strong> <span className="dim">· {it.author ?? "—"}</span> <span className="chip">{it.source === "curseforge" ? "CurseForge" : "Modrinth"}</span>
                <div className="dim" style={{ overflow: "hidden", textOverflow: "ellipsis", display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical" }}>{it.summary}</div>
                <div className="row wrap" style={{ marginTop: 4 }}>
                  {it.tags.slice(0, 6).map((tg) => <span key={tg} className="chip">{tg}</span>)}
                  <span className="dim" style={{ fontSize: 12 }}>{formatCount(it.downloads)} {t("download.downloads")}</span>
                </div>
              </div>
              {it.pageUrl && <Button variant="ghost" aria-label={`${t("download.openPage")} ${it.title}`} onClick={() => appApi.openUrl(it.pageUrl!).catch((e) => reportError(e))}><ExternalLink size={14} /></Button>}
              <Button onClick={() => installInto(it)} disabled={busyId === it.id} aria-label={`${t("common.install")} ${it.title}`}><DlIcon size={14} /> {t("common.install")}</Button>
            </li>
          ))}
        </ul>
      )}
      {data && data.total > PAGE && source !== "all" && (
        <div className="row">
          <Button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>‹</Button>
          <span className="dim">{offset / PAGE + 1} / {Math.ceil(data.total / PAGE)}</span>
          <Button disabled={offset + PAGE >= data.total} onClick={() => setOffset(offset + PAGE)}>›</Button>
        </div>
      )}
      {kind === "modpack" && <LocalPack />}
      {pack && <ModpackDialog item={pack} onClose={() => setPack(null)} />}
      <TaskPanel />
    </div>
  );
}

function useStartPack(onDone: () => void) {
  const track = useTasks((s) => s.track);
  const toast = useUi((s) => s.toast);
  return async (source: Parameters<typeof resourcesApi.installModpack>[0], name: string) => {
    try {
      const id = await resourcesApi.installModpack(source, name);
      track(id, name);
      toast(`${name} ${t("download.installedOk")}`, "info");
      onDone();
    } catch (e) { reportError(e, name); }
  };
}

function ModpackDialog({ item, onClose }: { item: ResourceItem; onClose(): void }) {
  const [name, setName] = useState(item.title.replace(/[\\/:*?"<>|]/g, " ").trim().slice(0, 60));
  const start = useStartPack(onClose);
  const ok = validInstanceName(name);
  return (
    <Dialog title={`${t("download.installModpackNew")}：${item.title}`} onClose={onClose} actions={
      <>
        <Button onClick={onClose}>{t("common.cancel")}</Button>
        <Button variant="primary" disabled={!ok} onClick={() => void start({ type: "project", platform: item.source, projectId: item.id }, name)}>{t("download.installBtn")}</Button>
      </>
    }>
      <Field label={t("download.modpackName")}>{(id) => <input id={id} className="input" value={name} aria-invalid={!ok} onChange={(e) => setName(e.target.value)} />}</Field>
      {!ok && <span role="alert" style={{ color: "var(--danger)", fontSize: 12 }}>{t("instance.invalidName")}</span>}
    </Dialog>
  );
}

/** Install a downloaded .mrpack / CurseForge zip / MultiMC pack from disk. */
function LocalPack() {
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const start = useStartPack(() => { setPath(""); setName(""); });
  const ok = path.trim().startsWith("/") && validInstanceName(name);
  return (
    <details className="card">
      <summary className="help-q">{t("download.modpackFile")}</summary>
      <form className="col" style={{ marginTop: 10 }} onSubmit={(e) => { e.preventDefault(); if (ok) void start({ type: "file", path: path.trim() }, name); }}>
        <Field label={t("download.filePath")}>{(id) => <input id={id} className="input" value={path} onChange={(e) => setPath(e.target.value)} placeholder="/Users/me/Downloads/pack.mrpack" />}</Field>
        <Field label={t("download.modpackName")}>{(id) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} />}</Field>
        <div><Button variant="primary" type="submit" disabled={!ok}>{t("download.installBtn")}</Button></div>
      </form>
    </details>
  );
}
