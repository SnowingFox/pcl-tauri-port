import { useEffect, useState } from "react";
import { appApi, instancesApi, settingsApi, type DeepPartial, type DownloadSource, type GlobalSettings, type IsolationMode, type JavaInfo, type LauncherVisibility } from "../../api";
import { Button, Card, ConfirmDialog, SettingRow, Segmented, Switch } from "../../components/ui";
import { useSettings } from "../../store/settings";
import { useGame } from "../../store/game";
import { HUES, useUi, type HueName, type ThemeMode } from "../../store/ui";
import { reportError } from "../../lib/notify";
import { t } from "../../i18n";

/** Optimistic patch with a readable error toast on failure. */
function usePatch() {
  const patch = useSettings((s) => s.patch);
  return (p: DeepPartial<GlobalSettings>) => patch(p).catch((e) => reportError(e, t("settings.saveFailed")));
}

const VISIBILITY: { value: LauncherVisibility; key: string }[] = [
  { value: "close_immediately", key: "settings.visCloseNow" },
  { value: "hide_then_close", key: "settings.visHideClose" },
  { value: "hide", key: "settings.visHide" },
  { value: "minimize", key: "settings.visMinimize" },
  { value: "keep", key: "settings.visKeep" },
];
const ISOLATION: { value: IsolationMode; key: string }[] = [
  { value: "off", key: "settings.isoOff" },
  { value: "moddable_only", key: "settings.isoModdable" },
  { value: "non_release_only", key: "settings.isoNonRelease" },
  { value: "moddable_and_non_release", key: "settings.isoModdableNon" },
  { value: "all", key: "settings.isoAll" },
];

/** Number input that commits on blur/Enter so half-typed values never reach the backend. */
function NumberInput({ value, onCommit, min, max, label, width = 90, disabled }: { value: number; onCommit(n: number): void; min: number; max: number; label: string; width?: number; disabled?: boolean }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Math.min(max, Math.max(min, Math.round(Number(draft))));
    if (!Number.isFinite(n)) return setDraft(String(value));
    setDraft(String(n));
    if (n !== value) onCommit(n);
  };
  return <input className="input" style={{ width }} type="number" min={min} max={max} aria-label={label} disabled={disabled} value={draft}
    onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />;
}

function TextInput({ value, onCommit, label, width = 320, disabled, placeholder }: { value: string; onCommit(v: string): void; label: string; width?: number; disabled?: boolean; placeholder?: string }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => { if (draft !== value) onCommit(draft); };
  return <input className="input" style={{ width }} aria-label={label} disabled={disabled} placeholder={placeholder} value={draft}
    onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />;
}

export function LaunchSettings() {
  const v = useSettings((s) => s.value.game);
  const loaded = useSettings((s) => s.loaded);
  const patch = usePatch();
  const loadInstances = useGame((s) => s.loadInstances);
  const [dirs, setDirs] = useState<{ data: string; logs: string } | null>(null);
  useEffect(() => { appApi.dataDirs().then(setDirs).catch(() => {}); }, []);
  const customWindow = v.window_mode === "custom";
  return (
    <>
      <Card title={t("settings.launch")}>
        <SettingRow title={t("settings.onLaunch")}>
          <select className="input" style={{ width: 300 }} aria-label={t("settings.onLaunch")} value={v.launcher_visibility} disabled={!loaded}
            onChange={(e) => void patch({ game: { launcher_visibility: e.target.value as LauncherVisibility } })}>
            {VISIBILITY.map((o) => <option key={o.value} value={o.value}>{t(o.key)}</option>)}
          </select>
        </SettingRow>
        <SettingRow title={t("settings.fullscreen")}>
          <Switch label={t("settings.fullscreen")} checked={v.window_mode === "fullscreen"} onChange={(c) => void patch({ game: { window_mode: c ? "fullscreen" : "default" } })} />
        </SettingRow>
        <SettingRow title={t("settings.winSize")}>
          <span className="row">
            <Segmented<"default" | "custom"> label={t("settings.winSize")} value={customWindow ? "custom" : "default"} onChange={(m) => void patch({ game: { window_mode: m } })}
              options={[{ value: "default", label: "854×480" }, { value: "custom", label: "自定义" }]} />
            <NumberInput label="width" value={v.window_width} min={300} max={7680} disabled={!customWindow} onCommit={(n) => void patch({ game: { window_width: n } })} />
            ×
            <NumberInput label="height" value={v.window_height} min={200} max={4320} disabled={!customWindow} onCommit={(n) => void patch({ game: { window_height: n } })} />
          </span>
        </SettingRow>
        <SettingRow title={t("settings.isolation")}>
          <select className="input" style={{ width: 240 }} aria-label={t("settings.isolation")} value={v.isolation} onChange={(e) => void patch({ game: { isolation: e.target.value as IsolationMode } })}>
            {ISOLATION.map((o) => <option key={o.value} value={o.value}>{t(o.key)}</option>)}
          </select>
        </SettingRow>
        <SettingRow title={t("settings.gameArgs")}>
          <TextInput label={t("settings.gameArgs")} value={v.game_args} onCommit={(x) => void patch({ game: { game_args: x } })} />
        </SettingRow>
        <SettingRow title={t("settings.preLaunch")}>
          <TextInput label={t("settings.preLaunch")} value={v.pre_launch_command} onCommit={(x) => void patch({ game: { pre_launch_command: x } })} />
        </SettingRow>
        <SettingRow title={t("settings.processPriority")}>
          <Segmented<"0" | "1" | "2"> label={t("settings.processPriority")} value={String(v.process_priority) as "0" | "1" | "2"} onChange={(p) => void patch({ game: { process_priority: Number(p) } })}
            options={[{ value: "0", label: t("settings.prioHigh") }, { value: "1", label: t("settings.prioNormal") }, { value: "2", label: t("settings.prioLow") }]} />
        </SettingRow>
      </Card>
      <Card title={t("settings.gameFolders")}>
        <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {v.folders.map((f) => (
            <li key={f.path} className="list-item">
              <input type="radio" name="folder" id={`folder-${f.path}`} checked={v.selected_folder === f.path}
                onChange={() => { instancesApi.selectFolder(f.path).then(() => useSettings.getState().load()).then(() => loadInstances()).catch((e) => reportError(e)); }} />
              <label htmlFor={`folder-${f.path}`} className="grow"><div>{f.name}</div><div className="dim selectable" style={{ fontSize: 12 }}>{f.path}</div></label>
            </li>
          ))}
        </ul>
      </Card>
      {dirs && (
        <Card title={t("settings.dataDir")}>
          <div className="col selectable" style={{ fontSize: 12 }}>
            <div>{t("settings.dataDir")}：{dirs.data}</div>
            <div>{t("settings.logDir")}：{dirs.logs}</div>
          </div>
          <div className="row" style={{ marginTop: 8 }}>
            <Button onClick={() => appApi.reveal(dirs.logs).catch((e) => reportError(e))}>{t("settings.openFolder")}</Button>
          </div>
        </Card>
      )}
    </>
  );
}

export function JavaSettings() {
  const java = useSettings((s) => s.value.java);
  const memory = useSettings((s) => s.value.memory);
  const jvmArgs = useSettings((s) => s.value.game.jvm_args);
  const patch = usePatch();
  const toast = useUi((s) => s.toast);
  const [list, setList] = useState<JavaInfo[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const auto = java.selected === null;

  const scan = async () => {
    setScanning(true);
    try { setList(await settingsApi.discoverJava()); } catch (e) { reportError(e); } finally { setScanning(false); }
  };
  useEffect(() => { void scan(); }, []);

  const setPath = async (path: string) => {
    const p = path.trim();
    if (!p) return void patch({ java: { selected: null } });
    try {
      const info = await settingsApi.inspectJava(p);
      await patch({ java: { selected: info.path } });
      toast(t("settings.javaOk", { v: info.version }), "success");
    } catch (e) { reportError(e, t("settings.inspectFailed")); }
  };

  return (
    <>
      <Card title={t("settings.java")}>
        <SettingRow title={t("settings.autoJava")}>
          <Switch label={t("settings.autoJava")} checked={auto} onChange={(c) => void patch({ java: { selected: c ? null : (list?.[0]?.path ?? "") } })} />
        </SettingRow>
        <SettingRow title={t("settings.javaPath")} desc={t("settings.pickJavaHint")}>
          <TextInput label={t("settings.javaPath")} value={java.selected ?? ""} disabled={auto} onCommit={(v) => void setPath(v)} />
        </SettingRow>
        <SettingRow title={t("settings.autoMemory")}>
          <Switch label={t("settings.autoMemory")} checked={memory.mode === "auto"} onChange={(c) => void patch({ memory: { mode: c ? "auto" : "custom" } })} />
        </SettingRow>
        <SettingRow title={t("settings.memory")}>
          <span className="row">
            <input type="range" aria-label={t("settings.memory")} min={512} max={32768} step={256} disabled={memory.mode === "auto"} value={memory.custom_mb}
              onChange={(e) => void patch({ memory: { custom_mb: Number(e.target.value) } })} />
            <span style={{ width: 56 }}>{memory.custom_mb}</span>
          </span>
        </SettingRow>
        <SettingRow title={t("settings.jvmArgs")}>
          <TextInput label={t("settings.jvmArgs")} value={jvmArgs} onCommit={(v) => void patch({ game: { jvm_args: v } })} />
        </SettingRow>
      </Card>
      <Card title={t("settings.scanJava")} actions={<Button onClick={scan} disabled={scanning}>{scanning ? t("settings.scanning") : t("settings.scanJava")}</Button>}>
        {list && list.length === 0 && <p className="dim" style={{ margin: 0 }}>{t("settings.noJava")}</p>}
        <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {(list ?? []).map((j) => (
            <li key={j.path} className="list-item">
              <div className="grow">
                <strong>Java {j.version}</strong> <span className="dim">{j.vendor} · {j.arch}{j.isJdk ? " · JDK" : " · JRE"}</span>
                <div className="dim selectable" style={{ fontSize: 12 }}>{j.path}</div>
              </div>
              {java.selected === j.path
                ? <span className="chip">{t("settings.current")}</span>
                : <Button onClick={() => void patch({ java: { selected: j.path } })}>{t("settings.useThis")}</Button>}
            </li>
          ))}
        </ul>
      </Card>
    </>
  );
}

const SOURCES: { value: DownloadSource; key: string }[] = [
  { value: "mirror_first", key: "settings.mirrorFirst" },
  { value: "official_first", key: "settings.officialFirst" },
  { value: "official", key: "settings.official" },
];

export function DownloadSettings() {
  const v = useSettings((s) => s.value.download);
  const patch = usePatch();
  const [confirmReset, setConfirmReset] = useState(false);
  const reset = useSettings((s) => s.reset);
  return (
    <>
      <Card title={t("settings.downloadSrc")}>
        {([["source", "settings.source"], ["version_source", "settings.versionSource"], ["mod_source", "settings.modSource"]] as const).map(([k, label]) => (
          <SettingRow key={k} title={t(label)}>
            <Segmented<DownloadSource> label={t(label)} value={v[k]} onChange={(s) => void patch({ download: { [k]: s } })}
              options={SOURCES.map((o) => ({ value: o.value, label: t(o.key) }))} />
          </SettingRow>
        ))}
        <SettingRow title={t("settings.threads")}>
          <NumberInput label={t("settings.threads")} value={v.thread_count} min={1} max={256} onCommit={(n) => void patch({ download: { thread_count: n } })} />
        </SettingRow>
        <SettingRow title={t("settings.verify")}>
          <Switch label={t("settings.verify")} checked={v.validate_ssl} onChange={(c) => void patch({ download: { validate_ssl: c } })} />
        </SettingRow>
      </Card>
      <Card>
        <Button variant="danger" onClick={() => setConfirmReset(true)}>{t("settings.reset")}</Button>
        {confirmReset && <ConfirmDialog danger title={t("settings.reset")} message={t("settings.confirmReset")} onClose={() => setConfirmReset(false)} onConfirm={() => { void reset().catch((e) => reportError(e)); }} />}
      </Card>
    </>
  );
}

export function PersonalizePage() {
  const { mode, hue, reduceMotion, setMode, setHue, setReduceMotion } = useUi();
  return (
    <Card title={t("settings.personalize")}>
      <SettingRow title={t("settings.themeMode")}>
        <Segmented<ThemeMode> label={t("settings.themeMode")} value={mode} onChange={setMode}
          options={[{ value: "system", label: t("settings.system") }, { value: "light", label: t("settings.light") }, { value: "dark", label: t("settings.dark") }]} />
      </SettingRow>
      <SettingRow title={t("settings.hue")}>
        <span className="row" role="group" aria-label={t("settings.hue")}>
          {(Object.keys(HUES) as HueName[]).map((h) => (
            <button key={h} type="button" className="hue-dot" aria-pressed={hue === h} aria-label={t(`theme.${h}`)} title={t(`theme.${h}`)}
              style={{ background: `hsl(${HUES[h]} 80% 52%)` }} onClick={() => setHue(h)} />
          ))}
        </span>
      </SettingRow>
      <SettingRow title={t("settings.reduceMotion")}><Switch label={t("settings.reduceMotion")} checked={reduceMotion} onChange={setReduceMotion} /></SettingRow>
    </Card>
  );
}

export function AboutPage() {
  const open = (url: string) => appApi.openUrl(url).catch((e) => reportError(e));
  const link = (url: string, label: string) => (
    <a href={url} onClick={(e) => { e.preventDefault(); void open(url); }}>{label}</a>
  );
  return (
    <Card title={t("about.title")}>
      <div className="col selectable">
        {/* LICENCE (heavy use, rule 3): the author credit and sponsor link come first on this page */}
        <p style={{ margin: 0 }}>
          {t("about.author")}：<strong>{t("about.authorName")}</strong>（Plain Craft Launcher）　{link(t("about.sponsorUrl"), t("about.sponsor"))}
        </p>
        <p style={{ margin: 0 }}>{t("about.credit")}</p>
        <h2>{t("app.name")}</h2>
        <p style={{ margin: 0 }}>{t("app.tagline")}</p>
        <p style={{ margin: 0 }} role="note">{t("about.notice")}</p>
        <p className="dim" style={{ margin: 0 }}>{t("about.desc")}</p>
        <p className="dim" style={{ margin: 0 }}>{t("about.licence")}</p>
        <p style={{ margin: 0 }}>{link(t("about.buyUrl"), t("about.buy"))}　{link(t("about.repoUrl"), t("about.repo"))}</p>
        <p className="dim" style={{ margin: 0 }}>{t("about.version")}: {__APP_VERSION__}</p>
      </div>
    </Card>
  );
}
