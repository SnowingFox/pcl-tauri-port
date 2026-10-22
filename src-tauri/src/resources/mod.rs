#![allow(dead_code)] // library surface consumed by other agents/UI later
//! Resource platforms (Modrinth, CurseForge) and local mod management.
//!
//! Ported in flow from PCL's `Modules/Resource/*` (see LICENCE for attribution).
pub mod commands;
pub mod curseforge;
pub mod hash;
pub mod local_mods;
pub mod modrinth;
pub mod types;

use serde_json::Value;

pub type Result<T> = std::result::Result<T, String>;

pub const USER_AGENT: &str = "SnowingFox/pcl-tauri-port/0.1.0 (+https://github.com/SnowingFox/pcl-tauri-port)";

pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder().user_agent(USER_AGENT).timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())
}

/// Percent-encode a URL query component.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub(crate) async fn send_json(req: reqwest::RequestBuilder) -> Result<Value> {
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {} from {}", status.as_u16(), resp.url()));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

pub(crate) fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}
pub(crate) fn opt_s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}
pub(crate) fn u(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}
pub(crate) fn str_list(v: &Value, k: &str) -> Vec<String> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}
