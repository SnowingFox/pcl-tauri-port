import { useState } from "react";
import { settingsApi, type JavaInstall, type WindowCloseAction } from "../../api";
import { Button, Card, SettingRow, Segmented, Switch } from "../../components/ui";
import { useSettings } from "../../store/settings";
import { HUES, useUi, type HueName, type ThemeMode } from "../../store/ui";
import { t } from "../../i18n";

function useSave() {
  const update = useSettings((s) => s.update);
  const toast = useUi((s) => s.toast);
  return (fn: Parameters<typeof update>[0]) => update(fn).catch((e) => toast(String(e.message ?? e), "error"));
}

export function LaunchSettings() {
  const v = useSettings((s) => s.value.launch);
  const save = useSave();
  return (
    <Card title={t("settings.launch")}>
      <SettingRow title={t("settings.winSize")}>
        <span className="row">
          <input className="input" style={{ width: 90 }} type="number" aria-label="width" value={v.width} onChange={(e) => save((d) => { d.launch.width = Number(e.target.value); })} />
          ×
          <input className="input" style={{ width: 90 }} type="number" aria-label="height" value={v.height} onChange={(e) => save((d) => { d.launch.height = Number(e.target.value); })} />
        </span>
      </SettingRow>
      <SettingRow title={t("settings.fullscreen")}><Switch label={t("settings.fullscreen")} checked={v.fullscreen} onChange={(c) => save((d) => { d.launch.fullscreen = c; })} /></SettingRow>
      <SettingRow title={t("settings.onLaunch")}>
        <Segmented<WindowCloseAction> label={t("settings.onLaunch")} value={v.onLaunch} onChange={(a) => save((d) => { d.launch.onLaunch = a; })}
          options={[{ value: "keep", label: t("settings.keep") }, { value: "hide", label: t("settings.hide") }, { value: "exit", label: t("settings.exit") }]} />
      </SettingRow>
      <SettingRow title={t("settings.gameArgs")}>
        <input className="input" style={{ width: 280 }} aria-label={t("settings.gameArgs")} value={v.gameArgs} onChange={(e) => save((d) => { d.launch.gameArgs = e.target.value; })} />
      </SettingRow>
    </Card>
  );
}

export function JavaSettings() {
  const v = useSettings((s) => s.value.java);
  const save = useSave();
  const toast = useUi((s) => s.toast);
  const [list, setList] = useState<JavaInstall[]>([]);
  const [scanning, setScanning] = useState(false);
  const scan = async () => {
    setScanning(true);
    try { setList(await settingsApi.scanJava()); } catch (e) { toast(String((e as Error).message ?? e), "error"); } finally { setScanning(false); }
  };
  return (
    <>
      <Card title={t("settings.java")}>
        <SettingRow title={t("settings.autoJava")}><Switch label={t("settings.autoJava")} checked={v.autoSelect} onChange={(c) => save((d) => { d.java.autoSelect = c; })} /></SettingRow>
        <SettingRow title={t("settings.javaPath")}>
          <input className="input" style={{ width: 320 }} aria-label={t("settings.javaPath")} disabled={v.autoSelect} value={v.path} onChange={(e) => save((d) => { d.java.path = e.target.value; })} />
        </SettingRow>
        <SettingRow title={t("settings.autoMemory")}><Switch label={t("settings.autoMemory")} checked={v.autoMemory} onChange={(c) => save((d) => { d.java.autoMemory = c; })} /></SettingRow>
        <SettingRow title={t("settings.memory")}>
          <span className="row">
            <input type="range" aria-label={t("settings.memory")} min={512} max={16384} step={256} disabled={v.autoMemory} value={v.memoryMb} onChange={(e) => save((d) => { d.java.memoryMb = Number(e.target.value); })} />
            <span style={{ width: 56 }}>{v.memoryMb}</span>
          </span>
        </SettingRow>
        <SettingRow title={t("settings.jvmArgs")}>
          <input className="input" style={{ width: 320 }} aria-label={t("settings.jvmArgs")} value={v.jvmArgs} onChange={(e) => save((d) => { d.java.jvmArgs = e.target.value; })} />
        </SettingRow>
      </Card>
      <Card title={t("settings.scanJava")} actions={<Button onClick={scan} disabled={scanning}>{scanning ? t("common.loading") : t("settings.scanJava")}</Button>}>
        <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {list.map((j) => (
            <li key={j.path} className="list-item">
              <div className="grow"><strong>Java {j.version}</strong> <span className="dim">{j.vendor}</span><div className="dim selectable" style={{ fontSize: 12 }}>{j.path}</div></div>
              <Button onClick={() => save((d) => { d.java.path = j.path; d.java.autoSelect = false; })}>{t("common.ok")}</Button>
            </li>
          ))}
        </ul>
      </Card>
    </>
  );
}

export function DownloadSettings() {
  const v = useSettings((s) => s.value.download);
  const save = useSave();
  return (
    <Card title={t("settings.downloadSrc")}>
      <SettingRow title={t("settings.source")}>
        <Segmented<"official" | "bmclapi"> label={t("settings.source")} value={v.source} onChange={(s) => save((d) => { d.download.source = s; })}
          options={[{ value: "official", label: t("settings.official") }, { value: "bmclapi", label: t("settings.bmclapi") }]} />
      </SettingRow>
      <SettingRow title={t("settings.threads")}>
        <input className="input" style={{ width: 90 }} type="number" min={1} max={64} aria-label={t("settings.threads")} value={v.threads} onChange={(e) => save((d) => { d.download.threads = Math.max(1, Number(e.target.value)); })} />
      </SettingRow>
      <SettingRow title={t("settings.verify")}><Switch label={t("settings.verify")} checked={v.verifyFiles} onChange={(c) => save((d) => { d.download.verifyFiles = c; })} /></SettingRow>
    </Card>
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
  return (
    <Card title={t("about.title")}>
      <div className="col selectable">
        <h2>{t("app.name")}</h2>
        <p style={{ margin: 0 }}>{t("app.tagline")}</p>
        <p style={{ margin: 0 }}>
          {t("about.author")}：<strong>{t("about.authorName")}</strong>（Plain Craft Launcher）
        </p>
        <p style={{ margin: 0 }}>
          <a href={t("about.sponsorUrl")} onClick={(e) => { e.preventDefault(); void settingsApi.openUrl(t("about.sponsorUrl")); }}>{t("about.sponsor")}</a>
        </p>
        <p className="dim" style={{ margin: 0 }}>{t("about.desc")}</p>
        <p className="dim" style={{ margin: 0 }}>{t("about.licence")}</p>
        <p className="dim" style={{ margin: 0 }}>{t("about.version")}: 0.1.0</p>
      </div>
    </Card>
  );
}
