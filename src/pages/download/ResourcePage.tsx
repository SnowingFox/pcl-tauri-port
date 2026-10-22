import { useEffect, useState } from "react";
import { Navigate, useParams } from "react-router-dom";
import { Download as DlIcon, Search } from "lucide-react";
import { resourcesApi, type LoaderKind, type ResourceItem, type ResourceKind, type ResourcePage as Page } from "../../api";
import { Button, Empty } from "../../components/ui";
import { formatCount } from "../../components/format";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
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
  const { instances, selectedInstanceId } = useGame();
  const [q, setQ] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [offset, setOffset] = useState(0);
  const [data, setData] = useState<Page | null>(null);
  const [target, setTarget] = useState(selectedInstanceId ?? "");
  const inst = instances.find((i) => i.id === target);

  useEffect(() => { if (!target && selectedInstanceId) setTarget(selectedInstanceId); }, [selectedInstanceId, target]);

  useEffect(() => {
    let live = true;
    setData(null);
    resourcesApi.search({
      kind, query: submitted, source: "modrinth", offset, limit: PAGE,
      mcVersion: inst?.mcVersion, loader: inst && inst.loader !== "vanilla" ? (inst.loader as LoaderKind) : undefined,
    }).then((p) => live && setData(p)).catch((e) => { if (live) { setData({ total: 0, items: [] }); toast(String(e.message ?? e), "error"); } });
    return () => { live = false; };
  }, [kind, submitted, offset, inst, toast]);

  const install = async (it: ResourceItem) => {
    if (!target) return toast(t("launch.needInstance"), "error");
    try { await resourcesApi.install(it.source, it.id, target); toast(`${it.title} ${t("download.installedOk")}`, "success"); }
    catch (e) { toast(String((e as Error).message ?? e), "error"); }
  };

  return (
    <div className="col">
      <h2>{t(`download.${kind}`)}</h2>
      <form className="row" role="search" onSubmit={(e) => { e.preventDefault(); setOffset(0); setSubmitted(q); }}>
        <input className="input grow" aria-label={t("common.search")} placeholder={t("download.searchPlaceholder")} value={q} onChange={(e) => setQ(e.target.value)} />
        <select className="input" style={{ width: 200 }} aria-label={t("download.targetInstance")} value={target} onChange={(e) => setTarget(e.target.value)}>
          <option value="">{t("download.targetInstance")}</option>
          {instances.map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
        </select>
        <Button variant="primary" type="submit"><Search size={14} /> {t("common.search")}</Button>
      </form>
      {!data ? <Empty text={t("common.loading")} /> : data.items.length === 0 ? <Empty text={t("download.noResult")} /> : (
        <ul className="col" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {data.items.map((it) => (
            <li key={it.id} className="card row">
              <span className="avatar" aria-hidden>{it.title[0]}</span>
              <div className="grow">
                <strong>{it.title}</strong> <span className="dim">· {it.author}</span>
                <div className="dim">{it.summary}</div>
                <div className="row wrap" style={{ marginTop: 4 }}>
                  {it.tags.map((tg) => <span key={tg} className="chip">{tg}</span>)}
                  <span className="dim" style={{ fontSize: 12 }}>{formatCount(it.downloads)} {t("download.downloads")}</span>
                </div>
              </div>
              <Button onClick={() => install(it)} aria-label={`${t("common.install")} ${it.title}`}><DlIcon size={14} /> {t("common.install")}</Button>
            </li>
          ))}
        </ul>
      )}
      {data && data.total > PAGE && (
        <div className="row">
          <Button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>‹</Button>
          <span className="dim">{offset / PAGE + 1} / {Math.ceil(data.total / PAGE)}</span>
          <Button disabled={offset + PAGE >= data.total} onClick={() => setOffset(offset + PAGE)}>›</Button>
        </div>
      )}
    </div>
  );
}
