//! Explicit boot-volume driver selection, separate from model file tools.
use alloc::{string::String, vec::Vec};

pub const MAX_DRIVER_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TOTAL_DRIVER_BYTES: usize = 32 * 1024 * 1024;

pub fn manifest(text: &str) -> Result<Vec<String>, String> {
    let paths: Vec<String> =
        serde_json::from_str(text).map_err(|_| "Invalid DRIVERS.JSON array")?;
    if paths.len() > 32 {
        return Err("DRIVERS.JSON exceeds 32 drivers".into());
    }
    for (index, path) in paths.iter().enumerate() {
        let lower = path.to_ascii_lowercase();
        if !lower.starts_with("\\efi\\agent\\drivers\\")
            || !lower.ends_with(".efi")
            || path.contains(['/', ':'])
            || path.chars().any(char::is_control)
            || path
                .split('\\')
                .skip(1)
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err("Drivers must use absolute paths under \\EFI\\AGENT\\DRIVERS".into());
        }
        if paths[..index]
            .iter()
            .any(|previous| previous.eq_ignore_ascii_case(path))
        {
            return Err("DRIVERS.JSON contains a duplicate path".into());
        }
    }
    Ok(paths)
}

/// Restrict startup to x64 PE32+ EFI drivers; never execute an application
/// disguised as a driver. LoadImage remains responsible for full validation.
pub fn validate_image(bytes: &[u8]) -> Result<(), &'static str> {
    let invalid = "Driver must be an x64 PE32+ EFI boot-service or runtime driver";
    let word = |offset: usize| {
        bytes
            .get(offset..offset.checked_add(2)?)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    if bytes.len() > MAX_DRIVER_BYTES || bytes.get(..2) != Some(b"MZ") {
        return Err(invalid);
    }
    let offset = bytes.get(0x3c..0x40).ok_or(invalid)?;
    let pe = u32::from_le_bytes(offset.try_into().expect("Four bytes")) as usize;
    let optional = pe.checked_add(24).ok_or(invalid)?;
    let signature_end = pe.checked_add(4).ok_or(invalid)?;
    let header_size = word(pe.checked_add(20).ok_or(invalid)?).ok_or(invalid)? as usize;
    if bytes.get(pe..signature_end) != Some(b"PE\0\0")
        || word(signature_end) != Some(0x8664)
        || word(optional) != Some(0x20b)
        || header_size < 70
        || optional
            .checked_add(header_size)
            .is_none_or(|end| end > bytes.len())
        || !matches!(
            word(optional.checked_add(68).ok_or(invalid)?),
            Some(11 | 12)
        )
    {
        return Err(invalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_rejects_cross_volume_paths_and_case_aliases() {
        assert!(manifest(r#"["\\EFI\\AGENT\\DRIVERS\\Net.efi"]"#).is_ok());
        for text in [
            r#"["fs0:\\EFI\\AGENT\\DRIVERS\\Net.efi"]"#,
            r#"["\\EFI\\AGENT\\DRIVERS\\..\\AGENT.EFI"]"#,
            r#"["\\EFI\\AGENT\\DRIVERS\\Net.efi","\\efi\\agent\\drivers\\net.EFI"]"#,
            r#"["\\EFI\\AGENT\\DRIVERS\\\\Net.efi"]"#,
        ] {
            assert!(manifest(text).is_err(), "{text}");
        }
    }

    #[test]
    fn only_efi_driver_subsystems_are_accepted() {
        let mut bytes = [0u8; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&128u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[148..150].copy_from_slice(&240u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
        for subsystem in 0u16..16 {
            bytes[220..222].copy_from_slice(&subsystem.to_le_bytes());
            assert_eq!(validate_image(&bytes).is_ok(), matches!(subsystem, 11 | 12));
        }
        bytes[220..222].copy_from_slice(&11u16.to_le_bytes());
        for length in 0..bytes.len() {
            assert_eq!(validate_image(&bytes[..length]).is_ok(), length >= 392);
        }
        bytes[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate_image(&bytes).is_err());
    }
}
