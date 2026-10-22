//! Crash log analysis. Port of PCL `ModCrash.vb` (`CrashAnalyzer`), adapted for macOS.
//! Derived from Plain Craft Launcher 2.

pub mod analyzer;
pub mod collect;
pub mod commands;
pub mod reasons;
pub mod text;

use analyzer::{Analyzer, Logs};
use reasons::{Reason, SuggestedAction, describe};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CrashReasonResult {
    /// Stable machine id, e.g. `out_of_memory`.
    pub id: &'static str,
    /// Details such as mod file names, dependency lines, config file names.
    pub details: Vec<String>,
    /// Chinese, human readable explanation.
    pub message: String,
    pub actions: Vec<SuggestedAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrashAnalysis {
    pub reasons: Vec<CrashReasonResult>,
    /// All reason messages joined the way PCL shows them (with trailing hints).
    pub summary: String,
    /// Files that were used for the analysis.
    pub files_used: Vec<String>,
    /// File the UI should offer to open ("查看输出").
    pub primary_file: Option<String>,
}

const SHARE_HINT: &str = "如果要寻求帮助，请把错误报告文件发给对方，而不是发送这个窗口的照片或者截图。";
const VIEW_HINT: &str = "你可以查看错误报告了解错误具体是如何发生的。";

/// Run the rules over prepared logs.
#[allow(dead_code)]
pub fn analyze_logs(logs: &Logs) -> Vec<(Reason, Vec<String>)> {
    Analyzer::new(logs).run()
}

/// Analyze and format. `manual` mirrors PCL's `IsHandAnalyze` (user imported a log).
pub fn analyze_to_result(logs: &Logs, manual: bool) -> CrashAnalysis {
    let a = Analyzer::new(logs);
    let log_all = a.log_all().to_string();
    let found = a.run();

    if found.is_empty() {
        let summary = if manual {
            "很抱歉，启动器无法确定错误原因。".to_string()
        } else {
            format!("很抱歉，你的游戏出现了一些问题……\n{SHARE_HINT}")
        };
        return CrashAnalysis { reasons: vec![], summary, files_used: vec![], primary_file: None };
    }

    let mut results = vec![];
    let mut msgs = vec![];
    let mut any_share = false;
    for (reason, details) in found {
        let d = describe(reason, &details, &log_all);
        let mut m = d.message.clone();
        if d.view_report_hint && !manual {
            m.push('\n');
            m.push_str(VIEW_HINT);
        }
        any_share |= d.share_hint;
        msgs.push(m);
        results.push(CrashReasonResult { id: reason.id(), details, message: d.message, actions: d.actions });
    }
    let mut summary = msgs.join("\n\n此外，").trim().to_string();
    if any_share && !manual {
        summary.push('\n');
        summary.push_str(SHARE_HINT);
    }
    CrashAnalysis { reasons: results, summary, files_used: vec![], primary_file: None }
}

#[cfg(test)]
mod tests;
