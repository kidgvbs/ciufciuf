//! Stato globale dell'applicazione: registry partite, regole, configurazione.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tokio::sync::RwLock;
use ttr_core::{MapData, Ruleset};

use crate::session::{GameHandle, SessionLimits};

/// Stato condiviso dell'applicazione.
#[derive(Clone)]
pub struct AppState {
    /// Partite attive, indicizzate per game_id.
    pub games: Arc<RwLock<BTreeMap<String, GameHandle>>>,
    /// Ruleset caricati, indicizzati per id (file stem di `data/rulesets/*.json`).
    pub rulesets: Arc<BTreeMap<String, Ruleset>>,
    /// Mappe caricate, indicizzate per id (file stem di `data/maps/*.json`).
    pub maps: Arc<BTreeMap<String, MapData>>,
    /// Percorso della cartella `saves/`.
    pub saves_dir: PathBuf,
    /// Percorso della cartella `data/`.
    pub data_dir: PathBuf,
    /// Porta su cui il server è in ascolto.
    pub port: u16,
    /// Serializza le scritture di `saves/profiles.json`.
    pub profiles_lock: Arc<std::sync::Mutex<()>>,
    /// Serializza le scritture di `saves/results.jsonl`.
    pub results_lock: Arc<std::sync::Mutex<()>>,
    /// Durate e limiti delle sessioni; i test li riducono.
    pub limits: SessionLimits,
    /// Se `true`, `POST /api/games` accetta un seed scelto dal client.
    pub allow_client_seed: bool,
    /// Impostato allo spegnimento: le creazioni, i join e i WebSocket nuovi
    /// ricevono 503.
    pub shutting_down: Arc<AtomicBool>,
}

impl AppState {
    /// Crea lo stato dell'applicazione caricando dati da `data_dir`.
    /// Fallisce se un ruleset non è coerente con la sua mappa.
    pub fn new(data_dir: PathBuf, saves_dir: PathBuf, port: u16) -> Result<Self, String> {
        let rulesets = load_rulesets(&data_dir)?;
        let maps = load_maps(&data_dir, &rulesets)?;
        validate_rulesets(&rulesets, &maps)?;

        Ok(Self {
            games: Arc::new(RwLock::new(BTreeMap::new())),
            rulesets: Arc::new(rulesets),
            maps: Arc::new(maps),
            saves_dir,
            data_dir,
            port,
            profiles_lock: Arc::new(std::sync::Mutex::new(())),
            results_lock: Arc::new(std::sync::Mutex::new(())),
            limits: SessionLimits::default(),
            allow_client_seed: false,
            shutting_down: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Sostituisce i limiti delle sessioni.
    pub fn with_limits(mut self, limits: SessionLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Ammette o vieta il seed scelto dal client alla creazione di una partita.
    pub fn with_allow_client_seed(mut self, allow: bool) -> Self {
        self.allow_client_seed = allow;
        self
    }

    /// Restituisce il ruleset dato l'id, se esiste.
    pub fn ruleset(&self, id: &str) -> Option<&Ruleset> {
        self.rulesets.get(id)
    }

    /// Restituisce la mappa dato l'id, se esiste.
    pub fn map(&self, id: &str) -> Option<&MapData> {
        self.maps.get(id)
    }
}

/// Carica tutti i ruleset da `data/rulesets/*.json`.
fn load_rulesets(data_dir: &Path) -> Result<BTreeMap<String, Ruleset>, String> {
    let rulesets_dir = data_dir.join("rulesets");
    let mut map = BTreeMap::new();

    let entries = std::fs::read_dir(&rulesets_dir).map_err(|e| {
        format!(
            "impossibile leggere la cartella rulesets {:?}: {}",
            rulesets_dir, e
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("errore di I/O rulesets: {}", e))?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("nome file non valido: {:?}", path))?
            .to_string();

        let json =
            std::fs::read_to_string(&path).map_err(|e| format!("lettura {:?}: {}", path, e))?;
        let ruleset: Ruleset = serde_json::from_str(&json)
            .map_err(|e| format!("parsing ruleset {:?}: {}", path, e))?;
        map.insert(id, ruleset);
    }

    if map.is_empty() {
        return Err(format!("nessun ruleset trovato in {:?}", rulesets_dir));
    }
    Ok(map)
}

/// Carica le mappe referenziate dai ruleset da `data/maps/{map}.json`.
fn load_maps(
    data_dir: &Path,
    rulesets: &BTreeMap<String, Ruleset>,
) -> Result<BTreeMap<String, MapData>, String> {
    let maps_dir = data_dir.join("maps");
    let mut loaded = BTreeMap::new();

    for ruleset in rulesets.values() {
        let map_name = &ruleset.map;
        if loaded.contains_key(map_name) {
            continue;
        }
        let path = maps_dir.join(format!("{}.json", map_name));
        let json =
            std::fs::read_to_string(&path).map_err(|e| format!("lettura {:?}: {}", path, e))?;
        let map: MapData =
            serde_json::from_str(&json).map_err(|e| format!("parsing mappa {:?}: {}", path, e))?;
        loaded.insert(map_name.clone(), map);
    }

    if loaded.is_empty() {
        return Err(format!("nessuna mappa trovata in {:?}", maps_dir));
    }
    Ok(loaded)
}

/// Verifica ogni coppia ruleset/mappa: il server non parte con dati incoerenti.
fn validate_rulesets(
    rulesets: &BTreeMap<String, Ruleset>,
    maps: &BTreeMap<String, MapData>,
) -> Result<(), String> {
    for (id, ruleset) in rulesets {
        let map = maps
            .get(&ruleset.map)
            .ok_or_else(|| format!("mappa '{}' del ruleset '{}' non caricata", ruleset.map, id))?;
        ruleset
            .validate(map)
            .map_err(|e| format!("ruleset '{}' non valido: {}", id, e))?;
    }
    Ok(())
}
