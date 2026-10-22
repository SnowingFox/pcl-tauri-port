//! Structured logging (tracing + daily rolling file), secret masking and the panic hook.
//!
//! Access tokens must never reach a log. Every token we handle is registered with
//! [`register_secret`]; the file writer, the launch command line and error messages all pass
//! through [`mask`], which additionally scrubs well-known token shapes.

use regex::Regex;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn secrets() -> &'static RwLock<Vec<String>> {
    static S: OnceLock<RwLock<Vec<String>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Remember a secret value so it is replaced by `***` in all later log output.
pub fn register_secret(s: &str) {
    // Very short values would mangle ordinary text; real tokens are long.
    if s.len() < 8 {
        return;
    }
    let mut g = secrets().write().unwrap();
    if !g.iter().any(|x| x == s) {
        if g.len() >= 64 {
            g.remove(0);
        }
        g.push(s.to_string());
    }
}

fn patterns() -> &'static [(Regex, &'static str)] {
    static P: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    P.get_or_init(|| {
        [
            (r"(?i)(--accessToken[ =]+)\S+", "${1}***"),
            (r"(?i)(bearer\s+)[A-Za-z0-9._~+/=\-]{12,}", "${1}***"),
            (r#"(?i)("?(?:access|refresh|id|client)_?token"?\s*[:=]\s*"?)[A-Za-z0-9._~+/=\-]{12,}"#, "${1}***"),
            (r"token:[^:\s]{8,}:", "token:***:"),
        ]
        .into_iter()
        .map(|(re, rep)| (Regex::new(re).expect("static regex"), rep))
        .collect()
    })
}

/// Replace registered secrets and token-shaped strings with `***`.
pub fn mask(s: &str) -> String {
    let mut out = s.to_string();
    for sec in secrets().read().unwrap().iter() {
        if out.contains(sec.as_str()) {
            out = out.replace(sec.as_str(), "***");
        }
    }
    for (re, rep) in patterns() {
        if re.is_match(&out) {
            out = re.replace_all(&out, *rep).into_owned();
        }
    }
    out
}

/// `io::Write` adapter that masks every chunk before passing it on.
pub struct MaskingWriter<W: Write>(pub W);

impl<W: Write> Write for MaskingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let masked = mask(&String::from_utf8_lossy(buf));
        self.0.write_all(masked.as_bytes())?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn log_dir() -> Option<&'static Path> {
    LOG_DIR.get().map(|p| p.as_path())
}

/// Install the global subscriber (idempotent) writing to `<dir>/pcl.<date>.log`, keeping 7 files.
pub fn init(dir: &Path, debug: bool) {
    if LOG_DIR.set(dir.to_path_buf()).is_err() {
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("pcl")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir);
    let appender = match appender {
        Ok(a) => a,
        Err(e) => {
            eprintln!("cannot open log directory {}: {e}", dir.display());
            return;
        }
    };
    let (writer, guard) = tracing_appender::non_blocking(MaskingWriter(appender));
    let _ = GUARD.set(guard);
    let default = if debug { "info,pcl_tauri_port_lib=debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let file_layer = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(writer);
    let stderr_layer = cfg!(debug_assertions).then(|| {
        tracing_subscriber::fmt::layer().with_ansi(false).with_writer(|| MaskingWriter(std::io::stderr()))
    });
    let _ = tracing_subscriber::registry().with(filter).with(file_layer).with(stderr_layer).try_init();
    install_panic_hook(dir.to_path_buf());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), dir = %dir.display(), "logging initialised");
}

/// Log panics (message, location, backtrace). The release profile aborts on panic, so the
/// report is also appended synchronously to `panic.log`.
pub(crate) fn install_panic_hook(dir: PathBuf) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".into());
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let bt = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current();
        let report = mask(&format!(
            "panic in thread '{}' at {loc}: {payload}\n{bt}",
            thread.name().unwrap_or("<unnamed>")
        ));
        tracing::error!(target: "panic", "{report}");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("panic.log")) {
            let _ = writeln!(f, "[{}] {report}", unix_ts());
        }
        prev(info);
    }));
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_registered_and_shaped_tokens() {
        register_secret("eyJhbGciOiJIUzI1NiJ9.secretpayload.sig");
        let line = "java -cp x --accessToken eyJhbGciOiJIUzI1NiJ9.secretpayload.sig --uuid abc";
        let m = mask(line);
        assert!(!m.contains("secretpayload"), "{m}");
        assert!(m.contains("--accessToken ***"));
        assert_eq!(mask("Authorization: Bearer abcdefghijklmnop1234"), "Authorization: Bearer ***");
        assert_eq!(mask(r#"{"access_token":"abcdefghijklmnopqrst"}"#), r#"{"access_token":"***"}"#);
        assert_eq!(mask("--session token:abcdefgh12345678:uuid"), "--session token:***:uuid");
        assert_eq!(mask("plain text 1.20.1"), "plain text 1.20.1");
    }

    #[test]
    fn short_values_are_not_registered() {
        register_secret("abc");
        assert_eq!(mask("abc def"), "abc def");
    }

    #[test]
    fn masking_writer_masks_chunks() {
        register_secret("supersecret-token-value");
        let mut w = MaskingWriter(Vec::new());
        w.write_all(b"x supersecret-token-value y").unwrap();
        assert_eq!(String::from_utf8(w.0).unwrap(), "x *** y");
    }
}
