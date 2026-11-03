//! Persistenza partite: formato JSON (D18), scrittura atomica (tmp + rename),
//! replay da save.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::session::GameSession;

/// Dati di salvataggio di una partita.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveData {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
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

    let players: Vec<SavePlayer> = session
        .players
        .iter()
        .map(|p| SavePlayer {
            id: session_player_id(session, p),
            name: p.name.clone(),
            color: format!("{:?}", p.color).to_uppercase(),
        })
        .collect();

    SaveData {
        format_version: 1,
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

fn session_player_id(session: &GameSession, player: &crate::session::SessionPlayer) -> String {
    session
        .player_index
        .iter()
        .find(|&(_, idx)| {
            *idx < session.players.len()
                && std::ptr::eq(&session.players[*idx] as *const _, player as *const _)
        })
        .map(|(id, _)| id.clone())
        .unwrap_or_default()
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
