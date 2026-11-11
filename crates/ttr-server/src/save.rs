//! Persistenza partite: formato JSON, scrittura durevole (tmp + fsync + rename),
//! archivio dei risultati, replay da save.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use subtle::ConstantTimeEq;

use crate::names::name_key;
use crate::session::{GameSession, MAX_PLAYER_NAME_CHARS};
use crate::state::AppState;

/// Versione del formato di scrittura del save; `validate_save` accetta anche la 1.
pub const FORMAT_VERSION: u32 = 2;

/// Età oltre la quale un file `*.tmp` lasciato da un crash viene eliminato.
const STALE_TMP_AGE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Tentativi di nome per i backup creati nello stesso secondo.
const MAX_BACKUP_ATTEMPTS: u32 = 1000;

/// Backup `{id}.resume-*.bak` conservati per partita: i più vecchi vengono eliminati.
const MAX_RESUME_BACKUPS: usize = 5;

/// Dimensione massima di un file di save letto dal server.
pub(crate) const MAX_SAVE_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// File con i risultati delle partite concluse, una riga JSON per partita.
pub const RESULTS_FILE: &str = "results.jsonl";

/// Risultato di una partita conclusa: tutto ciò che resta dopo che il save
/// della partita è stato eliminato.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameResult {
    pub game_id: String,
    pub game_name: String,
    pub ruleset_id: String,
    pub finished_at: String,
    pub players: Vec<ResultPlayer>,
}

impl GameResult {
    /// Costruisce il risultato da `(nome, totale, vincitore)` per giocatore;
    /// lo usano sia la sessione sia l'archiviazione all'avvio.
    pub fn from_scores(
        game_id: &str,
        game_name: &str,
        ruleset_id: &str,
        finished_at: String,
        players: impl IntoIterator<Item = (String, i32, bool)>,
    ) -> Self {
        Self {
            game_id: game_id.to_string(),
            game_name: game_name.to_string(),
            ruleset_id: ruleset_id.to_string(),
            finished_at,
            players: players
                .into_iter()
                .map(|(name, total, winner)| ResultPlayer {
                    name,
                    total,
                    winner,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultPlayer {
    pub name: String,
    pub total: i32,
    pub winner: bool,
}

/// Riga della classifica generale, aggregata per nome giocatore.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StandingRow {
    pub name: String,
    pub games: u32,
    pub wins: u32,
    pub total_points: i64,
    pub best: i32,
}

/// Istante corrente in RFC 3339 UTC, a precisione di secondo.
pub fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    let now = time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

// ── Scrittura durevole ──────────────────────────────────────────────

/// Sincronizza su disco la directory, così un `rename` o una creazione
/// sopravvivono a un crash. Non fa nulla fuori da unix.
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
    Ok(())
}

/// Scrive `bytes` in `path` in modo atomico e durevole: file temporaneo
/// univoco `{nome}.{pid}.{n}.tmp`, `sync_all`, `rename`, `sync_all` della
/// directory. Il chiamante garantisce che la directory esista.
pub(crate) fn durable_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "percorso senza nome di file")
    })?;
    let tmp = parent.join(format!(
        "{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    sync_dir(parent)
}

// ── Archivio risultati ──────────────────────────────────────────────

/// Aggiunge un risultato in coda a `results.jsonl` (crea il file se manca).
///
/// Sotto `lock` scrive la riga con un solo `write_all`; se il file non
/// termina con `\n` (riga parziale di un crash) ne antepone uno. Un `gameId`
/// già presente non viene aggiunto: ritorna `Ok(false)`. Un errore di
/// `sync_dir` dopo l'append riuscito è solo un avviso.
pub(crate) fn append_result(
    saves_dir: &Path,
    lock: &Mutex<()>,
    result: &GameResult,
) -> Result<bool, String> {
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(saves_dir).map_err(|e| format!("mkdir {:?}: {}", saves_dir, e))?;
    let path = saves_dir.join(RESULTS_FILE);

    let existing = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("lettura {:?}: {}", path, e)),
    };
    if parse_results(&existing)
        .iter()
        .any(|r| r.game_id == result.game_id)
    {
        return Ok(false);
    }

    let mut buf = Vec::new();
    if !existing.is_empty() && !existing.ends_with(b"\n") {
        buf.push(b'\n');
    }
    serde_json::to_writer(&mut buf, result).map_err(|e| format!("serializzazione: {}", e))?;
    buf.push(b'\n');

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("apertura {:?}: {}", path, e))?;
    file.write_all(&buf)
        .map_err(|e| format!("scrittura {:?}: {}", path, e))?;
    file.sync_all()
        .map_err(|e| format!("sync {:?}: {}", path, e))?;
    if existing.is_empty()
        && let Err(e) = sync_dir(saves_dir)
    {
        tracing::warn!(dir = %saves_dir.display(), error = %e, "sync della directory dopo l'append fallita");
    }
    Ok(true)
}

/// Interpreta il contenuto di `results.jsonl`; le righe illeggibili vengono
/// scartate con un `warn!` che riporta il numero di riga.
fn parse_results(bytes: &[u8]) -> Vec<GameResult> {
    let text = String::from_utf8_lossy(bytes);
    let mut results = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::warn!(line = idx + 1, error = %e, "riga di results.jsonl scartata");
            }
        }
    }
    results
}

/// Legge tutti i risultati; le righe illeggibili vengono scartate con un avviso.
pub fn read_results(saves_dir: &Path) -> Vec<GameResult> {
    let path = saves_dir.join(RESULTS_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => parse_results(&bytes),
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                tracing::warn!(path = %path.display(), error = %e, "lettura di results.jsonl fallita");
            }
            Vec::new()
        }
    }
}

/// Classifica generale: vittorie, poi punti totali. I nomi si confrontano
/// in NFC e minuscolo ("Anna" e "anna" sono lo stesso giocatore).
pub fn standings(results: &[GameResult]) -> Vec<StandingRow> {
    let mut rows: Vec<(String, StandingRow)> = Vec::new();
    for r in results {
        for p in &r.players {
            let key = name_key(&p.name);
            let idx = match rows.iter().position(|(k, _)| *k == key) {
                Some(i) => i,
                None => {
                    rows.push((
                        key,
                        StandingRow {
                            name: p.name.clone(),
                            games: 0,
                            wins: 0,
                            total_points: 0,
                            best: i32::MIN,
                        },
                    ));
                    rows.len() - 1
                }
            };
            let row = &mut rows[idx].1;
            row.games += 1;
            row.wins += u32::from(p.winner);
            row.total_points += i64::from(p.total);
            row.best = row.best.max(p.total);
        }
    }
    let mut rows: Vec<StandingRow> = rows.into_iter().map(|(_, row)| row).collect();
    rows.sort_by(|a, b| {
        b.wins
            .cmp(&a.wins)
            .then(b.total_points.cmp(&a.total_points))
            .then(a.name.cmp(&b.name))
    });
    rows
}

// ── Token ───────────────────────────────────────────────────────────

/// SHA-256 esadecimale del token: nel save finisce solo questo.
pub fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

/// Confronto a tempo costante tra un token e l'hash salvato.
pub(crate) fn token_matches(token: &str, hash: &str) -> bool {
    hash_token(token).as_bytes().ct_eq(hash.as_bytes()).into()
}

/// Confronto a tempo costante tra due token.
pub(crate) fn tokens_equal(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

// ── Formato del save ────────────────────────────────────────────────

/// Dati di salvataggio di una partita.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveData {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    /// Assente = 1, cioè regole che il server non sa più riprodurre.
    #[serde(rename = "rulesVersion", default = "missing_rules_version")]
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

fn missing_rules_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavePlayer {
    pub id: String,
    pub name: String,
    pub color: String,
    /// SHA-256 esadecimale del token. Assente nei save di formato 1.
    #[serde(rename = "tokenHash", default, skip_serializing_if = "Option::is_none")]
    pub token_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveActionRecord {
    pub action: ttr_core::Action,
}

/// Costruisce i dati di salvataggio dalla sessione corrente.
pub(crate) fn build_save_data(session: &GameSession) -> SaveData {
    // Prima dello start di un resume il motore è ancora in `resumed_engine`.
    let engine = session.engine.as_ref().or(session.resumed_engine.as_ref());

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

    let status =
        if engine.is_some_and(|e| matches!(e.phase(), ttr_core::error::GamePhase::GameOver)) {
            "completed"
        } else {
            "in_progress"
        };

    // I giocatori sono in ordine di indice del motore: `replay_engine` passa
    // `save.players` a `Engine::new` in quest'ordine.
    let players: Vec<SavePlayer> = if session.saved_roster.is_empty() {
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
                    token_hash: Some(hash_token(&p.token)),
                }
            })
            .collect()
    } else {
        // Partita ripresa: il roster salvato è completo anche quando non tutti
        // sono rientrati; i token cambiati da join o reassign sostituiscono l'hash.
        let mut entries: Vec<&crate::session::SavedEntry> = session.saved_roster.values().collect();
        entries.sort_by_key(|e| e.engine_index);
        entries
            .into_iter()
            .map(|e| {
                let joined = session
                    .player_index
                    .get(&e.id)
                    .and_then(|&pos| session.players.get(pos));
                SavePlayer {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    color: format!("{:?}", e.color).to_uppercase(),
                    token_hash: match joined {
                        Some(p) => Some(hash_token(&p.token)),
                        None => e.token_hash.clone(),
                    },
                }
            })
            .collect()
    };

    SaveData {
        format_version: FORMAT_VERSION,
        rules_version: ttr_core::RULES_VERSION,
        game_id: session.game_id.clone(),
        game_name: session.game_name.clone(),
        map_id: session.map_id.clone(),
        ruleset_id: session.ruleset_id.clone(),
        seed,
        first_player: 0,
        players,
        actions,
        status: status.to_string(),
        created_at: session.created_at.clone(),
        updated_at: now_rfc3339(),
    }
}

/// Scrive i dati con `durable_write`, creando la directory se manca.
pub(crate) fn write_atomic(path: &Path, data: &SaveData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| format!("serializzazione: {}", e))?;

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creazione directory {:?}: {}", parent, e))?;
    }
    durable_write(path, json.as_bytes()).map_err(|e| format!("scrittura {:?}: {}", path, e))
}

/// Scompone `{stem}.{kind}-{unix}[-{n}].bak` in `(stem, (unix, n))`; `None` se il nome
/// ha un'altra forma. L'ordine `(unix, n)` è numerico, non lessicografico.
fn parse_backup_name<'a>(name: &'a str, kind: &str) -> Option<(&'a str, (u64, u64))> {
    let (stem, rest) = name.strip_suffix(".bak")?.split_once('.')?;
    let rest = rest.strip_prefix(kind)?.strip_prefix('-')?;
    let (unix, n) = rest.split_once('-').unwrap_or((rest, "0"));
    let number = |s: &str| {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u64>().ok()
    };
    Some((stem, (number(unix)?, number(n)?)))
}

/// Ordine `(unix, n)` e percorso di un backup.
type BackupEntry = ((u64, u64), PathBuf);

/// Backup `{stem}.resume-*.bak` di `path`, dal più vecchio al più recente.
fn list_resume_backups(path: &Path) -> Result<Vec<BackupEntry>, String> {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return Ok(Vec::new());
    };
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let entries = std::fs::read_dir(dir).map_err(|e| format!("lettura {:?}: {}", dir, e))?;
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((entry_stem, order)) = name.to_str().and_then(|n| parse_backup_name(n, "resume"))
        else {
            continue;
        };
        if entry_stem == stem && entry.file_type().is_ok_and(|t| t.is_file()) {
            found.push((order, entry.path()));
        }
    }
    found.sort();
    Ok(found)
}

fn file_digest(file: &mut std::fs::File) -> std::io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    std::io::copy(file, &mut hasher)?;
    Ok(hasher.finalize().into())
}

/// Copia il save in `{id}.resume-{unix}.bak` prima di un resume, senza mai
/// sovrascrivere un backup esistente; se il backup più recente è identico al save
/// non ne crea uno nuovo e ritorna quello. Dopo una copia restano al più
/// `MAX_RESUME_BACKUPS` backup. Ritorna il percorso del backup, o `None` se il
/// save non esiste. `path` deve avere uno stem già validato.
pub(crate) fn backup_before_resume(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut source = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("lettura {:?} per backup: {}", path, e)),
    };

    let existing = list_resume_backups(path)?;
    let source_digest =
        file_digest(&mut source).map_err(|e| format!("lettura {:?} per backup: {}", path, e))?;
    std::io::Seek::rewind(&mut source)
        .map_err(|e| format!("lettura {:?} per backup: {}", path, e))?;
    if let Some((_, newest)) = existing.last()
        && std::fs::File::open(newest)
            .and_then(|mut file| file_digest(&mut file))
            .is_ok_and(|digest| digest == source_digest)
    {
        return Ok(Some(newest.clone()));
    }

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
        // `existing` non contiene il backup appena creato: ne restano al più
        // `MAX_RESUME_BACKUPS - 1` più recenti.
        let excess = existing.len().saturating_sub(MAX_RESUME_BACKUPS - 1);
        for (_, old) in &existing[..excess] {
            if let Err(e) = std::fs::remove_file(old) {
                tracing::warn!(path = %old.display(), error = %e, "eliminazione del vecchio backup fallita");
            }
        }
        return Ok(Some(backup_path));
    }

    Err(format!("troppi backup esistenti per {:?}", path))
}

/// Elimina un save rinominandolo in `{id}.deleted-{unix}.bak`, senza mai
/// sovrascrivere un backup esistente. Ritorna il nuovo percorso, o `None` se il
/// save non esiste. Il chiamante garantisce che nessuna sessione scriva `path`.
pub(crate) fn soft_delete_save(path: &Path) -> Result<Option<PathBuf>, String> {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for attempt in 0..MAX_BACKUP_ATTEMPTS {
        let extension = if attempt == 0 {
            format!("deleted-{}.bak", unix)
        } else {
            format!("deleted-{}-{}.bak", unix, attempt)
        };
        let backup_path = path.with_extension(extension);
        match backup_path.try_exists() {
            Ok(false) => {}
            Ok(true) => continue,
            Err(e) => return Err(format!("controllo {:?}: {}", backup_path, e)),
        }
        return match std::fs::rename(path, &backup_path) {
            Ok(()) => Ok(Some(backup_path)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("rinomina {:?} in {:?}: {}", path, backup_path, e)),
        };
    }

    Err(format!("troppi backup esistenti per {:?}", path))
}

/// Elimina i `{uuid}.deleted-{unix}[-{n}].bak` il cui `unix` nel nome è anteriore di più di
/// `retention` a ora (la rinomina conserva la mtime originale, quindi non conta).
/// Ritorna il numero di file eliminati; gli errori sono avvisi.
pub fn prune_deleted_saves(saves_dir: &Path, retention: std::time::Duration) -> usize {
    let entries = match std::fs::read_dir(saves_dir) {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                tracing::warn!(dir = %saves_dir.display(), error = %e, "lettura della cartella dei save fallita");
            }
            return 0;
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now.saturating_sub(retention.as_secs());

    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((stem, (unix, _))) = name.to_str().and_then(|n| parse_backup_name(n, "deleted"))
        else {
            continue;
        };
        if unix >= cutoff
            || !is_canonical_uuid(stem)
            || !entry.file_type().is_ok_and(|t| t.is_file())
        {
            continue;
        }
        let path = entry.path();
        match std::fs::remove_file(&path) {
            Ok(()) => {
                tracing::info!(path = %path.display(), "save eliminato scaduto rimosso");
                removed += 1;
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "rimozione del save eliminato fallita");
            }
        }
    }
    removed
}

/// Legge un save file.
pub fn read_save(path: &Path) -> Result<SaveData, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("lettura {:?}: {}", path, e))?;
    let save: SaveData =
        serde_json::from_str(&json).map_err(|e| format!("parsing save {:?}: {}", path, e))?;
    Ok(save)
}

/// `true` se `s` è un UUID in forma canonica (minuscolo, con trattini): l'unica
/// ammessa nei nomi dei file di save.
pub(crate) fn is_canonical_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|id| id.to_string() == s)
}

/// Esito del controllo su un file di save prima di leggerlo.
pub(crate) enum SaveFileCheck {
    Ok,
    /// Non è un file regolare (symlink, cartella, ...).
    NotRegular,
    /// Supera `MAX_SAVE_FILE_BYTES`.
    TooLarge(u64),
}

/// Controlla con `symlink_metadata` che `path` sia un file regolare di dimensione ammessa;
/// i symlink sono rifiutati.
pub(crate) fn check_save_file(path: &Path) -> std::io::Result<SaveFileCheck> {
    let meta = std::fs::symlink_metadata(path)?;
    Ok(if !meta.is_file() {
        SaveFileCheck::NotRegular
    } else if meta.len() > MAX_SAVE_FILE_BYTES {
        SaveFileCheck::TooLarge(meta.len())
    } else {
        SaveFileCheck::Ok
    })
}

/// Una partita salvata e riprendibile, come la elenca `GET /api/saves`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSummary {
    pub game_id: String,
    pub name: String,
    pub ruleset_id: String,
    pub players: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Elenca i save riprendibili di `{uuid}.json` nella cartella, dal più recente:
/// `in_progress`, con ruleset noto e che superano `validate_save` (formato,
/// versione delle regole, roster; il replay resta al resume). I file che non
/// sono save (profili, risultati, backup, temporanei) sono ignorati; quelli
/// illeggibili o con `gameId` diverso dal nome file sono saltati con un avviso.
pub(crate) fn list_in_progress_saves(state: &AppState) -> Vec<SaveSummary> {
    let dir = &state.saves_dir;
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                tracing::warn!(dir = %dir.display(), error = %e, "lettura della cartella dei save fallita");
            }
            return Vec::new();
        }
    };

    let mut saves = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".json"))
            .filter(|stem| is_canonical_uuid(stem))
        else {
            continue;
        };
        match check_save_file(&path) {
            Ok(SaveFileCheck::Ok) => {}
            Ok(SaveFileCheck::NotRegular) => {
                tracing::warn!(path = %path.display(), "non è un file regolare: save non elencato");
                continue;
            }
            Ok(SaveFileCheck::TooLarge(size)) => {
                tracing::warn!(path = %path.display(), size, "save troppo grande: non elencato");
                continue;
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "save non ispezionabile: non elencato");
                continue;
            }
        }
        let save = match read_save(&path) {
            Ok(save) => save,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "save illeggibile: non elencato");
                continue;
            }
        };
        if save.status != "in_progress" {
            continue;
        }
        if save.game_id != stem {
            tracing::warn!(path = %path.display(), game_id = %save.game_id, "gameId diverso dal nome del file: save non elencato");
            continue;
        }
        let Some(ruleset) = state.ruleset(&save.ruleset_id) else {
            tracing::debug!(path = %path.display(), ruleset = %save.ruleset_id, "ruleset sconosciuto: save non elencato");
            continue;
        };
        // Stessi controlli del resume, senza replay: formato, versione delle
        // regole, roster.
        if let Err(reason) = validate_save(&save, ruleset) {
            tracing::debug!(path = %path.display(), %reason, "save non riprendibile: non elencato");
            continue;
        }
        saves.push(SaveSummary {
            game_id: save.game_id,
            name: truncate_chars(&save.game_name, crate::lobby::MAX_GAME_NAME_CHARS),
            ruleset_id: save.ruleset_id,
            players: save
                .players
                .into_iter()
                .map(|p| truncate_chars(&p.name, MAX_PLAYER_NAME_CHARS))
                .collect(),
            created_at: save.created_at,
            updated_at: save.updated_at,
        });
    }
    saves.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.game_id.cmp(&b.game_id))
    });
    saves
}

/// Primi `max` caratteri di `s`: un save manomesso non gonfia le risposte dell'elenco.
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Replay: ricostruisce il motore da un save file.
pub fn replay_engine(
    save: &SaveData,
    map: &ttr_core::MapData,
    ruleset: &ttr_core::Ruleset,
) -> Result<ttr_core::Engine, String> {
    replay_with_final_scores(save, map, ruleset).map(|(engine, _)| engine)
}

/// Come `replay_engine`, e restituisce anche i punteggi finali se il replay
/// arriva al game over.
fn replay_with_final_scores(
    save: &SaveData,
    map: &ttr_core::MapData,
    ruleset: &ttr_core::Ruleset,
) -> Result<(ttr_core::Engine, Option<Vec<ttr_core::event::FinalScore>>), String> {
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
    )
    .map_err(|e| format!("motore non inizializzabile: {}", e))?;

    let mut final_scores = None;
    for rec in &save.actions {
        let events = engine
            .apply(rec.action.clone())
            .map_err(|e| format!("replay azione: {}", e))?;
        for event in events {
            if let ttr_core::event::GameEvent::GameOver {
                final_scores: scores,
                ..
            } = event
            {
                final_scores = Some(scores);
            }
        }
    }

    Ok((engine, final_scores))
}

/// All'avvio archivia in `results.jsonl` i save `completed` rimasti su disco
/// (archiviazione fallita in una sessione precedente) e li elimina; elimina
/// anche i `*.tmp` più vecchi di un'ora. Ritorna il numero di save archiviati;
/// i save illeggibili o non riproducibili restano dove sono, con un avviso.
pub fn archive_completed_saves(state: &AppState) -> usize {
    let dir = &state.saves_dir;
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                tracing::warn!(dir = %dir.display(), error = %e, "lettura della cartella dei save fallita");
            }
            return 0;
        }
    };

    let mut archived = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with(".tmp") {
            remove_if_stale(&path);
        } else if name.ends_with(".json") && name != crate::profiles::PROFILES_FILE {
            match archive_if_completed(state, &path) {
                Ok(true) => archived += 1,
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "save non archiviato");
                }
            }
        }
    }
    archived
}

fn remove_if_stale(path: &Path) {
    let age = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| std::time::SystemTime::now().duration_since(modified).ok());
    if age.is_some_and(|age| age > STALE_TMP_AGE) {
        match std::fs::remove_file(path) {
            Ok(()) => tracing::info!(path = %path.display(), "file temporaneo obsoleto eliminato"),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "eliminazione del file temporaneo fallita");
            }
        }
    }
}

/// Archivia e elimina il save se è `completed`; `Ok(false)` se non lo è.
fn archive_if_completed(state: &AppState, path: &Path) -> Result<bool, String> {
    let save = read_save(path)?;
    if save.status != "completed" {
        return Ok(false);
    }
    let ruleset = state
        .ruleset(&save.ruleset_id)
        .ok_or_else(|| format!("ruleset '{}' sconosciuto", save.ruleset_id))?;
    let map = state
        .map(&ruleset.map)
        .ok_or_else(|| format!("mappa '{}' sconosciuta", ruleset.map))?;
    let (_, scores) = replay_with_final_scores(&save, map, ruleset)?;
    let scores = scores.ok_or("save completed senza game over nel replay")?;

    let result = GameResult::from_scores(
        &save.game_id,
        &save.game_name,
        &save.ruleset_id,
        save.updated_at.clone(),
        scores.iter().map(|fs| {
            let name = save
                .players
                .get(fs.player_index)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            (name, fs.total, fs.winner)
        }),
    );
    append_result(&state.saves_dir, &state.results_lock, &result)?;
    std::fs::remove_file(path).map_err(|e| format!("eliminazione {:?}: {}", path, e))?;
    tracing::info!(game_id = %save.game_id, "save completed archiviato all'avvio");
    Ok(true)
}

/// Rifiuta i save che il motore non può riprodurre o che il resume non può
/// associare ai giocatori (roster indicizzato per `name_key`).
fn validate_save(save: &SaveData, ruleset: &ttr_core::Ruleset) -> Result<(), String> {
    if !matches!(save.format_version, 1 | 2) {
        return Err(format!(
            "formato del save non supportato: {}",
            save.format_version
        ));
    }
    if save.rules_version < ttr_core::RULES_VERSION {
        return Err("save creato con regole precedenti: non riproducibile".to_string());
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
        if !names.insert(name_key(&player.name)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn result(game_id: &str) -> GameResult {
        GameResult {
            game_id: game_id.into(),
            game_name: "Prova".into(),
            ruleset_id: "usa_v2".into(),
            finished_at: "t".into(),
            players: vec![ResultPlayer {
                name: "Anna".into(),
                total: 42,
                winner: true,
            }],
        }
    }

    #[test]
    fn now_rfc3339_parses_back() {
        use time::format_description::well_known::Rfc3339;
        let now = now_rfc3339();
        let parsed = time::OffsetDateTime::parse(&now, &Rfc3339).expect("RFC 3339 valido");
        assert!(now.ends_with('Z'));
        assert_eq!(parsed.nanosecond(), 0);
        let delta = (time::OffsetDateTime::now_utc() - parsed).whole_seconds();
        assert!((0..5).contains(&delta), "orario lontano da adesso: {now}");
    }

    #[test]
    fn standings_aggregate_by_name_and_rank_by_wins() {
        let game = |players: &[(&str, i32, bool)]| GameResult {
            game_id: "g".into(),
            game_name: "g".into(),
            ruleset_id: "usa_v2".into(),
            finished_at: "t".into(),
            players: players
                .iter()
                .map(|&(name, total, winner)| ResultPlayer {
                    name: name.into(),
                    total,
                    winner,
                })
                .collect(),
        };
        let results = vec![
            game(&[("Anna", 100, true), ("Bob", 90, false)]),
            game(&[("anna", 70, false), ("Bob", 120, true), ("Ciro", 30, false)]),
            game(&[("Bob", 80, true), ("Ciro", 60, false)]),
        ];
        let rows = standings(&results);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Bob", "Anna", "Ciro"]);
        assert_eq!(rows[0].wins, 2);
        assert_eq!(rows[0].games, 3);
        assert_eq!(rows[1].games, 2, "Anna e anna sono lo stesso giocatore");
        assert_eq!(rows[1].total_points, 170);
        assert_eq!(rows[1].best, 100);
    }

    #[test]
    fn standings_merge_names_equal_after_nfc() {
        let game = |name: &str| GameResult {
            players: vec![ResultPlayer {
                name: name.into(),
                total: 10,
                winner: true,
            }],
            ..result("g")
        };
        let rows = standings(&[game("\u{e9}"), game("e\u{301}")]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].games, 2);
    }

    #[test]
    fn results_append_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Mutex::new(());
        assert!(append_result(dir.path(), &lock, &result("g1")).unwrap());
        assert!(append_result(dir.path(), &lock, &result("g2")).unwrap());
        let back = read_results(dir.path());
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].players[0].total, 42);
    }

    #[test]
    fn results_append_is_idempotent_per_game() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Mutex::new(());
        assert!(append_result(dir.path(), &lock, &result("g1")).unwrap());
        assert!(!append_result(dir.path(), &lock, &result("g1")).unwrap());
        assert_eq!(read_results(dir.path()).len(), 1);
    }

    #[test]
    fn results_concurrent_appends_are_line_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Arc::new(Mutex::new(()));
        let path = Arc::new(dir.path().to_path_buf());
        let threads: Vec<_> = (0..16)
            .map(|t| {
                let (lock, path) = (lock.clone(), path.clone());
                std::thread::spawn(move || {
                    for n in 0..50 {
                        append_result(&path, &lock, &result(&format!("g{t}-{n}"))).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let raw = std::fs::read_to_string(dir.path().join(RESULTS_FILE)).unwrap();
        assert_eq!(raw.lines().count(), 800, "una riga per record");
        assert_eq!(read_results(dir.path()).len(), 800);
    }

    #[test]
    fn results_recovers_after_partial_line() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Mutex::new(());
        append_result(dir.path(), &lock, &result("g1")).unwrap();
        let path = dir.path().join(RESULTS_FILE);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(br#"{"gameId":"g2","gameNa"#).unwrap();
        drop(file);

        append_result(dir.path(), &lock, &result("g3")).unwrap();
        let ids: Vec<String> = read_results(dir.path())
            .into_iter()
            .map(|r| r.game_id)
            .collect();
        assert_eq!(
            ids,
            ["g1", "g3"],
            "la riga parziale è scartata, il nuovo record è leggibile"
        );
    }

    #[test]
    fn durable_write_replaces_file_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.json");
        durable_write(&path, b"uno").unwrap();
        durable_write(&path, b"due").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"due");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["save.json"]);
    }

    #[test]
    fn token_hash_is_hex_sha256_and_matches_in_constant_time_helper() {
        let hash = hash_token("abc");
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(token_matches("abc", &hash));
        assert!(!token_matches("abd", &hash));
        assert!(tokens_equal("x", "x") && !tokens_equal("x", "y"));
    }

    #[test]
    fn test_save_roundtrip() {
        let data = SaveData {
            format_version: FORMAT_VERSION,
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
                token_hash: Some(hash_token("t")),
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
        assert_eq!(back.players[0].token_hash, data.players[0].token_hash);
    }

    fn resume_backup_names(dir: &Path, id: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| parse_backup_name(n, "resume").is_some_and(|(stem, _)| stem == id))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn resume_backups_are_capped_at_five() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("{id}.json"));
        let seed = |name: String| std::fs::write(dir.path().join(&name), &name).unwrap();

        // L'ordine è numerico: (100, 10) è più recente di (100, 2).
        for suffix in ["99", "100", "100-2", "100-10", "1000"] {
            seed(format!("{id}.resume-{suffix}.bak"));
        }
        let unrelated = [
            format!("{id}.resume-abc.bak"),
            format!("{id}.resume-1.bak.tmp"),
            format!("{}.resume-1.bak", uuid::Uuid::new_v4()),
            format!("{id}.deleted-1.bak"),
        ];
        for name in &unrelated {
            seed(name.clone());
        }

        let mut expected_gone = ["99", "100", "100-2"].into_iter();
        for content in ["uno", "due", "tre"] {
            std::fs::write(&path, content).unwrap();
            let backup = backup_before_resume(&path).unwrap().unwrap();
            assert_eq!(std::fs::read_to_string(backup).unwrap(), content);
            let names = resume_backup_names(dir.path(), &id);
            assert_eq!(names.len(), 5, "{names:?}");
            let gone = format!("{id}.resume-{}.bak", expected_gone.next().unwrap());
            assert!(!names.contains(&gone), "{gone} doveva sparire: {names:?}");
        }
        assert!(
            resume_backup_names(dir.path(), &id).contains(&format!("{id}.resume-100-10.bak")),
            "il più recente tra i vecchi resta"
        );

        for content in ["quattro", "cinque", "sei", "sette"] {
            std::fs::write(&path, content).unwrap();
            backup_before_resume(&path).unwrap().unwrap();
            assert_eq!(resume_backup_names(dir.path(), &id).len(), 5);
        }
        for name in &unrelated {
            assert!(dir.path().join(name).exists(), "{name} non va toccato");
        }
    }

    #[test]
    fn identical_resume_backup_is_not_duplicated() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("{id}.json"));
        assert_eq!(backup_before_resume(&path).unwrap(), None);

        std::fs::write(&path, "contenuto").unwrap();
        let first = backup_before_resume(&path).unwrap().unwrap();
        let second = backup_before_resume(&path).unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(resume_backup_names(dir.path(), &id).len(), 1);

        std::fs::write(&path, "cambiato").unwrap();
        let third = backup_before_resume(&path).unwrap().unwrap();
        assert_ne!(first, third);
        assert_eq!(resume_backup_names(dir.path(), &id).len(), 2);
    }

    #[test]
    fn old_deleted_saves_are_pruned_at_startup() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let day = 24 * 60 * 60;
        let touch = |name: String| std::fs::write(dir.path().join(name), "x").unwrap();

        let old = [
            format!("{id}.deleted-{}.bak", now - 40 * day),
            format!("{id}.deleted-{}-3.bak", now - 31 * day),
        ];
        let kept = [
            format!("{id}.deleted-{}.bak", now - day),
            format!("{id}.deleted-{}-1.bak", now),
            format!("{id}.json"),
            format!("{id}.resume-1.bak"),
            format!("{id}.deleted-1.bak.tmp"),
            format!("{id}.deleted-abc.bak"),
            "non-un-uuid.deleted-1.bak".to_string(),
            format!("{}.deleted-1.bak", id.to_uppercase()),
        ];
        for name in old.iter().chain(&kept) {
            touch(name.clone());
        }

        let retention = std::time::Duration::from_secs(30 * day);
        assert_eq!(prune_deleted_saves(dir.path(), retention), 2);
        for name in &old {
            assert!(!dir.path().join(name).exists(), "{name} doveva sparire");
        }
        for name in &kept {
            assert!(dir.path().join(name).exists(), "{name} doveva restare");
        }
        assert_eq!(prune_deleted_saves(dir.path(), retention), 0);
    }
}
