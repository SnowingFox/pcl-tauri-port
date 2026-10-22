//! Download source selection and URL rewriting (official Mojang vs BMCLAPI).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Official,
    Bmcl,
    /// Official URL first, the BMCLAPI mirror only as fallback.
    #[serde(rename = "officialfirst")]
    OfficialFirst,
}

const BMCL: &str = "https://bmclapi2.bangbang93.com";

/// (official prefix, mirror prefix). Longest/most specific prefixes first.
const RULES: &[(&str, &str)] = &[
    ("https://piston-meta.mojang.com", BMCL),
    ("https://piston-data.mojang.com", BMCL),
    ("https://launchermeta.mojang.com", BMCL),
    ("https://launcher.mojang.com", BMCL),
    ("https://resources.download.minecraft.net", "https://bmclapi2.bangbang93.com/assets"),
    ("https://libraries.minecraft.net", "https://bmclapi2.bangbang93.com/maven"),
    ("https://maven.minecraftforge.net", "https://bmclapi2.bangbang93.com/maven"),
    ("https://files.minecraftforge.net/maven", "https://bmclapi2.bangbang93.com/maven"),
    ("https://maven.neoforged.net/releases", "https://bmclapi2.bangbang93.com/maven"),
    ("https://maven.fabricmc.net", "https://bmclapi2.bangbang93.com/maven"),
    ("https://meta.fabricmc.net", "https://bmclapi2.bangbang93.com/fabric-meta"),
    ("https://maven.quiltmc.org/repository/release", "https://bmclapi2.bangbang93.com/maven"),
    ("https://meta.quiltmc.org", "https://bmclapi2.bangbang93.com/quilt-meta"),
];

/// Rewrite an official URL to its BMCLAPI equivalent, if a rule exists.
pub fn to_mirror(url: &str) -> Option<String> {
    RULES
        .iter()
        .find(|(from, _)| url.starts_with(from) && matches!(url.as_bytes().get(from.len()), None | Some(b'/') | Some(b'?')))
        .map(|(from, to)| format!("{}{}", to, &url[from.len()..]))
}

/// Ordered list of URLs to try for `url` under `source`. The other origin is
/// always kept as a fallback.
pub fn candidates(url: &str, source: Source) -> Vec<String> {
    match (source, to_mirror(url)) {
        (Source::Bmcl, Some(m)) => vec![m, url.to_string()],
        (Source::OfficialFirst, Some(m)) => vec![url.to_string(), m],
        _ => vec![url.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_known_hosts() {
        assert_eq!(
            to_mirror("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap(),
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        );
        assert_eq!(
            to_mirror("https://resources.download.minecraft.net/ab/abcdef").unwrap(),
            "https://bmclapi2.bangbang93.com/assets/ab/abcdef"
        );
        assert_eq!(to_mirror("https://libraries.minecraft.net/a/b/c.jar").unwrap(), "https://bmclapi2.bangbang93.com/maven/a/b/c.jar");
        assert_eq!(
            to_mirror("https://maven.neoforged.net/releases/net/neoforged/x.jar").unwrap(),
            "https://bmclapi2.bangbang93.com/maven/net/neoforged/x.jar"
        );
        assert_eq!(
            to_mirror("https://meta.fabricmc.net/v2/versions/loader").unwrap(),
            "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader"
        );
    }

    #[test]
    fn leaves_unknown_and_lookalikes() {
        assert!(to_mirror("https://example.com/x").is_none());
        assert!(to_mirror("https://libraries.minecraft.net.evil.com/x").is_none());
    }

    #[test]
    fn candidate_order() {
        let u = "https://libraries.minecraft.net/a.jar";
        assert_eq!(candidates(u, Source::Official), vec![u.to_string()]);
        let c = candidates(u, Source::Bmcl);
        assert_eq!(c.len(), 2);
        assert!(c[0].starts_with("https://bmclapi2"));
        assert_eq!(c[1], u);
        assert_eq!(candidates("https://x.org/a", Source::Bmcl).len(), 1);
        let o = candidates(u, Source::OfficialFirst);
        assert_eq!(o[0], u);
        assert!(o[1].starts_with("https://bmclapi2"));
    }
}
