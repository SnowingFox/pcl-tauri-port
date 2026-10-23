import { useEffect } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { Shell } from "./components/Shell";
import { ThemeEffect } from "./components/ThemeEffect";
import { Toasts } from "./components/ui";
import { MissingFilesDialog } from "./components/MissingFilesDialog";
import { CrashDialog } from "./pages/others/CrashDialog";
import LaunchPage from "./pages/launch/LaunchPage";
import DownloadLayout from "./pages/download/DownloadLayout";
import InstanceLayout from "./pages/instance/InstanceLayout";
import SettingsLayout from "./pages/settings/SettingsLayout";
import OthersLayout from "./pages/others/OthersLayout";
import { bindGameEvents, useGame } from "./store/game";
import { bindTaskEvents } from "./store/tasks";
import { useAccounts } from "./store/accounts";
import { useSettings } from "./store/settings";
import { useUi } from "./store/ui";
import { inTauri, useMockApi } from "./api";
import { reportError } from "./lib/notify";
import { t } from "./i18n";

export default function App() {
  const toast = useUi((s) => s.toast);
  useEffect(() => {
    let uns: (() => void)[] = [];
    let dead = false;
    const keep = (u: () => void) => (dead ? u() : uns.push(u));
    bindGameEvents().then(keep).catch((e) => reportError(e));
    bindTaskEvents().then(keep).catch((e) => reportError(e));
    // settings first: the instance list depends on the selected game folder
    useSettings.getState().load()
      .catch((e) => reportError(e, t("settings.loadFailed")))
      .then(() => useGame.getState().loadInstances())
      .catch((e) => reportError(e, t("launch.instancesFailed")));
    useAccounts.getState().refresh().catch((e) => reportError(e));
    if (useMockApi) toast("当前使用模拟数据（Mock API）", "info", 2500);
    if (inTauri) {
      void import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(t("app.name")))
        .catch(() => {});
    }
    return () => { dead = true; uns.forEach((u) => u()); uns = []; };
  }, [toast]);

  return (
    <>
      <ThemeEffect />
      <Routes>
        <Route element={<Shell />}>
          <Route index element={<Navigate to="/launch" replace />} />
          <Route path="launch" element={<LaunchPage />} />
          <Route path="download/*" element={<DownloadLayout />} />
          <Route path="instances/*" element={<InstanceLayout />} />
          <Route path="settings/*" element={<SettingsLayout />} />
          <Route path="others/*" element={<OthersLayout />} />
          <Route path="*" element={<Navigate to="/launch" replace />} />
        </Route>
      </Routes>
      <CrashDialog />
      <MissingFilesDialog />
      <Toasts />
    </>
  );
}
