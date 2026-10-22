use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::Path;

pub fn sha1_hex(data: &[u8]) -> String {
    hex::encode(Sha1::digest(data))
}

pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// MurmurHash2 (32-bit), as used by CurseForge fingerprints (seed 1).
pub fn murmur2(data: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let mut h: u32 = seed ^ (data.len() as u32);
    let (chunks, t) = data.as_chunks::<4>();
    for c in chunks {
        let mut k = u32::from_le_bytes(*c);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    if t.len() >= 3 {
        h ^= (t[2] as u32) << 16;
    }
    if t.len() >= 2 {
        h ^= (t[1] as u32) << 8;
    }
    if !t.is_empty() {
        h ^= t[0] as u32;
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^ (h >> 15)
}

/// CurseForge fingerprint: murmur2 over the file with whitespace bytes (9,10,13,32) removed.
pub fn curseforge_fingerprint(data: &[u8]) -> u32 {
    let filtered: Vec<u8> = data
        .iter()
        .copied()
        .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
        .collect();
    murmur2(&filtered, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha1_known() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
    #[test]
    fn murmur_empty_seed1() {
        assert_eq!(murmur2(b"", 1), 0x5bd1_5e36);
    }
    #[test]
    fn fingerprint_ignores_whitespace() {
        assert_eq!(curseforge_fingerprint(b"a b\r\nc"), curseforge_fingerprint(b"abc"));
    }

    /// The previous `chunks_exact(4)` implementation, kept to prove the `as_chunks` refactor
    /// (needed for clippy) did not change any fingerprint.
    fn murmur2_reference(data: &[u8], seed: u32) -> u32 {
        const M: u32 = 0x5bd1_e995;
        let mut h: u32 = seed ^ (data.len() as u32);
        let mut chunks = data.chunks_exact(4);
        for c in &mut chunks {
            let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
            k = k.wrapping_mul(M);
            k ^= k >> 24;
            k = k.wrapping_mul(M);
            h = h.wrapping_mul(M) ^ k;
        }
        let t = chunks.remainder();
        if t.len() >= 3 {
            h ^= (t[2] as u32) << 16;
        }
        if t.len() >= 2 {
            h ^= (t[1] as u32) << 8;
        }
        if !t.is_empty() {
            h ^= t[0] as u32;
            h = h.wrapping_mul(M);
        }
        h ^= h >> 13;
        h = h.wrapping_mul(M);
        h ^ (h >> 15)
    }

    #[test]
    fn murmur2_matches_reference_for_all_tail_lengths() {
        let data: Vec<u8> = (0..4099u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        for len in (0..64).chain([255, 256, 257, 1023, 1024, 1025, 4095, 4096, 4097, 4099]) {
            for seed in [0u32, 1, 0xdead_beef] {
                assert_eq!(murmur2(&data[..len], seed), murmur2_reference(&data[..len], seed), "len {len} seed {seed}");
            }
        }
    }
}
