//! Process limits.
//!
//! A GUI app started from Finder or the Dock inherits launchd's soft limit of 256 open files,
//! while a terminal usually has far more. The download engine runs up to 64 files in parallel with
//! several connections each, plus a socket and a `.part` file per connection, so it needs well
//! over 256. Hitting the limit shows up as `Too many open files` errors that are retried with
//! backoff, which makes downloads crawl.

/// Open files we want available. Comfortably above 64 files x (4 sockets + 1 file) plus the
/// webview, logs and game processes.
pub const WANTED_OPEN_FILES: u64 = 10_240;

/// Raise the soft `RLIMIT_NOFILE` towards [`WANTED_OPEN_FILES`], never above the hard limit and
/// never lowering it. Returns `(before, after)` soft limits.
#[cfg(unix)]
pub fn raise_open_file_limit() -> std::io::Result<(u64, u64)> {
    raise_to(WANTED_OPEN_FILES)
}

/// `rlim_t` is `u64` on 64-bit macOS and Linux but narrower on some other Unix targets, so convert
/// explicitly. The conversion is an identity where it is already `u64`.
#[cfg(unix)]
#[allow(clippy::useless_conversion)]
fn to_u64(v: libc::rlim_t) -> u64 {
    u64::from(v)
}

#[cfg(unix)]
fn raise_to(wanted: u64) -> std::io::Result<(u64, u64)> {
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `lim` is a valid, writable rlimit for the duration of the call.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let before = to_u64(lim.rlim_cur);
    let mut target = wanted.min(to_u64(lim.rlim_max));
    // macOS reports an unlimited hard limit but rejects a soft limit above kern.maxfilesperproc.
    #[cfg(target_os = "macos")]
    if let Some(per_proc) = macos_max_files_per_proc() {
        target = target.min(per_proc);
    }
    if target <= before {
        return Ok((before, before));
    }
    let new = libc::rlimit { rlim_cur: target as libc::rlim_t, rlim_max: lim.rlim_max };
    // SAFETY: `new` is a valid rlimit.
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &new) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((before, target))
}

#[cfg(target_os = "macos")]
fn macos_max_files_per_proc() -> Option<u64> {
    let name = std::ffi::CString::new("kern.maxfilesperproc").ok()?;
    let mut v: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    // SAFETY: `v` and `len` describe a valid writable buffer; `name` is NUL terminated.
    let rc = unsafe { libc::sysctlbyname(name.as_ptr(), &mut v as *mut _ as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    (rc == 0 && v > 0).then_some(v as u64)
}

#[cfg(not(unix))]
pub fn raise_open_file_limit() -> std::io::Result<(u64, u64)> {
    Ok((0, 0))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn soft() -> u64 {
        let mut l = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut l) }, 0);
        to_u64(l.rlim_cur)
    }

    /// Runs in a child process so lowering the limit cannot affect other tests.
    #[test]
    fn a_launchd_style_low_limit_is_raised_and_actually_usable() {
        if std::env::var_os("PCL_LIMITS_CHILD").is_some() {
            let low = libc::rlimit { rlim_cur: 256, rlim_max: libc::RLIM_INFINITY };
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
            assert_eq!(soft(), 256);
            // Reproduce the production failure: 300 simultaneous files do not fit in 256.
            let mut held = Vec::new();
            let mut hit_limit = false;
            for _ in 0..300 {
                match std::fs::File::open("/dev/null") {
                    Ok(f) => held.push(f),
                    Err(e) => {
                        assert_eq!(e.raw_os_error(), Some(libc::EMFILE));
                        hit_limit = true;
                        break;
                    }
                }
            }
            assert!(hit_limit, "with a 256 limit, opening 300 files must fail");
            drop(held);

            let (before, after) = raise_open_file_limit().unwrap();
            assert_eq!(before, 256);
            assert!(after > 256 && after >= 8192.min(WANTED_OPEN_FILES), "raised to {after}");
            assert_eq!(soft(), after);
            let held: Vec<_> = (0..2000).map(|_| std::fs::File::open("/dev/null").expect("open after raise")).collect();
            assert_eq!(held.len(), 2000);
            return;
        }
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args([
                "app::limits::tests::a_launchd_style_low_limit_is_raised_and_actually_usable",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("PCL_LIMITS_CHILD", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "child failed:\n{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn never_lowers_an_already_higher_limit() {
        let now = soft();
        let (before, after) = raise_to(16).unwrap();
        assert_eq!(before, now);
        assert_eq!(after, now);
        assert_eq!(soft(), now);
    }
}
