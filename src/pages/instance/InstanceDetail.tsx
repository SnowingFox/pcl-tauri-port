import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { FolderOpen, Trash2 } from "lucide-react";
import { launchApi, type ModEntry, type SaveEntry } from "../../api";
import { Button, Card, ConfirmDialog, Empty, Field, Segmented, Switch } from "../../components/ui";
import { formatDate, formatSize } from "../../components/format";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
import { t } from "../../i18n";

type Tab = "overview" | "mods" | "saves" | "settings";

export default function InstanceDetail() {
  const id = decodeURIComponent(useParams().id ?? "");
  const inst = useGame((s) => s.instances.find((i) => i.id === id));
  const [tab, setTab] = useState<Tab>("overview");
  if (!inst) return <Empty text={t("instance.selectHint")} />;
  return (
    <div className="col">
      <h2>{inst.name}</h2>
      <Segmented<Tab> label={t("instance.title")} value={tab} onChange={setTab}
        options={(["overview", "mods", "saves", "settings"] as Tab[]).map((v) => ({ value: v, label: t(`instance.${v}`) }))} />
      {tab === "overview" && <Overview id={id} />}
      {tab === "mods" && <Mods id={id} />}
      {tab === "saves" && <Saves id={id} />}
      {tab === "settings" && <InstSettings />}
    </div>
  );
}

function Overview({ id }: { id: string }) {
  const inst = useGame((s) => s.instances.find((i) => i.id === id))!;
  const { deleteInstance, selectInstance } = useGame();
  const toast = useUi((s) => s.toast);
  const nav = useNavigate();
  const [confirm, setConfirm] = useState(false);
  return (
    <Card>
      <dl style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: 8, margin: 0 }}>
        <dt className="dim">{t("instance.version")}</dt><dd style={{ margin: 0 }}>{inst.mcVersion}</dd>
        <dt className="dim">{t("instance.loader")}</dt><dd style={{ margin: 0 }}>{inst.loader}{inst.loaderVersion ? ` ${inst.loaderVersion}` : ""}</dd>
        <dt className="dim">{t("launch.lastPlayed")}</dt><dd style={{ margin: 0 }}>{inst.lastPlayed ? formatDate(inst.lastPlayed) : t("launch.never")}</dd>
      </dl>
      <div className="row" style={{ marginTop: 16 }}>
        <Button onClick={() => { selectInstance(id); nav("/launch"); }}>{t("nav.launch")}</Button>
        <Button onClick={() => launchApi.openFolder(id).catch((e) => toast(String(e.message ?? e), "error"))}><FolderOpen size={14} /> {t("instance.openFolder")}</Button>
        <Button variant="danger" onClick={() => setConfirm(true)}><Trash2 size={14} /> {t("instance.deleteInstance")}</Button>
      </div>
      {confirm && <ConfirmDialog danger title={t("instance.deleteInstance")} message={t("instance.confirmDelete")} onClose={() => setConfirm(false)}
        onConfirm={() => { void deleteInstance(id).then(() => nav("/instances")); }} />}
    </Card>
  );
}

function Mods({ id }: { id: string }) {
  const [mods, setMods] = useState<ModEntry[] | null>(null);
  useEffect(() => { setMods(null); launchApi.listMods(id).then(setMods).catch(() => setMods([])); }, [id]);
  const toggle = async (m: ModEntry, enabled: boolean) => {
    await launchApi.toggleMod(id, m.fileName, enabled);
    setMods((cur) => cur && cur.map((x) => (x.fileName === m.fileName ? { ...x, enabled } : x)));
  };
  if (!mods) return <Empty text={t("common.loading")} />;
  if (!mods.length) return <Empty text={t("instance.noMods")} />;
  return (
    <Card title={`${t("instance.modCount")}: ${mods.length}`}>
      <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {mods.map((m) => (
          <li key={m.fileName} className="list-item">
            <div className="grow"><strong>{m.name}</strong> <span className="dim">{m.version}</span><div className="dim" style={{ fontSize: 12 }}>{m.fileName} · {formatSize(m.size)}</div></div>
            <Switch checked={m.enabled} label={`${m.enabled ? t("common.disable") : t("common.enable")} ${m.name}`} onChange={(v) => toggle(m, v)} />
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Saves({ id }: { id: string }) {
  const [saves, setSaves] = useState<SaveEntry[] | null>(null);
  useEffect(() => { setSaves(null); launchApi.listSaves(id).then(setSaves).catch(() => setSaves([])); }, [id]);
  if (!saves) return <Empty text={t("common.loading")} />;
  if (!saves.length) return <Empty text={t("instance.noSaves")} />;
  return (
    <Card>
      <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {saves.map((s) => (
          <li key={s.folder} className="list-item"><div className="grow"><strong>{s.name}</strong><div className="dim" style={{ fontSize: 12 }}>{s.folder} · {formatDate(s.lastPlayed)}</div></div><span className="dim">{formatSize(s.size)}</span></li>
        ))}
      </ul>
    </Card>
  );
}

/** Per-instance overrides are UI-only until the backend exposes per-instance settings (see contract doc). */
function InstSettings() {
  const [follow, setFollow] = useState(true);
  const [mem, setMem] = useState(4096);
  const [jvm, setJvm] = useState("");
  return (
    <Card>
      <div className="setting"><span>{t("instance.set.followGlobal")}</span><Switch checked={follow} onChange={setFollow} label={t("instance.set.followGlobal")} /></div>
      <div className="col" style={{ marginTop: 10 }}>
        <Field label={t("instance.set.memory")}>{(i) => <input id={i} className="input" type="number" min={512} step={256} disabled={follow} value={mem} onChange={(e) => setMem(Number(e.target.value))} />}</Field>
        <Field label={t("instance.set.jvm")}>{(i) => <input id={i} className="input" disabled={follow} value={jvm} onChange={(e) => setJvm(e.target.value)} />}</Field>
      </div>
    </Card>
  );
}
