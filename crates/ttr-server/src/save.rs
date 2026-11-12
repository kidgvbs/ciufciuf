//! Persistenza partite: formato JSON, scrittura durevole (tmp + fsync + rename),
//! archivio dei risultati, replay da save.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{ErrorKind, Read, Write};
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

/// Un backup con timestamp oltre `adesso + questo margine` (secondi) è anomalo.
const BACKUP_FUTURE_SLACK_SECS: u64 = 24 * 60 * 60;

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

/// Scrive i dati con `durable_write`. La cartella dei save esiste già (la crea `main.rs`
/// all'avvio): non va ricreata qui, altrimenti una scrittura in ritardo la farebbe rinascere.
pub(crate) fn write_atomic(path: &Path, data: &SaveData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| format!("serializzazione: {}", e))?;
    durable_write(path, json.as_bytes()).map_err(|e| format!("scrittura {:?}: {}", path, e))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Scompone `{stem}.{kind}-{unix}[-{n}].bak`. `None` se il nome ha un'altra forma;
/// `Some((stem, None))` se è un backup anomalo (timestamp non numerico o oltre
/// `now + 1 giorno`); altrimenti `Some((stem, Some((unix, n))))`. L'ordine
/// `(unix, n)` è numerico, non lessicografico.
fn parse_backup_name<'a>(
    name: &'a str,
    kind: &str,
    now: u64,
) -> Option<(&'a str, Option<(u64, u64)>)> {
    let (stem, rest) = name.strip_suffix(".bak")?.split_once('.')?;
    let rest = rest.strip_prefix(kind)?.strip_prefix('-')?;
    let (unix, n) = rest.split_once('-').unwrap_or((rest, "0"));
    let number = |s: &str| {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u64>().ok()
    };
    let order = number(unix)
        .zip(number(n))
        .filter(|&(unix, _)| unix <= now.saturating_add(BACKUP_FUTURE_SLACK_SECS));
    Some((stem, order))
}

/// Ordine `(unix, n)` (`None` se anomalo: ordina per primo) e percorso di un backup.
type BackupEntry = (Option<(u64, u64)>, PathBuf);

/// Backup `{stem}.resume-*.bak` di `path`, dal più vecchio al più recente; quelli
/// anomali vengono prima di tutti.
fn list_resume_backups(path: &Path) -> Result<Vec<BackupEntry>, String> {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return Ok(Vec::new());
    };
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let entries = std::fs::read_dir(dir).map_err(|e| format!("lettura {:?}: {}", dir, e))?;
    let now = unix_now();
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((entry_stem, order)) = name
            .to_str()
            .and_then(|n| parse_backup_name(n, "resume", now))
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

/// Copia al più `MAX_SAVE_FILE_BYTES` byte da `src`; un file che ne contiene di più (per
/// esempio cresciuto dopo il controllo dei metadati) è un errore.
fn copy_bounded(src: &mut std::fs::File, dst: &mut impl Write) -> std::io::Result<u64> {
    let copied = std::io::copy(&mut Read::take(&mut *src, MAX_SAVE_FILE_BYTES + 1), dst)?;
    if copied > MAX_SAVE_FILE_BYTES {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "il save supera la dimensione massima",
        ));
    }
    Ok(copied)
}

fn file_digest(file: &mut std::fs::File) -> std::io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    copy_bounded(file, &mut hasher)?;
    Ok(hasher.finalize().into())
}

/// Copia il save in `{id}.resume-{unix}.bak` prima di un resume, senza mai
/// sovrascrivere un backup esistente; se il backup più recente è identico al save
/// non ne crea uno nuovo e ritorna quello. Dopo una copia restano al più
/// `MAX_RESUME_BACKUPS` backup. Ritorna il percorso del backup, o `None` se il
/// save non esiste. `path` deve avere uno stem già validato.
pub(crate) fn backup_before_resume(path: &Path) -> Result<Option<PathBuf>, String> {
    let (mut source, meta) = match open_regular(path) {
        Ok(opened) => opened,
        Err(SaveReadError::NotFound) => return Ok(None),
        Err(e) => return Err(format!("lettura {:?} per backup: {}", path, e)),
    };
    if meta.len() > MAX_SAVE_FILE_BYTES {
        return Err(format!(
            "lettura {:?} per backup: {}",
            path,
            SaveReadError::TooLarge(meta.len())
        ));
    }

    let existing = list_resume_backups(path)?;
    let source_digest =
        file_digest(&mut source).map_err(|e| format!("lettura {:?} per backup: {}", path, e))?;
    std::io::Seek::rewind(&mut source)
        .map_err(|e| format!("lettura {:?} per backup: {}", path, e))?;
    if let Some((_, newest)) = existing.last().filter(|(order, _)| order.is_some())
        && std::fs::File::open(newest)
            .and_then(|mut file| file_digest(&mut file))
            .is_ok_and(|digest| digest == source_digest)
    {
        return Ok(Some(newest.clone()));
    }

    let unix = unix_now();

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
        if let Err(e) = copy_bounded(&mut source, &mut backup) {
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
/// `retention` a ora (la rinomina conserva la mtime originale, quindi non conta). I nomi
/// anomali (timestamp non numerico o nel futuro) sono potati in base alla mtime del file.
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

    let now = unix_now();
    let cutoff = now.saturating_sub(retention.as_secs());

    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((stem, order)) = name
            .to_str()
            .and_then(|n| parse_backup_name(n, "deleted", now))
        else {
            continue;
        };
        if !is_canonical_uuid(stem) || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let expired = match order {
            Some((unix, _)) => unix < cutoff,
            // Nome anomalo: conta la mtime. Una mtime nel futuro oltre il margine è anomala
            // anch'essa, quindi scaduta.
            None => entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .is_some_and(|modified| {
                    match std::time::SystemTime::now().duration_since(modified) {
                        Ok(age) => age > retention,
                        Err(ahead) => ahead.duration().as_secs() > BACKUP_FUTURE_SLACK_SECS,
                    }
                }),
        };
        if !expired {
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

/// Legge un save file seguendo i symlink, senza limiti di dimensione: solo per il percorso
/// scelto dall'operatore (`--resume`). Per la cartella dei save c'è `read_save_bounded`.
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

/// Errore di `read_save_bounded`.
#[derive(Debug)]
pub(crate) enum SaveReadError {
    /// Il file non esiste (anche se sparito tra l'elenco della cartella e la lettura).
    NotFound,
    /// Non è un file regolare (symlink, cartella, FIFO, ...).
    NotRegular,
    /// Supera `MAX_SAVE_FILE_BYTES`.
    TooLarge(u64),
    /// Apertura o lettura fallita.
    Io(String),
    /// Il contenuto non è un save.
    Parse(String),
}

impl std::fmt::Display for SaveReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("il file non esiste"),
            Self::NotRegular => f.write_str("non è un file regolare"),
            Self::TooLarge(size) => write!(f, "save troppo grande: {size} byte"),
            Self::Io(msg) | Self::Parse(msg) => f.write_str(msg),
        }
    }
}

/// Apre un file regolare con una sola apertura: su Unix `O_NOFOLLOW` (un symlink è «non
/// regolare») e `O_NONBLOCK` (un FIFO non blocca l'apertura); il tipo viene dai metadati
/// dell'handle. Ritorna l'handle e i metadati; `NotFound`, `NotRegular` o `Io` altrimenti.
fn open_regular(path: &Path) -> Result<(std::fs::File, std::fs::Metadata), SaveReadError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(SaveReadError::NotFound),
        #[cfg(unix)]
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Err(SaveReadError::NotRegular),
        Err(e) => return Err(SaveReadError::Io(format!("apertura {:?}: {}", path, e))),
    };
    let meta = file
        .metadata()
        .map_err(|e| SaveReadError::Io(format!("metadati {:?}: {}", path, e)))?;
    if !meta.is_file() {
        return Err(SaveReadError::NotRegular);
    }
    Ok((file, meta))
}

/// Legge un save dalla cartella dei save con `open_regular`; la dimensione viene dai
/// metadati dell'handle e la lettura è comunque limitata a `MAX_SAVE_FILE_BYTES`, anche se
/// il file cresce dopo il controllo.
pub(crate) fn read_save_bounded(path: &Path) -> Result<SaveData, SaveReadError> {
    let (file, meta) = open_regular(path)?;
    if meta.len() > MAX_SAVE_FILE_BYTES {
        return Err(SaveReadError::TooLarge(meta.len()));
    }

    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_SAVE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SaveReadError::Io(format!("lettura {:?}: {}", path, e)))?;
    if bytes.len() as u64 > MAX_SAVE_FILE_BYTES {
        return Err(SaveReadError::TooLarge(bytes.len() as u64));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| SaveReadError::Parse(format!("parsing save {:?}: {}", path, e)))
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
        let save = match read_save_bounded(&path) {
            Ok(save) => save,
            // Sparito tra l'elenco della cartella e la lettura (es. eliminato): non c'è più.
            Err(SaveReadError::NotFound) => continue,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "save non leggibile: non elencato");
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
        // Stessi controlli del resume, senza replay: nomi entro i limiti, formato, versione
        // delle regole, roster.
        if let Err(reason) = check_name_limits(&save).and_then(|()| validate_save(&save, ruleset)) {
            tracing::debug!(path = %path.display(), %reason, "save non riprendibile: non elencato");
            continue;
        }
        saves.push(SaveSummary {
            game_id: save.game_id,
            name: save.game_name,
            ruleset_id: save.ruleset_id,
            players: save.players.into_iter().map(|p| p.name).collect(),
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
/// anche i `*.tmp` più vecchi di un'ora. Considera solo i file `{uuid canonico}.json`.
/// Ritorna il numero di save archiviati; i save non regolari, troppo grandi, illeggibili
/// o non riproducibili restano dove sono, con un avviso.
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
        } else if name.strip_suffix(".json").is_some_and(is_canonical_uuid) {
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
    let save = match read_save_bounded(path) {
        Ok(save) => save,
        // Sparito dopo l'elenco della cartella: niente da archiviare.
        Err(SaveReadError::NotFound) => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
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

/// Rifiuta i save con il nome della partita o di un giocatore oltre i limiti della lobby:
/// non sono riprendibili né elencati (troncare cambierebbe la `name_key` e romperebbe il
/// rientro). Vale solo per resume ed elenco: l'archiviazione all'avvio dei save `completed`
/// non lo usa, così un save concluso con nomi lunghi finisce comunque in classifica.
pub(crate) fn check_name_limits(save: &SaveData) -> Result<(), String> {
    if save
        .game_name
        .chars()
        .nth(crate::lobby::MAX_GAME_NAME_CHARS)
        .is_some()
    {
        return Err("nome della partita oltre il limite".to_string());
    }
    if save
        .players
        .iter()
        .any(|p| p.name.chars().nth(MAX_PLAYER_NAME_CHARS).is_some())
    {
        return Err("nome di un giocatore oltre il limite".to_string());
    }
    Ok(())
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
            .filter(|n| parse_backup_name(n, "resume", 0).is_some_and(|(stem, _)| stem == id))
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

    #[test]
    fn future_resume_backup_is_dropped_first() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("{id}.json"));
        let future = format!("{id}.resume-99999999999.bak");
        std::fs::write(dir.path().join(&future), "futuro").unwrap();

        for content in ["uno", "due", "tre", "quattro", "cinque"] {
            std::fs::write(&path, content).unwrap();
            backup_before_resume(&path).unwrap().unwrap();
        }
        let names = resume_backup_names(dir.path(), &id);
        assert_eq!(names.len(), 5, "{names:?}");
        assert!(
            !names.contains(&future),
            "il backup nel futuro resta: {names:?}"
        );
    }

    #[test]
    fn unparsable_resume_backups_are_dropped_before_valid_ones() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("{id}.json"));
        let anomalous = [
            format!("{id}.resume-abc.bak"),
            format!("{id}.resume-18446744073709551616.bak"),
        ];
        let valid = [1, 2, 3].map(|n| format!("{id}.resume-{n}.bak"));
        for name in anomalous.iter().chain(&valid) {
            std::fs::write(dir.path().join(name), name).unwrap();
        }

        for content in ["uno", "due"] {
            std::fs::write(&path, content).unwrap();
            backup_before_resume(&path).unwrap().unwrap();
        }
        let names = resume_backup_names(dir.path(), &id);
        assert_eq!(names.len(), 5, "{names:?}");
        for name in &anomalous {
            assert!(!names.contains(name), "{name} doveva sparire: {names:?}");
        }
        for name in &valid {
            assert!(names.contains(name), "{name} doveva restare: {names:?}");
        }
    }

    #[test]
    fn anomalous_deleted_saves_are_pruned_by_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let day = 24 * 60 * 60;
        let retention = std::time::Duration::from_secs(30 * day);
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(31 * day);
        let create = |name: &str, modified: std::time::SystemTime| {
            let file = std::fs::File::create(dir.path().join(name)).unwrap();
            file.set_modified(modified).unwrap();
        };

        let old = [
            format!("{id}.deleted-abc.bak"),
            format!("{id}.deleted-99999999999.bak"),
            format!("{id}.deleted-99999999999-4.bak"),
            format!("{id}.deleted-18446744073709551616.bak"),
        ];
        let fresh = [
            format!("{id}.deleted-def.bak"),
            format!("{id}.deleted-99999999998.bak"),
        ];
        for name in &old {
            create(name, long_ago);
        }
        for name in &fresh {
            create(name, std::time::SystemTime::now());
        }

        assert_eq!(prune_deleted_saves(dir.path(), retention), old.len());
        for name in &old {
            assert!(!dir.path().join(name).exists(), "{name} doveva sparire");
        }
        for name in &fresh {
            assert!(dir.path().join(name).exists(), "{name} doveva restare");
        }
    }

    #[test]
    fn anomalous_deleted_save_with_far_future_mtime_is_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let day = std::time::Duration::from_secs(24 * 60 * 60);
        let now = std::time::SystemTime::now();
        let create = |name: &str, modified: std::time::SystemTime| {
            let file = std::fs::File::create(dir.path().join(name)).unwrap();
            file.set_modified(modified).unwrap();
        };
        let far = format!("{id}.deleted-abc.bak");
        let near = format!("{id}.deleted-def.bak");
        create(&far, now + 2 * day);
        create(&near, now + day / 24);

        assert_eq!(prune_deleted_saves(dir.path(), 30 * day), 1);
        assert!(!dir.path().join(&far).exists());
        assert!(dir.path().join(&near).exists());
    }

    #[test]
    fn read_save_bounded_reports_typed_errors() {
        let dir = tempfile::tempdir().unwrap();

        let missing = dir.path().join("manca.json");
        assert!(matches!(
            read_save_bounded(&missing),
            Err(SaveReadError::NotFound)
        ));

        assert!(matches!(
            read_save_bounded(dir.path()),
            Err(SaveReadError::NotRegular)
        ));

        let broken = dir.path().join("rotto.json");
        std::fs::write(&broken, "{").unwrap();
        assert!(matches!(
            read_save_bounded(&broken),
            Err(SaveReadError::Parse(_))
        ));

        // Esattamente al limite: non è «troppo grande» (qui non è JSON, quindi Parse).
        let at_limit = dir.path().join("limite.json");
        std::fs::File::create(&at_limit)
            .unwrap()
            .set_len(MAX_SAVE_FILE_BYTES)
            .unwrap();
        assert!(matches!(
            read_save_bounded(&at_limit),
            Err(SaveReadError::Parse(_))
        ));

        let too_big = dir.path().join("grande.json");
        std::fs::File::create(&too_big)
            .unwrap()
            .set_len(MAX_SAVE_FILE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            read_save_bounded(&too_big),
            Err(SaveReadError::TooLarge(size)) if size == MAX_SAVE_FILE_BYTES + 1
        ));
    }

    #[cfg(unix)]
    #[test]
    fn read_save_bounded_rejects_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vero.json");
        std::fs::write(&target, "{}").unwrap();
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(
            read_save_bounded(&link),
            Err(SaveReadError::NotRegular)
        ));

        let dangling = dir.path().join("pendente.json");
        std::os::unix::fs::symlink(dir.path().join("non-esiste.json"), &dangling).unwrap();
        assert!(matches!(
            read_save_bounded(&dangling),
            Err(SaveReadError::NotRegular)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn backup_before_resume_refuses_symlinks_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("{id}.json"));

        let target = dir.path().join("vero.json");
        std::fs::write(&target, "contenuto").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(backup_before_resume(&path).is_err());
        assert!(resume_backup_names(dir.path(), &id).is_empty());
        std::fs::remove_file(&path).unwrap();

        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_SAVE_FILE_BYTES + 1)
            .unwrap();
        assert!(backup_before_resume(&path).is_err());
        assert!(resume_backup_names(dir.path(), &id).is_empty());
    }
}
