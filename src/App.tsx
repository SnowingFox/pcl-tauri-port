import { useEffect } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { Shell } from "./components/Shell";
import { ThemeEffect } from "./components/ThemeEffect";
import { Toasts } from "./components/ui";
import { CrashDialog } from "./pages/others/CrashDialog";
import LaunchPage from "./pages/launch/LaunchPage";
import DownloadLayout from "./pages/download/DownloadLayout";
import InstanceLayout from "./pages/instance/InstanceLayout";
import SettingsLayout from "./pages/settings/SettingsLayout";
import OthersLayout from "./pages/others/OthersLayout";
import { bindGameEvents, useGame } from "./store/game";
import { useAccounts } from "./store/accounts";
import { useSettings } from "./store/settings";
import { useUi } from "./store/ui";
import { useMockApi } from "./api";
import { t } from "./i18n";

export default function App() {
  const toast = useUi((s) => s.toast);
  useEffect(() => {
    let un: (() => void) | undefined;
    let dead = false;
    const fail = (e: unknown) => toast(`${t("common.error")}: ${String((e as Error)?.message ?? e)}`, "error");
    bindGameEvents().then((u) => (dead ? u() : (un = u))).catch(fail);
    useGame.getState().loadInstances().catch(fail);
    useAccounts.getState().refresh().catch(fail);
    useSettings.getState().load().catch(fail);
    if (useMockApi) toast("当前使用模拟数据（Mock API）", "info", 2500);
    return () => { dead = true; un?.(); };
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
      <Toasts />
    </>
  );
}
