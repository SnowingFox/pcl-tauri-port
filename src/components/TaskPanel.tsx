import { Pause, Play, X } from "lucide-react";
import { Button, Card, ProgressBar } from "./ui";
import { formatSpeed } from "./format";
import { activeTasks, useTasks, type TaskView } from "../store/tasks";
import { t } from "../i18n";
import type { DownloadLoad } from "../api/types";

const STAGE_LABEL: Record<string, string> = {
  "version-json": "版本信息", "client-jar": "游戏本体", libraries: "依赖库", "asset-index": "资源索引", assets: "游戏资源",
  "log-config": "日志配置", "loader-profile": "加载器配置", installer: "安装器", "loader-libraries": "加载器依赖库", processors: "处理文件",
};

function loadVars(l: DownloadLoad) {
  return {
    files: l.activeFiles, maxFiles: l.concurrency, conns: l.activeConnections, segments: l.segments,
    threads: l.workerThreads, cpu: l.cpuCores.toFixed(1), cores: l.cores,
  };
}

function TaskRow({ task }: { task: TaskView }) {
  const { pause, resume, cancel } = useTasks();
  const running = task.status === "running";
  const paused = task.status === "paused";
  const pct = Math.round(task.fraction * 100);
  return (
    <li className="col" style={{ gap: 6 }}>
      <div className="row">
        <strong className="grow">{task.label || task.taskId}</strong>
        {(running || paused) && (
          <>
            <Button variant="ghost" aria-label={`${paused ? t("common.resume") : t("common.pause")} ${task.label}`} onClick={() => (paused ? resume(task.taskId) : pause(task.taskId))}>
              {paused ? <Play size={14} /> : <Pause size={14} />}
            </Button>
            <Button variant="ghost" aria-label={`${t("common.cancel")} ${task.label}`} onClick={() => cancel(task.taskId)}><X size={14} /></Button>
          </>
        )}
      </div>
      <ProgressBar value={task.fraction} label={task.label || t("download.installing")} />
      <div className="dim" style={{ fontSize: 12 }}>
        {paused ? t("common.pause") : STAGE_LABEL[task.stage] ?? task.stage}
        {task.totalFiles > 0 && ` · ${task.doneFiles}/${task.totalFiles}`}
        {running && task.speedBps > 0 && ` · ${formatSpeed(task.speedBps)}`}
        {` · ${pct}%`}
      </div>
      {running && task.load && task.load.concurrency > 0 && (
        <div className="dim" style={{ fontSize: 12 }} title={t("download.load", loadVars(task.load))}>
          {t("download.load", loadVars(task.load))}
        </div>
      )}
    </li>
  );
}

/** Active install/download tasks with pause, resume and cancel. Renders nothing when idle. */
export function TaskPanel() {
  const s = useTasks();
  const active = activeTasks(s);
  const finished = s.order.map((id) => s.tasks[id]).filter((x) => x && (x.status === "failed" || x.status === "cancelled")).slice(-3);
  if (!active.length && !finished.length) return null;
  return (
    <Card title={t("download.tasks")} actions={finished.length ? <Button variant="ghost" onClick={s.clearFinished}>{t("download.clearFinished")}</Button> : undefined}>
      <ul className="col" style={{ listStyle: "none", margin: 0, padding: 0 }} aria-live="polite">
        {active.map((x) => <TaskRow key={x.taskId} task={x} />)}
        {finished.map((x) => (
          <li key={x.taskId} className="dim" style={{ fontSize: 12 }}>
            {x.label || x.taskId}：{x.status === "cancelled" ? t("common.cancel") : x.error ?? t("download.failed")}
          </li>
        ))}
      </ul>
    </Card>
  );
}
