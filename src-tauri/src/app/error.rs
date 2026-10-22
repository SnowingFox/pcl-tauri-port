//! Typed errors returned by the `app_*` commands: `{ code, message, details? }`.

use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    /// Stable machine code, see `docs/api/app.md`.
    pub code: &'static str,
    /// Human readable (zh-CN) fallback text. Branch on `code`, not on this.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub type AppResult<T> = Result<T, AppError>;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: super::logging::mask(&message.into()), details: None }
    }

    pub fn with(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }

    pub fn io(e: impl std::fmt::Display) -> Self {
        Self::new("IO", format!("文件系统错误：{e}"))
    }

    pub fn internal(e: impl std::fmt::Display) -> Self {
        Self::new("INTERNAL", format!("内部错误：{e}"))
    }

    pub fn forbidden(path: &std::path::Path) -> Self {
        Self::new("FORBIDDEN_PATH", format!("不允许访问该路径：{}", path.display()))
    }

    pub fn cancelled() -> Self {
        Self::new("CANCELLED", "已取消")
    }

    pub fn java_missing(required: u32, max: Option<u32>, found: &[u32]) -> Self {
        let msg = match max {
            Some(m) => format!("未找到可用的 Java：需要 Java {required}（不高于 Java {m}）"),
            None => format!("未找到可用的 Java：需要 Java {required} 或更高版本"),
        };
        Self::new("JAVA_MISSING", msg).with(json!({ "requiredMajor": required, "maxMajor": max, "found": found }))
    }
}

impl From<crate::accounts::AccountError> for AppError {
    fn from(e: crate::accounts::AccountError) -> Self {
        use crate::accounts::AccountError as E;
        let msg = e.to_string();
        match e {
            E::NotFound(id) => Self::new("ACCOUNT_NOT_FOUND", format!("账号不存在：{id}")),
            E::NeedRelogin | E::Declined | E::DeviceCodeExpired => Self::new("ACCOUNT_EXPIRED", "登录已过期，请重新登录该账号"),
            E::MissingClientId => Self::new("MS_CLIENT_ID_MISSING", "未配置 Microsoft 登录所需的应用 Client ID，无法使用微软账号启动"),
            E::Network(_) => Self::new("NETWORK", format!("网络错误：{msg}")),
            E::Io(_) => Self::new("IO", msg),
            other => Self::new("ACCOUNT_ERROR", format!("账号错误：{other}")),
        }
    }
}

impl From<&crate::download::error::DlError> for AppError {
    fn from(e: &crate::download::error::DlError) -> Self {
        use crate::download::error::DlError as E;
        match e {
            E::Cancelled => Self::cancelled(),
            E::Http(_) | E::Status(..) => Self::new("NETWORK", format!("下载失败：{e}")),
            E::Io(_) => Self::new("IO", e.to_string()),
            E::Other(m) => match m.split_once(": ") {
                // Errors raised by app tasks travel as "CODE: message"; keep the code.
                Some((code, rest)) if code.len() > 3 && code.chars().all(|c| c.is_ascii_uppercase() || c == '_') => {
                    let code = KNOWN_CODES.iter().find(|k| **k == code).copied().unwrap_or("DOWNLOAD_FAILED");
                    Self::new(code, rest.to_string())
                }
                _ => Self::new("DOWNLOAD_FAILED", format!("下载失败：{m}")),
            },
            other => Self::new("DOWNLOAD_FAILED", format!("下载失败：{other}")),
        }
    }
}

impl From<crate::download::error::DlError> for AppError {
    fn from(e: crate::download::error::DlError) -> Self {
        Self::from(&e)
    }
}

/// Codes that may be carried inside a `DlError::Other("CODE: message")`.
const KNOWN_CODES: &[&str] = &[
    "MANUAL_DOWNLOAD_REQUIRED",
    "CURSEFORGE_KEY_MISSING",
    "NO_COMPATIBLE_VERSION",
    "INSTANCE_EXISTS",
    "UNSUPPORTED",
    "NETWORK",
    "INVALID_ARGUMENT",
    "WORLD_NOT_FOUND",
    "INSTANCE_NOT_MODDABLE",
    "IO",
    "INTERNAL",
];

impl From<crate::launch::LaunchError> for AppError {
    fn from(e: crate::launch::LaunchError) -> Self {
        use crate::launch::LaunchError as E;
        match e {
            E::NoJava(v) => Self::java_missing(v, None, &[]),
            E::Spawn(m) => Self::new("LAUNCH_FAILED", format!("无法启动游戏进程：{m}")),
            E::Io(m) => Self::new("IO", m),
            E::VersionJson(m) => Self::new("INSTANCE_INVALID", format!("版本文件有误：{m}")),
            E::NotRunning(id) => Self::new("NOT_FOUND", format!("没有正在运行的游戏：{id}")),
        }
    }
}

impl From<crate::settings::store::SettingsError> for AppError {
    fn from(e: crate::settings::store::SettingsError) -> Self {
        Self::new("IO", format!("无法保存设置：{e}"))
    }
}

/// Resource-module errors are plain strings; recognise the one that needs UI action.
pub fn from_resource_error(s: String) -> AppError {
    if s.contains("API key not configured") {
        AppError::new("CURSEFORGE_KEY_MISSING", "未配置 CurseForge API Key（环境变量 PCL_CURSEFORGE_KEY）")
    } else {
        AppError::new("NETWORK", format!("资源平台请求失败：{s}"))
    }
}

/// Convert an error that happened inside a download-manager task into the `CODE: message` form
/// that ends up in `TaskInfo.error`.
pub fn into_task_error(e: AppError) -> crate::download::error::DlError {
    if e.code == "CANCELLED" {
        return crate::download::error::DlError::Cancelled;
    }
    crate::download::error::DlError::Other(format!("{}: {}", e.code, e.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::AccountError as A;

    #[test]
    fn account_errors_map_to_stable_codes() {
        let code = |e: A| AppError::from(e).code;
        assert_eq!(code(A::NeedRelogin), "ACCOUNT_EXPIRED");
        assert_eq!(code(A::NotFound("x".into())), "ACCOUNT_NOT_FOUND");
        assert_eq!(code(A::MissingClientId), "MS_CLIENT_ID_MISSING");
        assert_eq!(code(A::Network("down".into())), "NETWORK");
        assert_eq!(code(A::NotOwned), "ACCOUNT_ERROR");
    }

    #[test]
    fn task_errors_round_trip_their_code() {
        use crate::download::error::DlError;
        let e = into_task_error(AppError::new("MANUAL_DOWNLOAD_REQUIRED", "need hand download"));
        let back = AppError::from(&e);
        assert_eq!((back.code, back.message.as_str()), ("MANUAL_DOWNLOAD_REQUIRED", "need hand download"));
        assert_eq!(AppError::from(&DlError::Cancelled).code, "CANCELLED");
        assert_eq!(AppError::from(&DlError::Other("random text: with colon".into())).code, "DOWNLOAD_FAILED");
        assert_eq!(AppError::from(&DlError::Status(404, "u".into())).code, "NETWORK");
    }

    #[test]
    fn messages_are_masked() {
        crate::app::logging::register_secret("error-secret-token-424242");
        let e = AppError::new("INTERNAL", "boom error-secret-token-424242");
        assert_eq!(e.message, "boom ***");
    }
}
