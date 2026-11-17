//! Session secrets are never logged or written as plaintext. No Google password is read.
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Session {
    cookie_header: String,
    sapisid: String,
}

impl Session {
    pub fn from_cookies(
        cookies: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, String> {
        let mut header = Zeroizing::new(String::new());
        let mut sapisid = Zeroizing::new(String::new());
        let mut has_sid = false;
        for (name, value) in cookies {
            let value = Zeroizing::new(value);
            // Only session cookies required by YouTube Music, from its own cookie manager.
            if !matches!(
                name.as_str(),
                "SAPISID"
                    | "APISID"
                    | "SID"
                    | "HSID"
                    | "SSID"
                    | "SIDCC"
                    | "__Secure-1PAPISID"
                    | "__Secure-3PAPISID"
                    | "__Secure-1PSID"
                    | "__Secure-3PSID"
                    | "__Secure-1PSIDTS"
                    | "__Secure-3PSIDTS"
                    | "__Secure-1PSIDCC"
                    | "__Secure-3PSIDCC"
                    | "LOGIN_INFO"
                    | "VISITOR_INFO1_LIVE"
                    | "PREF"
                    | "YSC"
                    | "SOCS"
                    | "CONSENT"
            ) {
                continue;
            }
            if value.is_empty()
                || !value
                    .bytes()
                    .all(|c| (0x21..=0x7e).contains(&c) && c != b';')
            {
                continue;
            }
            if name == "SAPISID" || (name == "__Secure-3PAPISID" && sapisid.is_empty()) {
                *sapisid = value.to_string();
            }
            has_sid |= matches!(name.as_str(), "SID" | "__Secure-1PSID" | "__Secure-3PSID");
            if !header.is_empty() {
                header.push_str("; ");
            }
            header.push_str(&name);
            header.push('=');
            header.push_str(&value);
        }
        if sapisid.is_empty() || !has_sid || header.len() > 32 * 1024 {
            return Err("Session Google absente ou incomplète.".into());
        }
        Ok(Self {
            cookie_header: header.to_string(),
            sapisid: sapisid.to_string(),
        })
    }

    pub fn cookie_header(&self) -> &str {
        &self.cookie_header
    }
    pub fn sapisid(&self) -> &str {
        &self.sapisid
    }
}

fn store_path() -> Result<std::path::PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA est indisponible.")?;
    Ok(std::path::PathBuf::from(base)
        .join("Tonal")
        .join("google-session.dpapi"))
}

pub fn load() -> Result<Option<Session>, String> {
    let path = store_path()?;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Impossible de lire la session Windows.".into()),
    };
    use std::io::Read;
    let mut encrypted = Vec::new();
    file.take(128 * 1024 + 1)
        .read_to_end(&mut encrypted)
        .map_err(|_| "Impossible de lire la session Windows.")?;
    if encrypted.len() > 128 * 1024 {
        return Err("Fichier de session invalide.".into());
    }
    let plain = Zeroizing::new(unprotect(&encrypted)?);
    serde_json::from_slice(&plain)
        .map(Some)
        .map_err(|_| "Session endommagée : reconnecte-toi.".into())
}

pub fn save(session: &Session) -> Result<(), String> {
    let plain = Zeroizing::new(
        serde_json::to_vec(session).map_err(|_| "Impossible de préparer la session.")?,
    );
    let encrypted = protect(&plain)?;
    let path = store_path()?;
    let parent = path.parent().ok_or("Dossier de session invalide.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Impossible de créer le dossier de session.")?;
    // Atomic replacement; even the temporary file contains ciphertext only.
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Impossible d'enregistrer la session.")?;
    file.write_all(&encrypted)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "Impossible d'enregistrer la session.")?;
    file.persist(&path)
        .map_err(|_| "Impossible de remplacer la session enregistrée.")?;
    Ok(())
}

pub fn delete() -> Result<(), String> {
    match std::fs::remove_file(store_path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Impossible d'effacer la session Windows. Réessaie la déconnexion.".into()),
    }
}

#[cfg(windows)]
fn crypt(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let data = CRYPT_INTEGER_BLOB {
        cbData: input
            .len()
            .try_into()
            .map_err(|_| "Session trop volumineuse.")?,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // SAFETY: all buffers live through the call. DPAPI allocates output with LocalAlloc.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &data,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &data,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(
            "Impossible de protéger ou déchiffrer la session avec ce compte Windows.".into(),
        );
    }
    // SAFETY: successful DPAPI call returned cbData initialized bytes; copy, wipe, then free them.
    let bytes = unsafe {
        let buffer = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let bytes = buffer.to_vec();
        buffer.zeroize();
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(bytes)
}

#[cfg(windows)]
pub(crate) fn protect(input: &[u8]) -> Result<Vec<u8>, String> {
    crypt(input, true)
}
#[cfg(windows)]
pub(crate) fn unprotect(input: &[u8]) -> Result<Vec<u8>, String> {
    crypt(input, false)
}
#[cfg(not(windows))]
pub(crate) fn protect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("Le stockage sécurisé nécessite Windows.".into())
}
#[cfg(not(windows))]
pub(crate) fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("Le stockage sécurisé nécessite Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_requires_both_google_identifiers_and_rejects_header_injection() {
        assert!(Session::from_cookies([("SAPISID".into(), "test".into())]).is_err());
        assert!(Session::from_cookies([
            ("SAPISID".into(), "test\r\nCookie: bad".into()),
            ("SID".into(), "test".into())
        ])
        .is_err());
        let session = Session::from_cookies([
            ("SAPISID".into(), "test".into()),
            ("SID".into(), "test".into()),
            ("unneeded_cookie".into(), "ignored".into()),
        ])
        .unwrap();
        assert!(!session.cookie_header().contains("ignored"));
    }

    #[test]
    #[cfg(windows)]
    fn dpapi_round_trip_and_tampering() {
        let mut encrypted = protect(b"tonal-test-secret").unwrap();
        assert_ne!(encrypted.as_slice(), b"tonal-test-secret");
        assert_eq!(unprotect(&encrypted).unwrap(), b"tonal-test-secret");
        let last = encrypted.len() - 1;
        encrypted[last] ^= 1;
        assert!(unprotect(&encrypted).is_err());
    }
}
