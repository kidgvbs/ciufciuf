//! Profili giocatore: nomi salvati una volta e riusati in ogni partita.
//!
//! Un profilo è solo un nome (il colore si sceglie a ogni partita). I profili
//! vivono in `saves/profiles.json` come array JSON di stringhe; due nomi che
//! differiscono solo per maiuscole sono lo stesso profilo e vince il primo.

use std::path::Path;

/// Nome del file dei profili dentro `saves/`.
pub const PROFILES_FILE: &str = "profiles.json";

/// Legge i profili, ordinati alfabeticamente senza distinzione di maiuscole.
/// Un file assente o illeggibile vale come elenco vuoto.
pub fn read_profiles(saves_dir: &Path) -> Vec<String> {
    let mut profiles: Vec<String> = std::fs::read_to_string(saves_dir.join(PROFILES_FILE))
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    profiles.sort_by_key(|p| p.to_lowercase());
    profiles
}

/// Aggiunge un profilo se non esiste già (confronto senza maiuscole).
/// Ritorna `true` se è stato creato. Scrittura atomica (tmp + rename).
/// Il chiamante deve serializzare le chiamate (vedi `AppState::profiles_lock`).
pub fn add_profile(saves_dir: &Path, name: &str) -> Result<bool, String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(false);
    }
    let mut profiles = read_profiles(saves_dir);
    let key = name.to_lowercase();
    if profiles.iter().any(|p| p.to_lowercase() == key) {
        return Ok(false);
    }
    profiles.push(name.to_string());
    profiles.sort_by_key(|p| p.to_lowercase());

    std::fs::create_dir_all(saves_dir).map_err(|e| format!("mkdir {:?}: {}", saves_dir, e))?;
    let path = saves_dir.join(PROFILES_FILE);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| format!("scrittura {:?}: {}", tmp, e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {:?}: {}", path, e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_saved_once_case_insensitive() {
        let dir = std::env::temp_dir().join(format!("ciuf-profiles-{}", uuid::Uuid::new_v4()));
        assert!(read_profiles(&dir).is_empty(), "nessun file = nessun profilo");

        assert!(add_profile(&dir, " Bob ").unwrap());
        assert!(add_profile(&dir, "anna").unwrap());
        assert!(!add_profile(&dir, "BOB").unwrap(), "stesso profilo con altre maiuscole");
        assert!(!add_profile(&dir, "   ").unwrap(), "nome vuoto ignorato");

        assert_eq!(read_profiles(&dir), ["anna", "Bob"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
