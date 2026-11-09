//! Persistenza partite: formato JSON (D18), scrittura atomica (tmp + rename),
//! replay da save.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::session::GameSession;

/// Tentativi di nome per i backup creati nello stesso secondo.
const MAX_BACKUP_ATTEMPTS: u32 = 1000;

/// Dati di salvataggio di una partita.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveData {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    /// Assente nei save scritti prima dell'introduzione del campo.
    #[serde(rename = "rulesVersion", default = "legacy_rules_version")]
    pub rules_version: u32,
    #[serde(rename = "gameId")]
    pub game_id: String,
    #[serde(rename = "gameName")]
    pub game_name: String,
    #[serde(rename = "mapId")]
    pub map_id: String,
    #[serde(rename = "rulesetId")]
    pub ruleset_id: String,
    pub seed: u64,
    #[serde(rename = "firstPlayer")]
    pub first_player: usize,
    pub players: Vec<SavePlayer>,
    pub actions: Vec<SaveActionRecord>,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// I save senza `rulesVersion` sono compatibili con le regole della versione 1.
fn legacy_rules_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavePlayer {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveActionRecord {
    pub action: ttr_core::Action,
}

/// Costruisce i dati di salvataggio dalla sessione corrente.
pub(crate) fn build_save_data(session: &GameSession) -> SaveData {
    let now = chrono_now();
    let engine = session.engine.as_ref();

    let seed = engine.map(|e| e.seed()).unwrap_or(0);
    let actions: Vec<SaveActionRecord> = engine
        .map(|e| {
            e.action_log()
                .iter()
                .map(|rec| SaveActionRecord {
                    action: rec.action.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    let status = if let Some(ref e) = session.engine {
        if matches!(e.phase(), ttr_core::error::GamePhase::GameOver) {
            "completed"
        } else {
            "in_progress"
        }
    } else {
        "in_progress"
    };

    // Costruisce i dati giocatore in ordine di indice del motore: `replay_engine`
    // passa `save.players` a `Engine::new` in quest'ordine.
    let players: Vec<SavePlayer> = {
        let mut ordered: Vec<(usize, &String)> = session
            .player_index
            .iter()
            .map(|(id, &idx)| (idx, id))
            .collect();
        ordered.sort_by_key(|(idx, _)| session.players[*idx].index.unwrap_or(*idx));
        ordered
            .into_iter()
            .map(|(idx, id)| {
                let p = &session.players[idx];
                SavePlayer {
                    id: id.clone(),
                    name: p.name.clone(),
                    color: format!("{:?}", p.color).to_uppercase(),
                }
            })
            .collect()
    };

    SaveData {
        format_version: 1,
        rules_version: ttr_core::RULES_VERSION,
        game_id: session.game_id.clone(),
        game_name: session.game_name.clone(),
        map_id: session.ruleset_id.replace("_v2", ""), // "usa_v2" → "usa"
        ruleset_id: session.ruleset_id.clone(),
        seed,
        first_player: 0,
        players,
        actions,
        status: status.to_string(),
        created_at: now.clone(),
        updated_at: now,
    }
}

/// Scrive i dati in modo atomico: scrive su file temporaneo e rinomina.
pub(crate) fn write_atomic(path: &Path, data: &SaveData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| format!("serializzazione: {}", e))?;

    // Assicura che la directory esista
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creazione directory {:?}: {}", parent, e))?;
    }

    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &json).map_err(|e| format!("scrittura tmp: {}", e))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("rename: {}", e))?;

    Ok(())
}

/// Copia il save in `{id}.resume-{unix}.bak` prima di un resume, senza mai
/// sovrascrivere un backup esistente. Ritorna il percorso del backup, o `None`
/// se il save non esiste. `path` deve avere uno stem già validato.
pub(crate) fn backup_before_resume(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut source = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("lettura {:?} per backup: {}", path, e)),
    };

    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for attempt in 0..MAX_BACKUP_ATTEMPTS {
        let extension = if attempt == 0 {
            format!("resume-{}.bak", unix)
        } else {
            format!("resume-{}-{}.bak", unix, attempt)
        };
        let backup_path = path.with_extension(extension);
        let mut backup = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup_path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("creazione backup {:?}: {}", backup_path, e)),
        };
        if let Err(e) = std::io::copy(&mut source, &mut backup) {
            let _ = std::fs::remove_file(&backup_path);
            return Err(format!("copia backup {:?}: {}", backup_path, e));
        }
        return Ok(Some(backup_path));
    }

    Err(format!("troppi backup esistenti per {:?}", path))
}

/// Legge un save file.
pub fn read_save(path: &Path) -> Result<SaveData, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("lettura {:?}: {}", path, e))?;
    let save: SaveData =
        serde_json::from_str(&json).map_err(|e| format!("parsing save {:?}: {}", path, e))?;
    Ok(save)
}

/// Replay: ricostruisce il motore da un save file.
pub fn replay_engine(
    save: &SaveData,
    map: &ttr_core::MapData,
    ruleset: &ttr_core::Ruleset,
) -> Result<ttr_core::Engine, String> {
    validate_save(save, ruleset)?;

    let player_ids: Vec<String> = save.players.iter().map(|p| p.id.clone()).collect();
    let player_names: Vec<String> = save.players.iter().map(|p| p.name.clone()).collect();
    let player_colors: Vec<ttr_proto::PlayerColor> = save
        .players
        .iter()
        .map(|p| parse_color(&p.color))
        .collect::<Result<Vec<_>, _>>()?;

    let mut engine = ttr_core::Engine::new(
        save.seed,
        ruleset.clone(),
        map.clone(),
        player_ids,
        player_names,
        player_colors,
        save.first_player,
    );

    for rec in &save.actions {
        engine
            .apply(rec.action.clone())
            .map_err(|e| format!("replay azione: {}", e))?;
    }

    Ok(engine)
}

/// Rifiuta i save che il motore non può riprodurre o che il resume non può
/// associare ai giocatori (roster indicizzato per nome minuscolo).
fn validate_save(save: &SaveData, ruleset: &ttr_core::Ruleset) -> Result<(), String> {
    if save.format_version != 1 {
        return Err(format!(
            "formato del save non supportato: {}",
            save.format_version
        ));
    }
    if save.rules_version != ttr_core::RULES_VERSION {
        return Err(format!(
            "versione delle regole incompatibile: save {}, server {}",
            save.rules_version,
            ttr_core::RULES_VERSION
        ));
    }

    let count = save.players.len();
    let (min, max) = (ruleset.players.min as usize, ruleset.players.max as usize);
    if !(min..=max).contains(&count) {
        return Err(format!(
            "numero di giocatori non valido: {} (ammessi da {} a {})",
            count, min, max
        ));
    }
    if save.first_player >= count {
        return Err(format!(
            "primo giocatore non valido: {} su {} giocatori",
            save.first_player, count
        ));
    }

    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for player in &save.players {
        if !ids.insert(player.id.as_str()) {
            return Err(format!("id giocatore duplicato: {}", player.id));
        }
        if !names.insert(player.name.to_lowercase()) {
            return Err(format!("nome giocatore duplicato: {}", player.name));
        }
        parse_color(&player.color)?;
    }

    Ok(())
}

pub fn parse_color(s: &str) -> Result<ttr_proto::PlayerColor, String> {
    match s {
        "RED" => Ok(ttr_proto::PlayerColor::Red),
        "BLUE" => Ok(ttr_proto::PlayerColor::Blue),
        "GREEN" => Ok(ttr_proto::PlayerColor::Green),
        "YELLOW" => Ok(ttr_proto::PlayerColor::Yellow),
        "BLACK" => Ok(ttr_proto::PlayerColor::Black),
        _ => Err(format!("colore sconosciuto: {}", s)),
    }
}

fn chrono_now() -> String {
    // ISO 8601 semplice senza dipendenza chrono
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Formato: YYYY-MM-DDTHH:MM:SSZ (approssimato)
    // Usiamo un approccio semplice
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let mins = (time_of_day % 3600) / 60;
    let secs_rem = time_of_day % 60;

    // Calcolo anno/mese/giorno semplificato (approssimazione)
    // Partiamo dal 1970-01-01
    let mut y = 1970i64;
    let mut remaining = days_since_epoch as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0usize;
    while m < 12 && remaining >= month_days[m] as i64 {
        remaining -= month_days[m] as i64;
        m += 1;
    }
    let d = remaining + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        d,
        hours,
        mins,
        secs_rem
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chrono_now_format() {
        let now = chrono_now();
        assert!(now.ends_with("Z"));
        assert!(now.contains("T"));
        assert_eq!(now.len(), 20); // "YYYY-MM-DDTHH:MM:SSZ"
    }

    #[test]
    fn test_save_roundtrip() {
        let data = SaveData {
            format_version: 1,
            rules_version: ttr_core::RULES_VERSION,
            game_id: "g1".into(),
            game_name: "Test".into(),
            map_id: "usa".into(),
            ruleset_id: "usa_v2".into(),
            seed: 42,
            first_player: 0,
            players: vec![SavePlayer {
                id: "p1".into(),
                name: "Gino".into(),
                color: "BLUE".into(),
            }],
            actions: vec![],
            status: "in_progress".into(),
            created_at: "2025-01-01T00:00:00Z".into(),
            updated_at: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: SaveData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.game_id, "g1");
        assert_eq!(back.players[0].name, "Gino");
    }
}
