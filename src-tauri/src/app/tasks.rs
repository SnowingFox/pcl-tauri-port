//! Helpers to run download-manager tasks and wait for them.

use super::error::{AppError, AppResult};
use crate::download::error::DlResult;
use crate::download::{Manager, SpawnOpts, TaskCtx};
use std::future::Future;
use std::sync::Arc;

/// Start a manager task and wait until it finished. The task is visible to the UI like any other
/// (`download://progress`, `download://finished`, `download_cancel`).
pub async fn run_task<F, Fut>(mgr: &Arc<Manager>, opts: SpawnOpts, kind: &str, label: &str, f: F) -> AppResult<String>
where
    F: FnOnce(TaskCtx) -> Fut + Send + 'static,
    Fut: Future<Output = DlResult<()>> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    let id = mgr.spawn_with(opts, kind, label, move |ctx| async move {
        let r = f(ctx).await;
        let _ = tx.send(r.as_ref().map(|_| ()).map_err(AppError::from));
        r
    });
    match rx.await {
        Ok(Ok(())) => Ok(id),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(AppError::new("INTERNAL", "下载任务意外终止")),
    }
}

/// Poll until the manager reports the task as finished.
#[cfg(test)]
pub async fn wait_finished(mgr: &Arc<Manager>, task_id: &str) -> crate::download::TaskInfo {
    loop {
        if let Ok(i) = mgr.info(task_id)
            && !matches!(i.status, crate::download::TaskStatus::Running | crate::download::TaskStatus::Paused)
        {
            return i;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
