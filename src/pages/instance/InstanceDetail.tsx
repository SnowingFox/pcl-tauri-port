import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Copy, FolderOpen, Pencil, Trash2 } from "lucide-react";
import { instancesApi, type Instance, type InstanceSettings, type ModEntry, type OpenTarget, type SaveEntry } from "../../api";
import { Button, Card, ConfirmDialog, Dialog, Empty, Field, LoadError, Loading, Segmented, Switch } from "../../components/ui";
import { formatDate, formatSize } from "../../components/format";
import { useGame } from "../../store/game";
import { useUi } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { validInstanceName } from "../../lib/path";
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
      {tab === "overview" && <Overview inst={inst} />}
      {tab === "mods" && <Mods inst={inst} />}
      {tab === "saves" && <Saves inst={inst} />}
      {tab === "settings" && <InstSettings key={inst.id} inst={inst} />}
    </div>
  );
}

type NameDialog = "rename" | "duplicate" | null;

function Overview({ inst }: { inst: Instance }) {
  const { deleteInstance, selectInstance, renameInstance, duplicateInstance } = useGame();
  const toast = useUi((s) => s.toast);
  const nav = useNavigate();
  const [confirm, setConfirm] = useState(false);
  const [dlg, setDlg] = useState<NameDialog>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const open = (target: OpenTarget) => instancesApi.openIn(inst, target).catch((e) => reportError(e));
  const submitName = async () => {
    setBusy(true);
    try {
      if (dlg === "rename") {
        const next = await renameInstance(inst.id, name);
        toast(t("instance.renamed"), "success");
        nav(`/instances/${encodeURIComponent(next.id)}`, { replace: true });
      } else {
        await duplicateInstance(inst.id, name);
        toast(t("instance.duplicated"), "success");
      }
      setDlg(null);
    } catch (e) { reportError(e); } finally { setBusy(false); }
  };
  const ok = validInstanceName(name) && name !== inst.name;

  return (
    <Card>
      <dl style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: 8, margin: 0 }}>
        <dt className="dim">{t("instance.description")}</dt><dd style={{ margin: 0 }}>{inst.description || "—"}</dd>
        <dt className="dim">{t("instance.version")}</dt><dd style={{ margin: 0 }}>{inst.mcVersion || "—"}</dd>
        <dt className="dim">{t("instance.loader")}</dt>
        <dd style={{ margin: 0 }}>{inst.loaders.length ? inst.loaders.map((l) => `${l.kind}${l.version ? ` ${l.version}` : ""}`).join("、") : t("download.vanilla")}</dd>
        <dt className="dim">{t("instance.path")}</dt><dd style={{ margin: 0, wordBreak: "break-all" }} className="selectable">{inst.path}</dd>
      </dl>
      {inst.error && <p role="alert" style={{ color: "var(--danger)" }}>{t("instance.broken")}：{inst.error}</p>}
      <div className="row wrap" style={{ marginTop: 16 }}>
        <Button variant="primary" disabled={!!inst.error} onClick={() => { selectInstance(inst.id); nav("/launch"); }}>{t("nav.launch")}</Button>
        <Button onClick={() => open("instance")}><FolderOpen size={14} /> {t("instance.openFolder")}</Button>
        <Button onClick={() => open("mods")}>{t("instance.openMods")}</Button>
        <Button onClick={() => open("saves")}>{t("instance.openSaves")}</Button>
        <Button onClick={() => open("logs")}>{t("instance.openLogs")}</Button>
      </div>
      <div className="row wrap" style={{ marginTop: 10 }}>
        <Button onClick={() => { setName(inst.name); setDlg("rename"); }}><Pencil size={14} /> {t("instance.rename")}</Button>
        <Button onClick={() => { setName(`${inst.name} 副本`); setDlg("duplicate"); }}><Copy size={14} /> {t("instance.duplicate")}</Button>
        <Button variant="danger" onClick={() => setConfirm(true)}><Trash2 size={14} /> {t("instance.deleteInstance")}</Button>
      </div>
      {dlg && (
        <Dialog title={dlg === "rename" ? t("instance.rename") : t("instance.duplicate")} onClose={() => setDlg(null)} actions={
          <>
            <Button onClick={() => setDlg(null)}>{t("common.cancel")}</Button>
            <Button variant="primary" disabled={!ok || busy} onClick={() => void submitName()}>{t("common.ok")}</Button>
          </>
        }>
          <form onSubmit={(e) => { e.preventDefault(); if (ok && !busy) void submitName(); }}>
            <Field label={t("instance.newName")}>{(id) => <input id={id} className="input" value={name} aria-invalid={!ok} onChange={(e) => setName(e.target.value)} />}</Field>
          </form>
          {name !== inst.name && !validInstanceName(name) && <span role="alert" style={{ color: "var(--danger)", fontSize: 12 }}>{t("instance.invalidName")}</span>}
        </Dialog>
      )}
      {confirm && <ConfirmDialog danger title={t("instance.deleteInstance")} message={t("instance.confirmDelete", { name: inst.name })} onClose={() => setConfirm(false)}
        onConfirm={() => { void deleteInstance(inst.id).then(() => { toast(t("instance.deleted"), "success"); nav("/instances"); }).catch((e) => reportError(e)); }} />}
    </Card>
  );
}

function Mods({ inst }: { inst: Instance }) {
  const [mods, setMods] = useState<ModEntry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const load = useCallback(() => {
    setMods(null);
    setErr(null);
    instancesApi.mods(inst).then(setMods).catch((e) => { setErr(String(e.message ?? e)); reportError(e, t("instance.modsFailed")); });
  }, [inst]);
  useEffect(load, [load]);
  const toggle = async (m: ModEntry, enabled: boolean) => {
    setPending(m.path);
    try {
      const path = await instancesApi.setModEnabled(m, enabled);
      setMods((cur) => cur && cur.map((x) => (x.path === m.path ? { ...x, enabled, path, fileName: path.split("/").pop() ?? x.fileName } : x)));
    } catch (e) { reportError(e, t("instance.modToggleFailed")); } finally { setPending(null); }
  };
  if (err) return <LoadError text={`${t("instance.modsFailed")}：${err}`} onRetry={load} />;
  if (!mods) return <Loading />;
  if (!mods.length) return <Empty text={t("instance.noMods")} />;
  return (
    <Card title={`${t("instance.modCount")}: ${mods.length}`}>
      <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {mods.map((m) => (
          <li key={m.path} className="list-item">
            <div className="grow"><strong>{m.name}</strong> <span className="dim">{m.version}</span><div className="dim" style={{ fontSize: 12 }}>{m.fileName} · {formatSize(m.size)}</div></div>
            <Switch checked={m.enabled} label={`${m.enabled ? t("common.disable") : t("common.enable")} ${m.name}`} onChange={(v) => { if (pending !== m.path) void toggle(m, v); }} />
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Saves({ inst }: { inst: Instance }) {
  const [saves, setSaves] = useState<SaveEntry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => {
    setSaves(null);
    setErr(null);
    instancesApi.saves(inst).then(setSaves).catch((e) => { setErr(String(e.message ?? e)); reportError(e, t("instance.savesFailed")); });
  }, [inst]);
  useEffect(load, [load]);
  if (err) return <LoadError text={`${t("instance.savesFailed")}：${err}`} onRetry={load} />;
  if (!saves) return <Loading />;
  if (!saves.length) return <Empty text={t("instance.noSaves")} />;
  return (
    <Card>
      <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {saves.map((s) => (
          <li key={s.path} className="list-item">
            {s.iconUrl ? <img src={s.iconUrl} alt="" width={40} height={40} style={{ borderRadius: 6, imageRendering: "pixelated" }} /> : <span className="avatar" aria-hidden>{s.name[0]}</span>}
            <div className="grow"><strong>{s.name}</strong><div className="dim" style={{ fontSize: 12 }}>{s.folder}{s.lastPlayed ? ` · ${formatDate(s.lastPlayed)}` : ""}</div></div>
            <span className="dim">{formatSize(s.size)}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

/** Per-instance overrides, persisted to `<instance>/PCL/config.json` via instance_settings_get/set. */
function InstSettings({ inst }: { inst: Instance }) {
  const toast = useUi((s) => s.toast);
  const [s, setS] = useState<InstanceSettings | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const load = useCallback(() => {
    setS(null);
    setErr(null);
    instancesApi.getSettings(inst).then((v) => { setS(v); setDirty(false); }).catch((e) => { setErr(String(e.message ?? e)); reportError(e, t("instance.set.loadFailed")); });
  }, [inst]);
  useEffect(load, [load]);
  if (err) return <LoadError text={`${t("instance.set.loadFailed")}：${err}`} onRetry={load} />;
  if (!s) return <Loading />;

  const edit = (fn: (d: InstanceSettings) => void) => { const d = structuredClone(s); fn(d); setS(d); setDirty(true); };
  const follow = s.memory_mb === null && s.java_path === null;
  const save = async () => {
    setSaving(true);
    try { await instancesApi.setSettings(inst, s); setDirty(false); toast(t("instance.set.saved"), "success"); } catch (e) { reportError(e); } finally { setSaving(false); }
  };
  const iso = s.isolated === null ? "auto" : s.isolated ? "on" : "off";

  return (
    <Card actions={<Button variant="primary" disabled={!dirty || saving} onClick={() => void save()}>{t("common.save")}</Button>}>
      <div className="setting"><span>{t("instance.set.followGlobal")}</span>
        <Switch checked={follow} onChange={(v) => edit((d) => { if (v) { d.memory_mb = null; d.java_path = null; } else { d.memory_mb = 4096; d.java_path = ""; } })} label={t("instance.set.followGlobal")} />
      </div>
      <div className="col" style={{ marginTop: 10 }}>
        <Field label={t("instance.set.memory")}>{(i) => <input id={i} className="input" type="number" min={512} step={256} disabled={follow} value={s.memory_mb ?? ""} onChange={(e) => edit((d) => { d.memory_mb = Math.max(512, Number(e.target.value) || 512); })} />}</Field>
        <Field label={t("instance.set.java")}>{(i) => <input id={i} className="input" disabled={follow} placeholder={t("instance.set.javaHint")} value={s.java_path ?? ""} onChange={(e) => edit((d) => { d.java_path = e.target.value; })} />}</Field>
        <Field label={t("instance.set.jvm")}>{(i) => <input id={i} className="input" value={s.jvm_args} onChange={(e) => edit((d) => { d.jvm_args = e.target.value; })} />}</Field>
        <Field label={t("instance.set.gameArgs")}>{(i) => <input id={i} className="input" value={s.game_args} onChange={(e) => edit((d) => { d.game_args = e.target.value; })} />}</Field>
        <Field label={t("instance.set.server")}>{(i) => <input id={i} className="input" value={s.server_address} placeholder="play.example.com" onChange={(e) => edit((d) => { d.server_address = e.target.value; })} />}</Field>
        <Field label={t("instance.set.preLaunch")}>{(i) => <input id={i} className="input" value={s.pre_launch_command} onChange={(e) => edit((d) => { d.pre_launch_command = e.target.value; })} />}</Field>
        <div className="field">
          <span className="label">{t("instance.set.isolated")}</span>
          <Segmented<"auto" | "on" | "off"> label={t("instance.set.isolated")} value={iso} onChange={(v) => edit((d) => { d.isolated = v === "auto" ? null : v === "on"; })}
            options={[{ value: "auto", label: t("instance.set.isolatedAuto") }, { value: "on", label: t("instance.set.isolatedOn") }, { value: "off", label: t("instance.set.isolatedOff") }]} />
        </div>
      </div>
    </Card>
  );
}
