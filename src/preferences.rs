//! Only interface preferences; authentication remains in the separate DPAPI store.
use serde::{Deserialize, Serialize};
use std::{io::Read, path::PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub show_quality: bool,
    pub close_to_tray: bool,
    pub smart_shuffle: bool,
    pub normalize_volume: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_quality: true,
            close_to_tray: false,
            smart_shuffle: false,
            normalize_volume: false,
        }
    }
}
impl Preferences {
    fn path() -> Option<PathBuf> {
        Some(
            PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
                .join("Tonal")
                .join("preferences.json"),
        )
    }
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(file) = std::fs::File::open(path) else {
            return Self::default();
        };
        let mut bytes = Vec::new();
        if file.take(4097).read_to_end(&mut bytes).is_err() || bytes.len() > 4096 {
            return Self::default();
        }
        serde_json::from_slice(&bytes).unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Dossier des préférences indisponible.")?;
        let parent = path
            .parent()
            .ok_or("Dossier des préférences indisponible.")?;
        std::fs::create_dir_all(parent)
            .map_err(|_| "Impossible de créer le dossier des préférences.")?;
        let mut file = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| "Impossible d'enregistrer les préférences.")?;
        serde_json::to_writer(file.as_file_mut(), self)
            .map_err(|_| "Impossible d'enregistrer les préférences.")?;
        file.persist(path)
            .map_err(|_| "Impossible d'enregistrer les préférences.")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_playback_options_default_off_and_round_trip_with_existing_preferences() {
        let mut preferences: Preferences =
            serde_json::from_str(r#"{"show_quality":false,"close_to_tray":true}"#).unwrap();
        assert!(!preferences.smart_shuffle && !preferences.normalize_volume);
        preferences.smart_shuffle = true;
        preferences.normalize_volume = true;
        let restored: Preferences =
            serde_json::from_slice(&serde_json::to_vec(&preferences).unwrap()).unwrap();
        assert!(restored.smart_shuffle && restored.normalize_volume && restored.close_to_tray);
        assert!(!restored.show_quality);
    }
}
