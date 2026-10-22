import { Route, Routes } from "react-router-dom";
import { SideLink, SplitPage } from "../../components/Shell";
import { Empty } from "../../components/ui";
import { useGame } from "../../store/game";
import { t } from "../../i18n";
import InstanceDetail from "./InstanceDetail";

export default function InstanceLayout() {
  const instances = useGame((s) => s.instances);
  return (
    <SplitPage side={
      <>
        <div className="group">{t("instance.title")}</div>
        {instances.map((i) => <SideLink key={i.id} to={`/instances/${encodeURIComponent(i.id)}`}>{i.name}</SideLink>)}
      </>
    }>
      <Routes>
        <Route index element={<Empty text={instances.length ? t("instance.selectHint") : t("launch.noInstance")} />} />
        <Route path=":id/*" element={<InstanceDetail />} />
      </Routes>
    </SplitPage>
  );
}
