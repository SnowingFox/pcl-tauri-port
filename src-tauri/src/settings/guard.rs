//! Trust-boundary helpers: validation for arguments that arrive from the webview.
//!
//! The webview is treated as untrusted input. Commands that take a path, URL or
//! name from JS must pass it through these helpers before touching the disk,
//! the network or spawning a process. The pure functions take their roots as
//! parameters so they can be unit-tested without the global settings store.

use super::commands::global;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Component, Path, PathBuf};

/// Minecraft folders the user registered in settings (the trust anchor for file access).
pub fn registered_roots() -> Vec<PathBuf> {
    global().get().game.folders.iter().map(|f| PathBuf::from(&f.path)).collect()
}

/// Lexically clean an absolute path. Rejects relative paths, `..` and NUL bytes.
pub fn clean_abs(p: &Path) -> Result<PathBuf, String> {
    if p.to_string_lossy().contains('\0') {
        return Err("路径包含非法字符".into());
    }
    if !p.is_absolute() {
        return Err(format!("需要绝对路径：{}", p.display()));
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => return Err(format!("路径不能包含 ..：{}", p.display())),
            Component::Normal(s) => out.push(s),
        }
    }
    Ok(out)
}

/// Canonicalize the deepest existing ancestor (resolving symlinks) and re-append the
/// not-yet-existing tail. The path itself does not have to exist.
pub fn resolve(p: &Path) -> Result<PathBuf, String> {
    let p = clean_abs(p)?;
    let mut tail = Vec::new();
    let mut cur = p.as_path();
    loop {
        if let Ok(mut c) = cur.canonicalize() {
            for t in tail.iter().rev() {
                c.push(t);
            }
            return Ok(c);
        }
        match (cur.file_name(), cur.parent()) {
            (Some(n), Some(par)) => {
                tail.push(n.to_owned());
                cur = par;
            }
            _ => return Err(format!("路径无效：{}", p.display())),
        }
    }
}

/// `p` (symlinks resolved) must be inside one of `roots`. Returns the resolved path.
pub fn confine(roots: &[PathBuf], p: &Path) -> Result<PathBuf, String> {
    let r = resolve(p)?;
    for root in roots {
        if let Ok(rr) = resolve(root)
            && r.starts_with(&rr)
        {
            return Ok(r);
        }
    }
    Err(format!("路径不在已登记的 Minecraft 文件夹内：{}", p.display()))
}

/// Like [`confine`] with the registered Minecraft folders as roots.
pub fn confine_to_registered(p: &Path) -> Result<PathBuf, String> {
    confine(&registered_roots(), p)
}

/// `dir` must be exactly `<root>/versions/<name>` for a registered root.
pub fn confine_instance_dir(roots: &[PathBuf], dir: &Path) -> Result<PathBuf, String> {
    let r = confine(roots, dir)?;
    let name_ok = r.file_name().and_then(|n| n.to_str()).map(|n| crate::instances::detect::validate_name(n).is_ok()).unwrap_or(false);
    let versions = r.parent();
    let in_versions = versions.and_then(|v| v.file_name()).map(|n| n == "versions").unwrap_or(false);
    let root_ok =
        versions.and_then(|v| v.parent()).map(|root| roots.iter().any(|x| resolve(x).map(|x| x == root).unwrap_or(false))).unwrap_or(false);
    if name_ok && in_versions && root_ok { Ok(r) } else { Err(format!("不是有效的实例文件夹：{}", dir.display())) }
}

/// Folders inside a Minecraft root / instance directory that hold user content. Generic
/// file downloads may only target these, never `versions/<id>/<id>.json`, `libraries/`,
/// `assets/` or launcher metadata, because those control what the launcher executes.
pub const CONTENT_DIRS: &[&str] = &["mods", "resourcepacks", "shaderpacks", "saves", "config"];

/// `p` must be `<root>/<content>/...` or `<root>/versions/<name>/<content>/...` for a
/// registered root. Returns the resolved path and how many components lie below the content
/// folder (0 = the content folder itself).
pub fn confine_content(roots: &[PathBuf], p: &Path) -> Result<(PathBuf, usize), String> {
    let r = confine(roots, p)?;
    for root in roots {
        let Ok(rr) = resolve(root) else { continue };
        let Ok(rel) = r.strip_prefix(&rr) else { continue };
        let parts: Vec<&std::ffi::OsStr> =
            rel.components().filter_map(|c| if let Component::Normal(s) = c { Some(s) } else { None }).collect();
        let skip = match parts.first().and_then(|s| s.to_str()) {
            Some("versions") => match parts.get(1).and_then(|s| s.to_str()) {
                Some(n) if crate::instances::detect::validate_name(n).is_ok() => 2,
                _ => continue,
            },
            _ => 0,
        };
        if let Some(c) = parts.get(skip).and_then(|s| s.to_str())
            && CONTENT_DIRS.contains(&c)
        {
            let depth = parts.len() - skip - 1;
            return Ok((r.clone(), depth));
        }
    }
    Err(format!("只能写入 mods、resourcepacks、shaderpacks、saves 或 config 文件夹：{}", p.display()))
}

/// Flags that make the JVM load or run code outside the game's classpath. UI-supplied extra
/// JVM arguments must not contain them (the launcher itself adds `-javaagent` for
/// authlib-injector on a code path the webview does not control).
pub fn jvm_arg_is_dangerous(arg: &str) -> bool {
    let a = arg.trim_start().to_ascii_lowercase();
    [
        "-javaagent",
        "-agentlib",
        "-agentpath",
        "-xbootclasspath",
        "--patch-module",
        "--upgrade-module-path",
        "--add-modules=ALL",
        "-xrun",
        "-xdebug",
        "-xrunjdwp",
        "-xx:onerror",
        "-xx:onoutofmemoryerror",
        "-xx:+unlockdiagnostic",
        "-xx:vmoptionsfile",
        "-xx:flags",
        "-xx:+executeonlyexternal",
        "-xx:+createcoredumponcrash",
        "-djava.system.class.loader",
        "-djdk.attach",
        "-djava.security",
        "@",
    ]
    .iter()
    .any(|p| a.starts_with(&p.to_ascii_lowercase()))
}

/// Top-level home folders that must never become a Minecraft root.
const PROTECTED_HOME_DIRS: &[&str] = &[
    "Library",
    ".ssh",
    ".gnupg",
    ".config",
    ".aws",
    ".kube",
    ".docker",
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Movies",
    "Public",
    "Applications",
    ".Trash",
];

/// The launcher's own data folders below `$HOME` (current Tauri app dir and the legacy one).
const OWN_DATA_DIRS_UNDER_HOME: &[&str] =
    &["Library/Application Support/com.snowingfox.pcltauriport", "Library/Application Support/PCL-Tauri"];

/// System locations that must never become a Minecraft root.
const PROTECTED_SYSTEM_DIRS: &[&str] =
    &["/System", "/usr", "/bin", "/sbin", "/etc", "/dev", "/cores", "/Library", "/Applications", "/private/etc", "/private/var/db", "/opt"];

/// A folder may only be registered as a Minecraft root when it is not `/`, a system
/// directory, the home directory or one of its protected children, or related to the
/// launcher's own data directory (which holds settings and account metadata).
pub fn validate_root_candidate(path: &Path, home: &Path, data_dir: &Path) -> Result<PathBuf, String> {
    let p = resolve(path)?;
    let home = resolve(home).unwrap_or_else(|_| home.to_path_buf());
    let data = resolve(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let bad = |why: &str| Err(format!("不能把该位置用作 Minecraft 文件夹（{why}）：{}", path.display()));
    if p.parent().is_none() {
        return bad("根目录");
    }
    if p == home {
        return bad("用户主目录");
    }
    if let Ok(rel) = p.strip_prefix(&home)
        && let Some(Component::Normal(first)) = rel.components().next()
        && rel.components().count() == 1
        && PROTECTED_HOME_DIRS.iter().any(|d| first == *d)
    {
        return bad("受保护的目录");
    }
    for s in PROTECTED_SYSTEM_DIRS {
        if p.starts_with(s) {
            return bad("系统目录");
        }
    }
    // The launcher's own folders: settings (`PCL-Tauri`, or `PCL_DATA_DIR`) and the Tauri app
    // data directory that holds accounts.json (`<bundle identifier>`).
    let mut own = vec![data];
    for d in OWN_DATA_DIRS_UNDER_HOME {
        own.push(home.join(d));
    }
    for d in &own {
        if p.starts_with(d) || d.starts_with(&p) {
            return bad("与启动器数据目录重叠");
        }
    }
    // A shallow path such as /Users or /Volumes would cover other users' or disks' data.
    if p.components().count() <= 2 {
        return bad("过于靠近根目录");
    }
    Ok(p)
}

/// Production wrapper for [`validate_root_candidate`].
pub fn validate_new_root(path: &Path) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/nonexistent-home"));
    validate_root_candidate(path, &home, &super::store::data_dir())
}

/// Navigations the main webview may perform: its own bundled app, `about:blank`, and in debug
/// builds the Vite dev server. Everything else (a stray `<a href>`, `window.location = ...`,
/// a redirect) is refused so remote content never gets loaded into the webview that holds the
/// IPC bridge. External links go through the opener in Rust (`http`/`https` only).
pub fn navigation_allowed(u: &url::Url, debug: bool) -> bool {
    match u.scheme() {
        "tauri" => u.host_str() == Some("localhost"),
        "about" => u.as_str() == "about:blank",
        "http" | "https" => {
            // Tauri's Windows/Android origin; on macOS the scheme is `tauri://localhost`.
            u.host_str() == Some("tauri.localhost") || (debug && u.host_str() == Some("localhost") && u.port() == Some(1420))
        }
        _ => false,
    }
}

/// Plugin that enforces [`navigation_allowed`] for every webview.
pub fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("pcl-navigation-guard")
        .on_navigation(|_webview, url| navigation_allowed(url, cfg!(debug_assertions)))
        .build()
}

/// Which hosts a remote URL may point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPolicy {
    /// Public internet only: no loopback, private, CGNAT, link-local, `.local`/`.internal`
    /// names or bare intranet host names. Used for downloads.
    Public,
    /// Like `Public` but private LAN ranges and bare/`.local` names are allowed, for
    /// self-hosted login servers. Loopback and link-local stay blocked.
    AllowLan,
}

fn ipv4_blocked(a: Ipv4Addr, p: HostPolicy) -> bool {
    let o = a.octets();
    if a.is_loopback() || a.is_link_local() || a.is_unspecified() || a.is_broadcast() || a.is_multicast() {
        return true;
    }
    p == HostPolicy::Public && (a.is_private() || (o[0] == 100 && (64..128).contains(&o[1])))
}

fn ipv6_blocked(a: Ipv6Addr, p: HostPolicy) -> bool {
    if let Some(v4) = a.to_ipv4_mapped() {
        return ipv4_blocked(v4, p);
    }
    let s = a.segments();
    if a.is_loopback() || a.is_unspecified() || a.is_multicast() || (s[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    p == HostPolicy::Public && (s[0] & 0xfe00) == 0xfc00
}

/// Whether `ip` is blocked under `p`.
pub fn ip_blocked(ip: IpAddr, p: HostPolicy) -> bool {
    match ip {
        IpAddr::V4(a) => ipv4_blocked(a, p),
        IpAddr::V6(a) => ipv6_blocked(a, p),
    }
}

/// Host-level SSRF check shared by [`check_remote_url`] and the redirect policy.
/// Works on the host as the URL parser sees it, so `http://2130706433/` and
/// `http://0x7f.1/` are already normalised to `127.0.0.1`. DNS names that resolve to
/// internal addresses are not detected (see docs/SECURITY.md).
pub fn host_blocked(u: &url::Url, p: HostPolicy) -> bool {
    match u.host() {
        Some(url::Host::Ipv4(a)) => ipv4_blocked(a, p),
        Some(url::Host::Ipv6(a)) => ipv6_blocked(a, p),
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            if d == "localhost" || d.ends_with(".localhost") {
                return true;
            }
            p == HostPolicy::Public
                && (d.ends_with(".local")
                    || d.ends_with(".internal")
                    || d.ends_with(".lan")
                    || d.ends_with(".home.arpa")
                    || !d.contains('.'))
        }
        None => true,
    }
}

/// A URL JS may ask the launcher to fetch: http(s) only, no embedded credentials, and a
/// host allowed by `p` (SSRF).
pub fn check_remote_url(s: &str, p: HostPolicy) -> Result<url::Url, String> {
    let u = url::Url::parse(s.trim()).map_err(|e| format!("无效的 URL：{e}"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(format!("仅支持 http/https 链接：{}", u.scheme()));
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("链接不能包含用户名或密码".into());
    }
    if host_blocked(&u, p) {
        return Err("不允许访问本机或内网地址".into());
    }
    Ok(u)
}

/// Download URLs coming from JS must additionally be https, so a network attacker cannot
/// swap the payload of an unverified download.
pub fn check_download_url(s: &str) -> Result<url::Url, String> {
    let u = check_remote_url(s, HostPolicy::Public)?;
    if u.scheme() != "https" {
        return Err("下载链接必须使用 https".into());
    }
    Ok(u)
}

/// DNS resolver that drops internal answers. IP literals never reach a resolver (they are
/// covered by [`check_remote_url`] and the redirect policy), host names do: a public-looking
/// name that resolves to 127.0.0.1 / 169.254.169.254 / 10.x is refused here, at connect time,
/// which also defeats DNS rebinding between a check and the request.
pub struct GuardedResolver(pub HostPolicy);

pub(crate) fn filter_resolved(addrs: Vec<std::net::SocketAddr>, p: HostPolicy) -> Result<Vec<std::net::SocketAddr>, String> {
    let ok: Vec<_> = addrs.iter().copied().filter(|a| !ip_blocked(a.ip(), p)).collect();
    if ok.is_empty() && !addrs.is_empty() { Err("host resolves to a local or private address".into()) } else { Ok(ok) }
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let policy = self.0;
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let ok = filter_resolved(addrs, policy)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { format!("{host}: {e}").into() })?;
            Ok(Box::new(ok.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Redirect policy: at most 10 hops, never downgrade https to http, never follow into
/// hosts blocked by `p`. Direct requests are not affected, only redirect targets.
pub fn redirect_policy(p: HostPolicy) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let downgrade = attempt.previous().last().map(|u| u.scheme() == "https").unwrap_or(false) && attempt.url().scheme() != "https";
        if downgrade || !matches!(attempt.url().scheme(), "http" | "https") || host_blocked(attempt.url(), p) {
            return attempt.error("redirect to a disallowed location");
        }
        attempt.follow()
    })
}

/// Version-like tokens (Minecraft / loader / OptiFine versions) that end up in URLs and
/// file names: `[A-Za-z0-9._+-]`, 1..=64 chars, no `..`, not starting with `.` or `-`.
pub fn check_version_token(what: &str, s: &str) -> Result<(), String> {
    let ok = !s.is_empty()
        && s.len() <= 64
        && !s.contains("..")
        && !s.starts_with(['.', '-'])
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    if ok { Ok(()) } else { Err(format!("无效的{what}：{s}")) }
}

/// Minecraft version ids end up in URLs and folder names. Same alphabet as
/// [`check_version_token`] plus inner spaces (Mojang ships ids like `3D Shareware v1.34`).
pub fn check_mc_version(s: &str) -> Result<(), String> {
    let ok = !s.is_empty()
        && s.len() <= 64
        && !s.contains("..")
        && !s.starts_with(['.', '-', ' '])
        && !s.ends_with(' ')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-' | ' '));
    if ok { Ok(()) } else { Err(format!("无效的 Minecraft 版本：{s}")) }
}

/// One path component taken from remote or on-disk metadata (version id, asset index id,
/// log config id): non-empty, no separators, not `.`/`..`, no NUL.
pub fn is_safe_component(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\0'])
}

/// Asset object hashes name files below `assets/objects/<2 hex>/<hash>`: hex digits only.
pub fn is_hex_hash(s: &str) -> bool {
    (2..=128).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A relative path coming from remote metadata (library / asset paths, pack entries):
/// non-empty, no absolute prefix, no `.`/`..`/empty components, no backslashes or NUL.
pub fn is_safe_rel(p: &str) -> bool {
    !p.is_empty() && !p.contains(['\\', '\0']) && !p.starts_with('/') && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn confine_accepts_inside_and_rejects_outside() {
        let d = tmp();
        let root = d.path().join("mc");
        std::fs::create_dir_all(root.join("mods")).unwrap();
        let roots = vec![root.clone()];
        assert!(confine(&roots, &root.join("mods")).is_ok());
        assert!(confine(&roots, &root.join("mods/not-yet/a.jar")).is_ok());
        assert!(confine(&roots, &root.join("../other")).is_err());
        assert!(confine(&roots, Path::new("/etc/passwd")).is_err());
        assert!(confine(&roots, Path::new("relative/mods")).is_err());
        // Prefix match must be per component, not per string.
        let sibling = d.path().join("mc-evil");
        std::fs::create_dir_all(&sibling).unwrap();
        assert!(confine(&roots, &sibling).is_err());
    }

    #[test]
    fn confine_rejects_symlink_escape() {
        let d = tmp();
        let root = d.path().join("mc");
        let outside = d.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("mods")).unwrap();
        assert!(confine(std::slice::from_ref(&root), &root.join("mods/x.jar")).is_err());
    }

    #[test]
    fn instance_dir_shape() {
        let d = tmp();
        let root = d.path().join("mc");
        std::fs::create_dir_all(root.join("versions/1.20.1")).unwrap();
        let roots = vec![root.clone()];
        assert!(confine_instance_dir(&roots, &root.join("versions/1.20.1")).is_ok());
        assert!(confine_instance_dir(&roots, &root.join("versions")).is_err());
        assert!(confine_instance_dir(&roots, &root).is_err());
        assert!(confine_instance_dir(&roots, &root.join("versions/1.20.1/mods")).is_err());
        assert!(confine_instance_dir(&roots, &root.join("mods")).is_err());
    }

    #[test]
    fn root_candidates() {
        let d = tmp();
        let home = d.path().join("home/me");
        let data = home.join("Library/Application Support/PCL-Tauri");
        std::fs::create_dir_all(&data).unwrap();
        let ok = |p: PathBuf| validate_root_candidate(&p, &home, &data);
        assert!(ok(home.join("minecraft")).is_ok());
        assert!(ok(home.join("Library/Application Support/minecraft")).is_ok());
        assert!(ok(home.join("Games/mc")).is_ok());
        assert!(ok(PathBuf::from("/")).is_err());
        assert!(ok(PathBuf::from("/Users")).is_err());
        assert!(ok(PathBuf::from("/etc")).is_err());
        assert!(ok(PathBuf::from("/usr/local/mc")).is_err());
        assert!(ok(home.clone()).is_err());
        assert!(ok(home.join("Library")).is_err());
        assert!(ok(home.join(".ssh")).is_err());
        assert!(ok(home.join("Documents")).is_err());
        assert!(ok(home.join("Library/Application Support")).is_err(), "ancestor of the data dir");
        assert!(ok(data.join("sub")).is_err(), "inside the data dir");
        let app_dir = home.join("Library/Application Support/com.snowingfox.pcltauriport");
        assert!(ok(app_dir.clone()).is_err(), "Tauri app data dir holds accounts.json");
        assert!(ok(app_dir.join("x")).is_err());
        assert!(ok(home.join("a/../../b")).is_err());
        assert!(ok(PathBuf::from("relative")).is_err());
    }

    #[test]
    fn remote_urls() {
        assert!(check_remote_url("https://cdn.modrinth.com/data/a/b.jar", HostPolicy::Public).is_ok());
        assert!(check_remote_url("http://edge.forgecdn.net/x", HostPolicy::Public).is_ok());
        for bad in [
            "file:///etc/passwd",
            "ftp://example.com/x",
            "javascript:alert(1)",
            "data:text/plain,hi",
            "https://user:pw@example.com/x",
            "http://localhost/x",
            "http://foo.localhost/x",
            "http://127.0.0.1:8080/x",
            "http://2130706433/",
            "http://0x7f.1/",
            "http://10.0.0.5/x",
            "http://192.168.1.1/",
            "http://172.16.0.1/",
            "http://169.254.169.254/latest/meta-data",
            "http://100.64.0.1/",
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://[fe80::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://intranet/",
            "http://printer.local/",
            "not a url",
        ] {
            assert!(check_remote_url(bad, HostPolicy::Public).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn webview_navigation_is_local_only() {
        let u = |s: &str| url::Url::parse(s).unwrap();
        for ok in ["tauri://localhost/", "tauri://localhost/index.html#/launch", "about:blank", "http://tauri.localhost/"] {
            assert!(navigation_allowed(&u(ok), false), "{ok}");
        }
        assert!(navigation_allowed(&u("http://localhost:1420/"), true));
        assert!(!navigation_allowed(&u("http://localhost:1420/"), false), "dev server is debug-only");
        for bad in [
            "https://evil.example/",
            "http://evil.example/",
            "http://localhost:8080/",
            "http://tauri.localhost.evil.example/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "tauri://evil/",
            "about:srcdoc",
            "blob:https://evil.example/uuid",
        ] {
            assert!(!navigation_allowed(&u(bad), true), "{bad}");
        }
    }

    #[test]
    fn lan_policy_allows_private_but_not_loopback() {
        let lan = |u: &str| check_remote_url(u, HostPolicy::AllowLan);
        assert!(lan("http://192.168.1.20:8080/api").is_ok());
        assert!(lan("http://10.1.2.3/").is_ok());
        assert!(lan("https://auth.example.org/api/yggdrasil").is_ok());
        assert!(lan("http://nas/yggdrasil").is_ok());
        assert!(lan("http://printer.local/").is_ok());
        assert!(lan("http://127.0.0.1:25565/").is_err());
        assert!(lan("http://localhost/").is_err());
        assert!(lan("http://[::1]/").is_err());
        assert!(lan("http://169.254.169.254/").is_err());
        assert!(lan("http://0.0.0.0/").is_err());
        assert!(lan("file:///etc/passwd").is_err());
    }

    #[test]
    fn resolved_addresses_are_filtered() {
        let sa = |s: &str| -> std::net::SocketAddr { s.parse().unwrap() };
        let mixed = vec![sa("127.0.0.1:0"), sa("93.184.216.34:0"), sa("[::1]:0")];
        assert_eq!(filter_resolved(mixed, HostPolicy::Public).unwrap(), vec![sa("93.184.216.34:0")]);
        assert!(filter_resolved(vec![sa("169.254.169.254:0")], HostPolicy::Public).is_err());
        assert!(filter_resolved(vec![sa("10.0.0.7:0")], HostPolicy::Public).is_err());
        assert!(filter_resolved(vec![sa("10.0.0.7:0")], HostPolicy::AllowLan).is_ok());
        assert!(filter_resolved(vec![sa("127.0.0.1:0")], HostPolicy::AllowLan).is_err());
    }

    #[tokio::test]
    async fn localhost_name_is_refused_at_dns_time() {
        // `localhost.` passes the textual check only if it were a public name; the resolver is the backstop.
        let c = reqwest::Client::builder()
            .dns_resolver(std::sync::Arc::new(GuardedResolver(HostPolicy::Public)))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let err = c.get("http://localhost.localdomain.:9/").send().await.unwrap_err();
        assert!(err.is_connect() || err.is_request(), "{err}");
    }

    #[test]
    fn download_urls_must_be_https() {
        assert!(check_download_url("https://cdn.modrinth.com/a.jar").is_ok());
        assert!(check_download_url("http://cdn.modrinth.com/a.jar").is_err());
        assert!(check_download_url("https://127.0.0.1/a.jar").is_err());
    }

    #[tokio::test]
    async fn redirects_into_internal_hosts_are_refused() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else { return };
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                let _ = s.shutdown().await;
            }
        });
        let c = reqwest::Client::builder()
            .redirect(redirect_policy(HostPolicy::Public))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let err = c.get(format!("http://{addr}/x")).send().await.unwrap_err();
        assert!(err.is_redirect(), "expected a redirect error, got {err}");
    }

    #[test]
    fn version_tokens() {
        for ok in ["1.20.1", "47.2.0", "0.15.11", "24w14a", "1.20.1-47.2.0", "HD_U_I7", "21.0.5-beta", "1.20.4+build.3"] {
            assert!(check_version_token("v", ok).is_ok(), "{ok}");
        }
        for bad in ["", "..", "../../x", "a/b", "a\\b", "x..y", ".hidden", "-rf", "a b", "a?b", "a#b", "a%2fb", &"1".repeat(65)] {
            assert!(check_version_token("v", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn mc_versions() {
        for ok in ["1.20.1", "24w14a", "1.RV-Pre1", "3D Shareware v1.34", "1.20.1-forge-47.2.0"] {
            assert!(check_mc_version(ok).is_ok(), "{ok}");
        }
        for bad in ["", "../x", "a/b", " 1.20", "1.20 ", "1.20?x=1", "1.20#frag", "1.20%2f", "a\\b", "1..2"] {
            assert!(check_mc_version(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn content_dirs_only() {
        let d = tmp();
        let root = d.path().join("mc");
        std::fs::create_dir_all(root.join("versions/1.20.1/mods")).unwrap();
        std::fs::create_dir_all(root.join("mods")).unwrap();
        let roots = vec![root.clone()];
        let ok = |p: PathBuf| confine_content(&roots, &p);
        assert_eq!(ok(root.join("mods/a.jar")).unwrap().1, 1);
        assert_eq!(ok(root.join("mods")).unwrap().1, 0);
        assert_eq!(ok(root.join("versions/1.20.1/mods/a.jar")).unwrap().1, 1);
        assert_eq!(ok(root.join("saves/w/datapacks/p.zip")).unwrap().1, 3);
        // Things that decide what the launcher executes are not content.
        for bad in [
            "versions/1.20.1/1.20.1.json",
            "versions/1.20.1/PCL/config.json",
            "libraries/a/b/c.jar",
            "assets/indexes/x.json",
            "launcher_profiles.json",
            "versions/mods/a.jar",
            "versions/../mods/a.jar",
            "mods/../libraries/x.jar",
            "mods-evil/a.jar",
        ] {
            assert!(ok(root.join(bad)).is_err(), "{bad}");
        }
        assert!(ok(PathBuf::from("/tmp/mods/a.jar")).is_err());
        std::os::unix::fs::symlink(d.path(), root.join("config")).unwrap();
        assert!(ok(root.join("config/x")).is_err(), "symlinked content dir escapes the root");
    }

    #[test]
    fn dangerous_jvm_args() {
        for bad in [
            "-javaagent:/tmp/x.jar",
            "-JavaAgent:x",
            "-agentlib:jdwp=transport=dt_socket",
            "-agentpath:/tmp/x.dylib",
            "-Xbootclasspath/a:/tmp/x.jar",
            "--patch-module=java.base=/tmp/x",
            "-XX:OnError=open -a Calculator",
            "-XX:OnOutOfMemoryError=sh",
            "-XX:Flags=/tmp/x",
            "-XX:VMOptionsFile=/tmp/x",
            "@/tmp/argfile",
            " -javaagent:x",
            "-Djava.system.class.loader=Evil",
        ] {
            assert!(jvm_arg_is_dangerous(bad), "{bad}");
        }
        for ok in [
            "-Xmx4G",
            "-Xms512m",
            "-XX:+UseG1GC",
            "-XX:-OmitStackTraceInFastThrow",
            "-Dfml.ignoreInvalidMinecraftCertificates=True",
            "-Dminecraft.launcher.brand=pcl",
            "-XX:MaxGCPauseMillis=50",
        ] {
            assert!(!jvm_arg_is_dangerous(ok), "{ok}");
        }
    }

    #[test]
    fn components_and_hashes() {
        for ok in ["1.20.1", "1.20.1-forge-47.2.0", "3D Shareware v1.34", "12"] {
            assert!(is_safe_component(ok), "{ok}");
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "../x", "a\0"] {
            assert!(!is_safe_component(bad), "{bad:?}");
        }
        assert!(is_hex_hash("da39a3ee5e6b4b0d3255bfef95601890afd80709"));
        for bad in ["", "a", "zz", "../../etc/passwd", "ab/cd", "é", "ab\0", &"a".repeat(129)] {
            assert!(!is_hex_hash(bad), "{bad:?}");
        }
    }

    #[test]
    fn safe_rel() {
        assert!(is_safe_rel("net/minecraft/client/1.20/client-1.20.jar"));
        for bad in ["", "/etc/passwd", "../x", "a/../b", "a//b", "a/./b", "a\\b", "a/b/", "a\0b"] {
            assert!(!is_safe_rel(bad), "{bad:?}");
        }
    }
}
