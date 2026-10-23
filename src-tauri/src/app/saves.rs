//! World listing for `app_instance_saves`.

use base64::Engine as _;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ICON_BYTES: u64 = 256 * 1024;
const MAX_LEVEL_DAT: u64 = 16 * 1024 * 1024;
const MAX_WALK_ENTRIES: usize = 200_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SaveEntry {
    /// Folder name inside `saves/` (use it as `worldName` for datapacks).
    pub folder: String,
    /// Level name from `level.dat`, or the folder name.
    pub name: String,
    pub size_bytes: u64,
    /// Unix epoch milliseconds.
    pub last_played_ms: Option<i64>,
    pub icon_data_url: Option<String>,
    pub path: String,
}

// ---- minimal NBT reader: only what `level.dat` needs (Data.LevelName / Data.LastPlayed) ----

#[derive(Debug)]
enum Nbt {
    Compound(Vec<(String, Nbt)>),
    Str(String),
    Long(i64),
    Other,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.p.checked_add(n)?;
        let s = self.b.get(self.p..end)?;
        self.p = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn i16(&mut self) -> Option<i16> {
        self.take(2).map(|s| i16::from_be_bytes([s[0], s[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4).map(|s| i32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8).map(|s| i64::from_be_bytes(s.try_into().unwrap()))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.i16()? as u16 as usize;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn payload(&mut self, ty: u8, depth: u32) -> Option<Nbt> {
        if depth > 24 {
            return None;
        }
        Some(match ty {
            1 => {
                self.take(1)?;
                Nbt::Other
            }
            2 => {
                self.take(2)?;
                Nbt::Other
            }
            3 | 5 => {
                self.take(4)?;
                Nbt::Other
            }
            4 => Nbt::Long(self.i64()?),
            6 => {
                self.take(8)?;
                Nbt::Other
            }
            7 => {
                let n = usize::try_from(self.i32()?).ok()?;
                self.take(n)?;
                Nbt::Other
            }
            8 => Nbt::Str(self.string()?),
            9 => {
                let et = self.u8()?;
                let n = usize::try_from(self.i32()?).ok()?;
                for _ in 0..n {
                    self.payload(et, depth + 1)?;
                }
                Nbt::Other
            }
            10 => {
                let mut items = Vec::new();
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        break;
                    }
                    let name = self.string()?;
                    items.push((name, self.payload(t, depth + 1)?));
                }
                Nbt::Compound(items)
            }
            11 => {
                let n = usize::try_from(self.i32()?).ok()?.checked_mul(4)?;
                self.take(n)?;
                Nbt::Other
            }
            12 => {
                let n = usize::try_from(self.i32()?).ok()?.checked_mul(8)?;
                self.take(n)?;
                Nbt::Other
            }
            _ => return None,
        })
    }
}

fn get<'a>(c: &'a Nbt, key: &str) -> Option<&'a Nbt> {
    match c {
        Nbt::Compound(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

/// `(level name, last played in epoch ms)` from the decompressed bytes of a `level.dat`.
pub fn parse_level_dat(bytes: &[u8]) -> Option<(Option<String>, Option<i64>)> {
    let mut r = Reader { b: bytes, p: 0 };
    if r.u8()? != 10 {
        return None;
    }
    r.string()?; // root name, usually empty
    let root = r.payload(10, 0)?;
    let data = get(&root, "Data")?;
    let name = match get(data, "LevelName") {
        Some(Nbt::Str(s)) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    };
    let played = match get(data, "LastPlayed") {
        Some(Nbt::Long(l)) if *l > 0 => Some(*l),
        _ => None,
    };
    Some((name, played))
}

fn read_level_dat(path: &Path) -> Option<(Option<String>, Option<i64>)> {
    let f = std::fs::File::open(path).ok()?;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(f).take(MAX_LEVEL_DAT).read_to_end(&mut raw).ok()?;
    parse_level_dat(&raw)
}

fn dir_size(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut seen = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > MAX_WALK_ENTRIES {
                return total;
            }
            let Ok(ty) = e.file_type() else { continue };
            if ty.is_dir() {
                stack.push(e.path());
            } else if ty.is_file() {
                total += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

fn mtime_ms(p: &Path) -> Option<i64> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn icon_data_url(world: &Path) -> Option<String> {
    let p = world.join("icon.png");
    let meta = std::fs::metadata(&p).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(&p).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Worlds under `<game_dir>/saves`: directories that contain a `level.dat`. Newest first.
pub fn list_saves(game_dir: &Path) -> Vec<SaveEntry> {
    let saves = game_dir.join("saves");
    let Ok(rd) = std::fs::read_dir(&saves) else { return vec![] };
    let mut out: Vec<SaveEntry> = rd
        .flatten()
        .filter_map(|e| {
            let path: PathBuf = e.path();
            let level = path.join("level.dat");
            if !e.file_type().ok()?.is_dir() || !level.is_file() {
                return None;
            }
            let folder = e.file_name().to_string_lossy().into_owned();
            let (name, played) = read_level_dat(&level).unwrap_or((None, None));
            Some(SaveEntry {
                name: name.unwrap_or_else(|| folder.clone()),
                folder,
                size_bytes: dir_size(&path),
                last_played_ms: played.or_else(|| mtime_ms(&level)),
                icon_data_url: icon_data_url(&path),
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.last_played_ms.cmp(&a.last_played_ms).then_with(|| a.folder.to_lowercase().cmp(&b.folder.to_lowercase())));
    out
}

#[cfg(test)]
pub(crate) fn build_level_dat(name: &str, last_played: i64) -> Vec<u8> {
    use std::io::Write;
    let mut raw: Vec<u8> = vec![10, 0, 0]; // root compound, empty name
    raw.extend([10, 0, 4]);
    raw.extend(b"Data");
    // a list of two ints and an int array that the reader has to skip correctly
    raw.extend([9, 0, 4]);
    raw.extend(b"junk");
    raw.extend([3, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2]);
    raw.extend([11, 0, 3]);
    raw.extend(b"arr");
    raw.extend([0, 0, 0, 1, 0, 0, 0, 7]);
    raw.extend([8, 0, 9]);
    raw.extend(b"LevelName");
    raw.extend((name.len() as u16).to_be_bytes());
    raw.extend(name.as_bytes());
    raw.extend([4, 0, 10]);
    raw.extend(b"LastPlayed");
    raw.extend(last_played.to_be_bytes());
    raw.extend([0, 0]); // end Data, end root
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_worlds_with_names_icons_and_order() {
        let d = tempfile::tempdir().unwrap();
        let saves = d.path().join("saves");
        for (folder, name, t) in [("w1", "Old World", 1_000i64), ("w2", "新世界", 5_000), ("broken", "", 0)] {
            let w = saves.join(folder);
            std::fs::create_dir_all(w.join("region")).unwrap();
            if folder == "broken" {
                std::fs::write(w.join("level.dat"), b"not gzip").unwrap();
            } else {
                std::fs::write(w.join("level.dat"), build_level_dat(name, t)).unwrap();
            }
            std::fs::write(w.join("region/r.0.0.mca"), vec![0u8; 100]).unwrap();
        }
        std::fs::write(saves.join("w2/icon.png"), [0x89, b'P', b'N', b'G']).unwrap();
        std::fs::write(saves.join("w1/icon.png"), vec![0u8; (MAX_ICON_BYTES + 1) as usize]).unwrap();
        std::fs::create_dir_all(saves.join("not-a-world")).unwrap();
        std::fs::write(saves.join("stray.txt"), "x").unwrap();

        let list = list_saves(d.path());
        assert_eq!(list.len(), 3, "only directories with a level.dat are worlds");
        let pos = |f: &str| list.iter().position(|s| s.folder == f).unwrap();
        assert!(pos("w2") < pos("w1"), "newest first");
        let w2 = &list[pos("w2")];
        assert_eq!(w2.name, "新世界");
        assert_eq!(w2.last_played_ms, Some(5_000));
        assert!(w2.icon_data_url.as_deref().unwrap().starts_with("data:image/png;base64,iVBORw"));
        assert!(w2.size_bytes >= 100);
        let w1 = list.iter().find(|s| s.folder == "w1").unwrap();
        assert_eq!(w1.name, "Old World");
        assert!(w1.icon_data_url.is_none(), "oversized icons are omitted");
        let broken = list.iter().find(|s| s.folder == "broken").unwrap();
        assert_eq!(broken.name, "broken", "falls back to the folder name");
        assert!(broken.last_played_ms.is_some(), "falls back to the file mtime");
        assert!(list_saves(&d.path().join("missing")).is_empty());
    }

    #[test]
    fn nbt_reader_rejects_garbage_without_panicking() {
        assert!(parse_level_dat(&[]).is_none());
        assert!(parse_level_dat(&[10, 0, 0, 10, 0, 4, b'D', b'a', b't', b'a', 9, 0, 1, b'x', 10, 0x7f, 0xff, 0xff, 0xff]).is_none());
        assert!(parse_level_dat(&[3, 0, 0]).is_none());
    }
}
