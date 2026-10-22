//! Java discovery and selection (macOS first, other platforms best-effort).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JavaInfo {
    /// Path to the `java` executable.
    pub path: String,
    /// JAVA_HOME-style root (parent of bin/).
    pub home: String,
    /// Full version string, e.g. "17.0.9" or "1.8.0_392".
    pub version: String,
    /// Major version: 8, 17, 21...
    pub major: u32,
    /// "arm64", "x64", "x86" or "unknown".
    pub arch: String,
    pub vendor: String,
    pub is_jdk: bool,
}

/// "1.8.0_351" -> 8, "17.0.2" -> 17, "21" -> 21, "9-ea" -> 9, "25-beta" -> 25.
pub fn parse_major(version: &str) -> Option<u32> {
    let v = version.trim().trim_matches('"');
    let mut parts = v.split(|c: char| c == '.' || c == '_' || c == '-' || c == '+');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Extract the quoted version from `java -version` output.
pub fn parse_java_version_output(out: &str) -> Option<String> {
    for line in out.lines() {
        if let Some(i) = line.find(" version \"") {
            let rest = &line[i + 10..];
            return rest.find('"').map(|e| rest[..e].to_string());
        }
    }
    None
}

fn normalize_arch(s: &str) -> String {
    match s.trim().trim_matches('"') {
        "aarch64" | "arm64" => "arm64",
        "x86_64" | "amd64" | "x64" => "x64",
        "x86" | "i386" | "i586" => "x86",
        _ => "unknown",
    }
    .to_string()
}

fn release_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Read the architecture from a Mach-O (thin or fat) or ELF binary header.
pub fn binary_arch(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = [0u8; 20];
    std::fs::File::open(path).ok()?.read_exact(&mut buf).ok()?;
    let be = |o: usize| u32::from_be_bytes(buf[o..o + 4].try_into().unwrap());
    let le = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    match &buf[0..4] {
        [0xCF, 0xFA, 0xED, 0xFE] | [0xCE, 0xFA, 0xED, 0xFE] => Some(match le(4) {
            0x0100000C => "arm64",
            0x01000007 => "x64",
            7 => "x86",
            _ => "unknown",
        }
        .into()),
        // Fat/universal: report native arch if present, else first slice.
        [0xCA, 0xFE, 0xBA, 0xBE] => {
            let n = be(4).min(4) as usize;
            if n == 0 || n > 4 {
                return None; // Java class files share this magic.
            }
            let mut f = std::fs::File::open(path).ok()?;
            let mut hdr = vec![0u8; 8 + 20 * n];
            f.read_exact(&mut hdr).ok()?;
            let mut archs = Vec::new();
            for i in 0..n {
                let t = u32::from_be_bytes(hdr[8 + 20 * i..12 + 20 * i].try_into().unwrap());
                archs.push(match t {
                    0x0100000C => "arm64",
                    0x01000007 => "x64",
                    _ => "unknown",
                });
            }
            let native = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
            Some(if archs.contains(&native) { native } else { archs[0] }.into())
        }
        [0x7F, b'E', b'L', b'F'] => Some(match u16::from_le_bytes([buf[18], buf[19]]) {
            0xB7 => "arm64",
            0x3E => "x64",
            0x03 => "x86",
            _ => "unknown",
        }
        .into()),
        _ => None,
    }
}

/// Only a real Java launcher may be executed with arguments that come from the UI:
/// the file must be named `java` (`java.exe` on Windows) and be a Mach-O / ELF binary.
/// This keeps `java_inspect`, `launch_start` and `install_loader` from being turned into
/// "run any program" primitives (`/bin/sh`, scripts, `rm`, ...).
pub fn is_java_launcher(exe: &Path) -> bool {
    let want = if cfg!(windows) { "java.exe" } else { "java" };
    let named = |p: &Path| p.file_name().map(|n| n == want).unwrap_or(false);
    // Both the given path and its symlink target must be called `java`, so a link named
    // `java` that points at /bin/sh is refused.
    named(exe) && exe.canonicalize().map(|c| named(&c) && c.is_file()).unwrap_or(false) && is_executable(exe) && binary_arch(exe).is_some()
}

/// The launcher never sets the executable bit on anything it downloads or unzips, so a file
/// fetched through `download_start_file` or a modpack can never pass as a Java launcher.
fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Validate a user-supplied java executable path before it is executed.
pub fn check_java_exe(path: &str) -> Result<(), String> {
    if is_java_launcher(Path::new(path)) {
        Ok(())
    } else {
        Err(format!("不是有效的 Java 可执行文件（必须是名为 java 的程序）：{path}"))
    }
}

/// Like [`check_java_exe`] but also accepts a java home directory (`<home>/bin/java`),
/// which is what settings and the file picker may hold.
pub fn check_java_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    let exe = if p.is_dir() { p.join("bin").join(if cfg!(windows) { "java.exe" } else { "java" }) } else { p.to_path_buf() };
    if is_java_launcher(&exe) {
        Ok(())
    } else {
        Err(format!("不是有效的 Java（必须是名为 java 的程序，或包含 bin/java 的目录）：{path}"))
    }
}

/// Inspect a java home or executable path.
pub fn inspect(path: &Path) -> Option<JavaInfo> {
    let exe_name = if cfg!(windows) { "java.exe" } else { "java" };
    let (home, exe) = if path.is_file() {
        let home = path.parent()?.parent()?.to_path_buf();
        (home, path.to_path_buf())
    } else {
        (path.to_path_buf(), path.join("bin").join(exe_name))
    };
    if !is_java_launcher(&exe) {
        return None;
    }
    let exe = exe.canonicalize().unwrap_or(exe);
    let home = if home.join("bin").join(exe_name).is_file() { home } else { exe.parent()?.parent()?.to_path_buf() };
    let release = std::fs::read_to_string(home.join("release")).unwrap_or_default();
    let mut version = release_value(&release, "JAVA_VERSION");
    let mut vendor = release_value(&release, "IMPLEMENTOR").unwrap_or_default();
    let mut arch = release_value(&release, "OS_ARCH").map(|a| normalize_arch(&a));
    if let Some(a) = binary_arch(&exe) {
        if a != "unknown" {
            arch = Some(a);
        }
    }
    if version.is_none() {
        // The macOS /usr/bin/java stub may pop an installer dialog; callers filter it out.
        let out = Command::new(&exe).arg("-version").output().ok()?;
        let text = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
        version = parse_java_version_output(&text);
        if vendor.is_empty() {
            vendor = text.lines().nth(1).unwrap_or("").split_whitespace().next().unwrap_or("").to_string();
        }
    }
    let version = version?;
    let major = parse_major(&version)?;
    Some(JavaInfo {
        path: exe.display().to_string(),
        home: home.display().to_string(),
        version,
        major,
        arch: arch.unwrap_or_else(|| "unknown".into()),
        vendor,
        is_jdk: home.join("bin").join(if cfg!(windows) { "javac.exe" } else { "javac" }).is_file(),
    })
}

/// Parse `/usr/libexec/java_home -V` stderr lines: last column is the home path.
pub fn parse_java_home_v(out: &str) -> Vec<PathBuf> {
    out.lines()
        .filter_map(|l| {
            let l = l.trim();
            let i = l.rfind(" /")?;
            l.starts_with(|c: char| c.is_ascii_digit()).then(|| PathBuf::from(l[i + 1..].trim()))
        })
        .collect()
}

fn children(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

/// Candidate java homes from well-known locations. `extra_dirs` may hold the
/// launcher's own runtime folders (e.g. `<.minecraft>/runtime`).
pub fn candidate_homes(extra_dirs: &[PathBuf]) -> BTreeSet<PathBuf> {
    let mut set = BTreeSet::new();
    let home_dir = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(jh) = std::env::var_os("JAVA_HOME") {
        set.insert(PathBuf::from(jh));
    }
    #[cfg(target_os = "macos")]
    {
        let mut jvm_dirs = vec![PathBuf::from("/Library/Java/JavaVirtualMachines")];
        if let Some(h) = &home_dir {
            jvm_dirs.push(h.join("Library/Java/JavaVirtualMachines"));
        }
        for d in jvm_dirs {
            for c in children(&d) {
                set.insert(c.join("Contents/Home"));
            }
        }
        if let Ok(out) = Command::new("/usr/libexec/java_home").arg("-V").output() {
            set.extend(parse_java_home_v(&String::from_utf8_lossy(&out.stderr)));
        }
    }
    // Homebrew (Apple Silicon and Intel prefixes).
    for prefix in ["/opt/homebrew", "/usr/local"] {
        for c in children(&Path::new(prefix).join("opt")) {
            let n = c.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if n.starts_with("openjdk") || n.starts_with("temurin") || n.starts_with("zulu") || n.starts_with("oracle-jdk") {
                set.insert(c.join("libexec/openjdk.jdk/Contents/Home"));
                set.insert(c.join("libexec"));
                set.insert(c.clone());
            }
        }
    }
    if let Some(h) = &home_dir {
        for c in children(&h.join(".sdkman/candidates/java")) {
            set.insert(c);
        }
        for c in children(&h.join(".jdks")) {
            set.insert(c.join("Contents/Home"));
            set.insert(c);
        }
    }
    #[cfg(target_os = "linux")]
    for c in children(Path::new("/usr/lib/jvm")) {
        set.insert(c);
    }
    // Mojang-provided runtimes: runtime/<component>/<platform>/<component>/[jre.bundle/Contents/Home]
    for base in extra_dirs {
        for comp in children(base) {
            for plat in children(&comp) {
                for inner in children(&plat) {
                    set.insert(inner.join("jre.bundle/Contents/Home"));
                    set.insert(inner.clone());
                }
            }
        }
    }
    set
}

pub fn discover(extra_dirs: &[PathBuf]) -> Vec<JavaInfo> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<JavaInfo> = candidate_homes(extra_dirs)
        .iter()
        .filter(|h| h.join("bin").join("java").is_file())
        .filter_map(|h| inspect(h))
        .filter(|j| seen.insert(j.path.clone()))
        .collect();
    out.sort_by(|a, b| b.major.cmp(&a.major).then(a.path.cmp(&b.path)));
    out
}

fn native_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
}

/// Pick the best Java for `required_major`: exact major first, otherwise the
/// smallest newer one. Native-arch builds are preferred; x64 on arm64 (Rosetta)
/// works but is only used if nothing else matches. Returns None if no
/// installed Java has major >= required.
pub fn select_for<'a>(javas: &'a [JavaInfo], required_major: u32) -> Option<&'a JavaInfo> {
    let native = native_arch();
    javas
        .iter()
        .filter(|j| j.major >= required_major)
        .filter(|j| j.arch == native || j.arch == "unknown" || cfg!(target_arch = "aarch64"))
        .min_by_key(|j| (j.arch != native, j.major != required_major, j.major, !j.is_jdk as u8))
}

/// Fallback major from the MC version id when the json has no `javaVersion`.
pub fn default_major_for_mc(mc: &str) -> u32 {
    let mut it = mc.split('.');
    let _ = it.next();
    let minor: u32 = it.next().and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).unwrap_or(0);
    let patch: u32 = it.next().and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).unwrap_or(0);
    match (minor, patch) {
        (m, _) if m >= 21 => 21,
        (20, p) if p >= 5 => 21,
        (m, _) if m >= 18 => 17,
        (17, _) => 16,
        _ => 8,
    }
}

#[tauri::command]
pub fn java_discover(extra_dirs: Option<Vec<String>>) -> Result<Vec<JavaInfo>, String> {
    // Extra search roots (e.g. `<mc>/runtime`) must lie inside a registered Minecraft folder:
    // discovery lists directories and executes the `java` binaries it finds.
    let dirs: Vec<PathBuf> = extra_dirs
        .unwrap_or_default()
        .iter()
        .map(|d| crate::settings::guard::confine_to_registered(Path::new(d)))
        .collect::<Result<_, _>>()?;
    Ok(discover(&dirs))
}

/// Validate a user-supplied java executable or home.
#[tauri::command]
pub fn java_inspect(path: String) -> Result<JavaInfo, String> {
    inspect(Path::new(&path)).ok_or_else(|| format!("not a usable Java: {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn j(major: u32, arch: &str) -> JavaInfo {
        JavaInfo { path: format!("/j{major}{arch}/bin/java"), home: "/x".into(), version: major.to_string(), major, arch: arch.into(), vendor: "".into(), is_jdk: true }
    }

    #[test]
    fn majors() {
        assert_eq!(parse_major("1.8.0_351"), Some(8));
        assert_eq!(parse_major("17.0.2"), Some(17));
        assert_eq!(parse_major("21"), Some(21));
        assert_eq!(parse_major("9-ea"), Some(9));
        assert_eq!(parse_major("25-beta"), Some(25));
        assert_eq!(parse_major("11.0.21+9"), Some(11));
        assert_eq!(parse_major("abc"), None);
    }

    #[test]
    fn version_output() {
        let o = "openjdk version \"17.0.9\" 2023-10-17\nOpenJDK Runtime Environment Temurin-17.0.9+9 (build 17.0.9+9)\n";
        assert_eq!(parse_java_version_output(o).as_deref(), Some("17.0.9"));
        assert_eq!(parse_java_version_output("java version \"1.8.0_351\"\n").as_deref(), Some("1.8.0_351"));
    }

    #[test]
    fn java_home_v() {
        let o = "Matching Java Virtual Machines (2):\n    21.0.1 (arm64) \"Eclipse Adoptium\" - \"Eclipse Temurin 21\" /Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home\n    1.8.0_392 (x86_64) \"Azul Systems, Inc.\" - \"Zulu 8.76.0.17\" /Library/Java/JavaVirtualMachines/zulu-8.jdk/Contents/Home\n/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home\n";
        let v = parse_java_home_v(o);
        assert_eq!(v.len(), 2);
        assert!(v[1].ends_with("zulu-8.jdk/Contents/Home"));
    }

    #[test]
    fn release_parse() {
        let r = "JAVA_VERSION=\"21.0.1\"\nIMPLEMENTOR=\"Eclipse Adoptium\"\nOS_ARCH=\"aarch64\"\n";
        assert_eq!(release_value(r, "JAVA_VERSION").as_deref(), Some("21.0.1"));
        assert_eq!(normalize_arch(&release_value(r, "OS_ARCH").unwrap()), "arm64");
    }

    #[test]
    fn selection() {
        let native = native_arch();
        let list = vec![j(8, native), j(17, native), j(21, native)];
        assert_eq!(select_for(&list, 17).unwrap().major, 17);
        assert_eq!(select_for(&list, 8).unwrap().major, 8);
        assert_eq!(select_for(&list, 11).unwrap().major, 17);
        assert!(select_for(&list, 25).is_none());
        let other = if native == "arm64" { "x64" } else { "arm64" };
        let list = vec![j(17, other), j(21, native)];
        if cfg!(target_arch = "aarch64") {
            // native build preferred over exact-major Rosetta build
            assert_eq!(select_for(&list, 17).unwrap().major, 21);
        } else {
            assert_eq!(select_for(&list, 17).unwrap().major, 21);
        }
    }

    #[cfg(unix)]
    #[test]
    fn inspect_refuses_non_java_executables() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        // A script named like a JVM that would print a plausible version if it were run.
        let bin = d.path().join("evil/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("java");
        std::fs::write(&script, "#!/bin/sh\ntouch \"$(dirname \"$0\")/pwned\"\necho 'java version \"17.0.1\"' >&2\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(inspect(&script).is_none(), "script named java must not be executed");
        assert!(inspect(&d.path().join("evil")).is_none());
        assert!(!bin.join("pwned").exists(), "the script must not have run");
        // A real binary that is not called java.
        assert!(inspect(Path::new("/bin/sh")).is_none());
        assert!(inspect(Path::new("/bin/ls")).is_none());
        assert!(check_java_exe("/bin/sh").is_err());
        assert!(check_java_exe("").is_err());
        assert!(check_java_exe("../../bin/sh").is_err());
        // A real Mach-O copied to a file called `java` passes the shape check (it is only run with -version).
        let copy = d.path().join("ok/bin/java");
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::copy("/bin/ls", &copy).unwrap();
        assert!(is_java_launcher(&copy));
        // Without the executable bit (everything the launcher writes itself) it is refused.
        std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_java_launcher(&copy));
        assert!(inspect(&copy).is_none());
        std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755)).unwrap();
        // A symlink called `java` that points at another program is refused.
        let link = d.path().join("link/bin/java");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink("/bin/sh", &link).unwrap();
        assert!(!is_java_launcher(&link));
        assert!(inspect(&link).is_none());
    }

    #[test]
    fn mc_defaults() {
        assert_eq!(default_major_for_mc("1.16.5"), 8);
        assert_eq!(default_major_for_mc("1.17.1"), 16);
        assert_eq!(default_major_for_mc("1.20.1"), 17);
        assert_eq!(default_major_for_mc("1.20.5"), 21);
        assert_eq!(default_major_for_mc("1.21"), 21);
    }
}
