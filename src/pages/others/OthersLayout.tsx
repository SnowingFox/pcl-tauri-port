import { Navigate, Route, Routes } from "react-router-dom";
import { Bug, CircleHelp, ScrollText } from "lucide-react";
import { SideLink, SplitPage } from "../../components/Shell";
import { t } from "../../i18n";
import { HelpPage, LogPage, CrashPage } from "./OthersPages";

export default function OthersLayout() {
  return (
    <SplitPage side={
      <>
        <SideLink to="/others/help"><CircleHelp size={16} /> {t("others.help")}</SideLink>
        <SideLink to="/others/logs"><ScrollText size={16} /> {t("others.logs")}</SideLink>
        <SideLink to="/others/crash"><Bug size={16} /> {t("others.crash")}</SideLink>
      </>
    }>
      <Routes>
        <Route index element={<Navigate to="help" replace />} />
        <Route path="help" element={<HelpPage />} />
        <Route path="logs" element={<LogPage />} />
        <Route path="crash" element={<CrashPage />} />
      </Routes>
    </SplitPage>
  );
}
