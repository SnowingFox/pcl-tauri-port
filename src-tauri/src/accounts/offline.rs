use super::{dash, undash, AccountError, Result};
use md5::{Digest, Md5};

/// Minecraft's offline UUID: `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`,
/// i.e. MD5 with version 3 / IETF variant bits. Returned dashed.
pub fn offline_uuid(name: &str) -> String {
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
    dash(&hex)
}

#[allow(dead_code)]
pub fn offline_uuid_undashed(name: &str) -> String {
    undash(&offline_uuid(name))
}

/// Offline names: 1-16 chars of `[A-Za-z0-9_]` (what vanilla servers accept).
pub fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.chars().count() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AccountError::Invalid(
            "offline name must be 1-16 characters of letters, digits or underscore".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        // Computed from the Java algorithm (MD5 + version/variant bits).
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("Steve"), "5627dd98-e6be-3c21-b8a8-e92344183641");
        assert_eq!(offline_uuid("PCL"), "2470d217-3514-3cbe-9416-36e9382fe454");
    }

    #[test]
    fn deterministic_and_versioned() {
        let u = offline_uuid("abc");
        assert_eq!(u, offline_uuid("abc"));
        assert_eq!(&u[14..15], "3");
        assert!(matches!(&u[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn name_validation() {
        assert!(validate_name("Steve_1").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("12345678901234567").is_err());
        assert!(validate_name("bad name").is_err());
    }
}
