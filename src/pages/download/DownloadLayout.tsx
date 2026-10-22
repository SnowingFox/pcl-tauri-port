import { Navigate, Route, Routes } from "react-router-dom";
import { Blocks, Box, Image, Layers, Sun } from "lucide-react";
import { SideLink, SplitPage } from "../../components/Shell";
import { t } from "../../i18n";
import GamePage from "./GamePage";
import ResourcePage from "./ResourcePage";

export default function DownloadLayout() {
  return (
    <SplitPage
      side={
        <>
          <div className="group">{t("download.game")}</div>
          <SideLink to="/download/game"><Box size={16} /> {t("download.game")}</SideLink>
          <div className="group">{t("download.resources")}</div>
          <SideLink to="/download/mod"><Blocks size={16} /> {t("download.mod")}</SideLink>
          <SideLink to="/download/modpack"><Layers size={16} /> {t("download.modpack")}</SideLink>
          <SideLink to="/download/resourcepack"><Image size={16} /> {t("download.resourcepack")}</SideLink>
          <SideLink to="/download/shader"><Sun size={16} /> {t("download.shader")}</SideLink>
        </>
      }
    >
      <Routes>
        <Route index element={<Navigate to="game" replace />} />
        <Route path="game" element={<GamePage />} />
        <Route path=":kind" element={<ResourcePage />} />
      </Routes>
    </SplitPage>
  );
}
