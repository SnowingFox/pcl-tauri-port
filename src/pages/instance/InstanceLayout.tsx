import { Route, Routes } from "react-router-dom";
import { SideLink, SplitPage } from "../../components/Shell";
import { Empty, LoadError, Loading } from "../../components/ui";
import { useGame } from "../../store/game";
import { reportError } from "../../lib/notify";
import { t } from "../../i18n";
import InstanceDetail from "./InstanceDetail";

export default function InstanceLayout() {
  const { instances, instancesLoaded, instancesError, loadInstances } = useGame();
  return (
    <SplitPage side={
      <>
        <div className="group">{t("instance.title")}</div>
        {instances.map((i) => <SideLink key={i.id} to={`/instances/${encodeURIComponent(i.id)}`}>{i.name}{i.error ? " ⚠" : ""}</SideLink>)}
      </>
    }>
      {!instancesLoaded ? <Loading />
        : instancesError ? <LoadError text={`${t("launch.instancesFailed")}：${instancesError}`} onRetry={() => void loadInstances().catch((e) => reportError(e))} />
        : (
        <Routes>
          <Route index element={<Empty text={instances.length ? t("instance.selectHint") : t("instance.noInstances")} />} />
          <Route path=":id/*" element={<InstanceDetail />} />
        </Routes>
      )}
    </SplitPage>
  );
}
