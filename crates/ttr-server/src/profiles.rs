//! Profili giocatore: nomi salvati una volta e riusati in ogni partita.
//!
//! Un profilo è solo un nome (il colore si sceglie a ogni partita). I profili
//! vivono in `saves/profiles.json` come array JSON di stringhe; due nomi che
//! differiscono solo per maiuscole o forma di normalizzazione Unicode sono lo
//! stesso profilo e vince il primo.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::names::name_key;
use crate::save::durable_write;

/// Nome del file dei profili dentro `saves/`.
pub const PROFILES_FILE: &str = "profiles.json";

/// Numero massimo di profili: oltre, i nuovi nomi non vengono salvati.
pub const MAX_PROFILES: usize = 200;

/// Tentativi di nome per la quarantena di un file corrotto.
const MAX_QUARANTINE_ATTEMPTS: u32 = 1000;

/// Errore di lettura, interpretazione o scrittura dei profili.
#[derive(Debug, thiserror::Error)]
pub enum ProfilesError {
    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("profili non interpretabili in {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl ProfilesError {
    fn io(what: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            what,
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Legge i profili, ordinati alfabeticamente senza distinzione di maiuscole.
/// Un file assente è un elenco vuoto; un file illeggibile o non interpretabile
/// è un errore.
pub fn read_profiles(saves_dir: &Path) -> Result<Vec<String>, ProfilesError> {
    let path = saves_dir.join(PROFILES_FILE);
    let json = match std::fs::read_to_string(&path) {
        Ok(json) => json,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ProfilesError::io("lettura", &path, e)),
    };
    let mut profiles: Vec<String> =
        serde_json::from_str(&json).map_err(|source| ProfilesError::Parse { path, source })?;
    profiles.sort_by_key(|p| name_key(p));
    Ok(profiles)
}

/// Sposta il file corrotto in `profiles.json.corrupt-<unix>` (con suffisso
/// `-N` se il nome è occupato) senza sovrascrivere nulla.
fn quarantine(path: &Path) -> Result<PathBuf, ProfilesError> {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for attempt in 0..MAX_QUARANTINE_ATTEMPTS {
        let suffix = if attempt == 0 {
            format!("corrupt-{unix}")
        } else {
            format!("corrupt-{unix}-{attempt}")
        };
        let dest = path.with_file_name(format!("{PROFILES_FILE}.{suffix}"));
        // `create_new` riserva il nome di destinazione; il rename lo sostituisce.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(_) => {
                std::fs::rename(path, &dest)
                    .map_err(|e| ProfilesError::io("quarantena di", path, e))?;
                return Ok(dest);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(ProfilesError::io("creazione", &dest, e)),
        }
    }
    Err(ProfilesError::io(
        "troppi file in quarantena per",
        path,
        std::io::Error::new(ErrorKind::AlreadyExists, "nomi di quarantena esauriti"),
    ))
}

/// Aggiunge un profilo se non esiste già (confronto senza maiuscole).
/// Ritorna `true` se è stato creato; `false` se esiste già o se
/// `MAX_PROFILES` è raggiunto. Un file illeggibile viene messo in
/// quarantena e ricreato da zero; non viene mai sovrascritto.
/// Il chiamante deve serializzare le chiamate (vedi `AppState::profiles_lock`).
pub fn add_profile(saves_dir: &Path, name: &str) -> Result<bool, ProfilesError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(false);
    }
    let path = saves_dir.join(PROFILES_FILE);
    let mut profiles = match read_profiles(saves_dir) {
        Ok(profiles) => profiles,
        Err(e) => {
            let moved = quarantine(&path)?;
            tracing::error!(
                error = %e,
                quarantined = %moved.display(),
                "profili illeggibili: file spostato in quarantena"
            );
            Vec::new()
        }
    };
    let key = name_key(name);
    if profiles.iter().any(|p| name_key(p) == key) {
        return Ok(false);
    }
    if profiles.len() >= MAX_PROFILES {
        tracing::warn!(
            limit = MAX_PROFILES,
            "limite di profili raggiunto: nome non salvato"
        );
        return Ok(false);
    }
    profiles.push(name.to_string());
    profiles.sort_by_key(|p| name_key(p));

    std::fs::create_dir_all(saves_dir)
        .map_err(|e| ProfilesError::io("creazione directory", saves_dir, e))?;
    let json = serde_json::to_string_pretty(&profiles).map_err(|source| ProfilesError::Parse {
        path: path.clone(),
        source,
    })?;
    durable_write(&path, json.as_bytes()).map_err(|e| ProfilesError::io("scrittura", &path, e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corrupt_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("profiles.json.corrupt-")
            })
            .collect()
    }

    #[test]
    fn profiles_are_saved_once_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        assert!(
            read_profiles(dir).unwrap().is_empty(),
            "nessun file = nessun profilo"
        );

        assert!(add_profile(dir, " Bob ").unwrap());
        assert!(add_profile(dir, "anna").unwrap());
        assert!(
            !add_profile(dir, "BOB").unwrap(),
            "stesso profilo con altre maiuscole"
        );
        assert!(!add_profile(dir, "   ").unwrap(), "nome vuoto ignorato");

        assert_eq!(read_profiles(dir).unwrap(), ["anna", "Bob"]);
    }

    #[test]
    fn corrupt_file_is_quarantined_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        for n in ["Anna", "Bob", "Ciro"] {
            add_profile(dir, n).unwrap();
        }
        let path = dir.join(PROFILES_FILE);
        let original = std::fs::read_to_string(&path).unwrap();
        let truncated = &original[..original.len() - 3];
        std::fs::write(&path, truncated).unwrap();

        assert!(matches!(
            read_profiles(dir),
            Err(ProfilesError::Parse { .. })
        ));
        assert!(add_profile(dir, "Dario").unwrap());

        let quarantined = corrupt_files(dir);
        assert_eq!(
            quarantined.len(),
            1,
            "un file in quarantena: {quarantined:?}"
        );
        assert_eq!(std::fs::read_to_string(&quarantined[0]).unwrap(), truncated);
        assert_eq!(read_profiles(dir).unwrap(), ["Dario"]);
    }

    #[test]
    fn repeated_corruption_never_overwrites_a_quarantined_file() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let path = dir.join(PROFILES_FILE);
        for round in 0..3 {
            std::fs::write(&path, format!("[\"rotto{round}")).unwrap();
            add_profile(dir, "Dario").unwrap();
        }
        let mut contents: Vec<String> = corrupt_files(dir)
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        contents.sort();
        assert_eq!(contents, ["[\"rotto0", "[\"rotto1", "[\"rotto2"]);
    }

    #[test]
    fn profiles_capped() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        for i in 0..MAX_PROFILES {
            assert!(add_profile(dir, &format!("p{i}")).unwrap());
        }
        assert!(!add_profile(dir, "uno-di-troppo").unwrap());
        let profiles = read_profiles(dir).unwrap();
        assert_eq!(profiles.len(), MAX_PROFILES);
        assert!(!profiles.contains(&"uno-di-troppo".to_string()));
    }

    #[test]
    fn names_equal_after_nfc_are_the_same_profile() {
        let dir = tempfile::tempdir().unwrap();
        assert!(add_profile(dir.path(), "\u{e9}").unwrap());
        assert!(!add_profile(dir.path(), "e\u{301}").unwrap());
    }
}
