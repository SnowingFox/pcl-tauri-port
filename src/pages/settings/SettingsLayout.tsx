import { Navigate, Route, Routes } from "react-router-dom";
import { Coffee, Download, Info, Palette, Rocket } from "lucide-react";
import { SideLink, SplitPage } from "../../components/Shell";
import { t } from "../../i18n";
import { AboutPage, DownloadSettings, JavaSettings, LaunchSettings, PersonalizePage } from "./SettingsPages";

export default function SettingsLayout() {
  return (
    <SplitPage side={
      <>
        <SideLink to="/settings/launch"><Rocket size={16} /> {t("settings.launch")}</SideLink>
        <SideLink to="/settings/java"><Coffee size={16} /> {t("settings.java")}</SideLink>
        <SideLink to="/settings/download"><Download size={16} /> {t("settings.downloadSrc")}</SideLink>
        <SideLink to="/settings/personalize"><Palette size={16} /> {t("settings.personalize")}</SideLink>
        <SideLink to="/settings/about"><Info size={16} /> {t("settings.about")}</SideLink>
      </>
    }>
      <Routes>
        <Route index element={<Navigate to="launch" replace />} />
        <Route path="launch" element={<LaunchSettings />} />
        <Route path="java" element={<JavaSettings />} />
        <Route path="download" element={<DownloadSettings />} />
        <Route path="personalize" element={<PersonalizePage />} />
        <Route path="about" element={<AboutPage />} />
      </Routes>
    </SplitPage>
  );
}
