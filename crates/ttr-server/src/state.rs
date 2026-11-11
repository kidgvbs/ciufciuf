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
    /// Documenti JSON delle mappe come letti da disco (con i campi che il
    /// motore ignora, come `lat`/`lon`), indicizzati come `maps`.
    pub map_documents: Arc<BTreeMap<String, serde_json::Value>>,
    /// Layout schematici delle mappe (`data/layouts/{map}.json`), indicizzati per id
    /// mappa; una mappa senza file non ha layout.
    pub layout_documents: Arc<BTreeMap<String, serde_json::Value>>,
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
        let (maps, map_documents) = load_maps(&data_dir, &rulesets)?;
        validate_rulesets(&rulesets, &maps)?;
        let layout_documents = load_layouts(&data_dir, &maps)?;

        Ok(Self {
            games: Arc::new(RwLock::new(BTreeMap::new())),
            rulesets: Arc::new(rulesets),
            maps: Arc::new(maps),
            map_documents: Arc::new(map_documents),
            layout_documents: Arc::new(layout_documents),
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

/// Mappe caricate e documenti JSON originali, indicizzati per id mappa.
type LoadedMaps = (
    BTreeMap<String, MapData>,
    BTreeMap<String, serde_json::Value>,
);

/// Carica le mappe referenziate dai ruleset da `data/maps/{map}.json`.
fn load_maps(data_dir: &Path, rulesets: &BTreeMap<String, Ruleset>) -> Result<LoadedMaps, String> {
    let maps_dir = data_dir.join("maps");
    let mut loaded = BTreeMap::new();
    let mut documents = BTreeMap::new();

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
        let document: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("parsing mappa {:?}: {}", path, e))?;
        loaded.insert(map_name.clone(), map);
        documents.insert(map_name.clone(), document);
    }

    if loaded.is_empty() {
        return Err(format!("nessuna mappa trovata in {:?}", maps_dir));
    }
    Ok((loaded, documents))
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

/// Carica i layout schematici da `data/layouts/{map}.json`, uno per mappa. Un layout
/// assente è ammesso (la mappa resta senza layout); uno presente deve coprire la mappa.
fn load_layouts(
    data_dir: &Path,
    maps: &BTreeMap<String, MapData>,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let layouts_dir = data_dir.join("layouts");
    let mut layouts = BTreeMap::new();

    for (map_name, map) in maps {
        let path = layouts_dir.join(format!("{}.json", map_name));
        let json = match std::fs::read_to_string(&path) {
            Ok(json) => json,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("lettura {:?}: {}", path, e)),
        };
        let layout: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("parsing layout {:?}: {}", path, e))?;
        validate_layout(map, &layout)
            .map_err(|e| format!("layout {:?} non valido: {}", path, e))?;
        layouts.insert(map_name.clone(), layout);
    }
    Ok(layouts)
}

/// Un layout deve avere `width`/`height` positivi, voci ben formate (come `isMapLayout` del
/// client) e coprire ogni città (posizione ed etichetta) e ogni rotta della mappa; le voci
/// in più sono ammesse.
fn validate_layout(map: &MapData, layout: &serde_json::Value) -> Result<(), String> {
    use serde_json::Value;

    let is_number = |v: Option<&Value>| v.is_some_and(Value::is_number);
    let obj = layout.as_object().ok_or("non è un oggetto")?;
    for key in ["width", "height"] {
        if !obj
            .get(key)
            .and_then(Value::as_f64)
            .is_some_and(|n| n > 0.0)
        {
            return Err(format!("'{key}' mancante o non positivo"));
        }
    }
    for key in ["unit", "doubleGap"] {
        if obj.get(key).is_some_and(|v| !v.is_number()) {
            return Err(format!("'{key}' non numerico"));
        }
    }
    if let Some(font) = obj.get("font") {
        let ok = font.get("family").is_some_and(Value::is_string)
            && is_number(font.get("weight"))
            && is_number(font.get("size"));
        if !ok {
            return Err("'font' senza family/weight/size".into());
        }
    }
    let section = |key: &str| {
        obj.get(key)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("'{key}' mancante o non è un oggetto"))
    };
    let cities = section("cities")?;
    let labels = section("labels")?;
    let routes = section("routes")?;

    for (id, p) in cities {
        if !(is_number(p.get("x")) && is_number(p.get("y"))) {
            return Err(format!("città '{id}' senza posizione {{x, y}}"));
        }
    }
    for (id, l) in labels {
        let anchor_ok = matches!(
            l.get("anchor").and_then(Value::as_str),
            Some("start" | "middle" | "end")
        );
        if !(is_number(l.get("dx")) && is_number(l.get("dy")) && anchor_ok) {
            return Err(format!(
                "città '{id}' con etichetta {{dx, dy, anchor}} non valida"
            ));
        }
    }
    for (id, pts) in routes {
        let ok = pts.as_array().is_some_and(|pts| {
            (2..=3).contains(&pts.len())
                && pts.iter().all(|p| {
                    p.as_array()
                        .is_some_and(|xy| xy.len() == 2 && xy.iter().all(Value::is_number))
                })
        });
        if !ok {
            return Err(format!("rotta '{id}' senza 2 o 3 punti [x, y]"));
        }
    }

    for city in &map.cities {
        if !cities.contains_key(&city.id) {
            return Err(format!("città '{}' senza posizione {{x, y}}", city.id));
        }
        if !labels.contains_key(&city.id) {
            return Err(format!("città '{}' senza etichetta", city.id));
        }
    }
    for route in &map.routes {
        if !routes.contains_key(&route.id) {
            return Err(format!("rotta '{}' senza 2 o 3 punti [x, y]", route.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn map() -> MapData {
        serde_json::from_value(json!({
            "cities": [{"id": "a", "name": "A"}, {"id": "b", "name": "B"}],
            "routes": [{"id": "a-b", "from": "a", "to": "b", "len": 2, "color": "Gray", "double": false}],
            "tickets": [],
        }))
        .expect("mappa di prova")
    }

    fn layout() -> Value {
        json!({
            "width": 100, "height": 50,
            "cities": {"a": {"x": 1, "y": 2}, "b": {"x": 3, "y": 4}},
            "labels": {"a": {"dx": 0, "dy": -10, "anchor": "middle"}, "b": {"dx": 0, "dy": 14, "anchor": "middle"}},
            "routes": {"a-b": [[1, 2], [3, 4]]},
        })
    }

    #[test]
    fn layout_covering_the_map_is_accepted() {
        assert_eq!(validate_layout(&map(), &layout()), Ok(()));
    }

    #[test]
    fn layout_missing_a_city_is_rejected() {
        let mut l = layout();
        l["cities"].as_object_mut().unwrap().remove("b");
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'b'"));

        let mut l = layout();
        l["labels"].as_object_mut().unwrap().remove("a");
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'a'"));
    }

    #[test]
    fn layout_missing_or_degenerate_route_is_rejected() {
        let mut l = layout();
        l["routes"].as_object_mut().unwrap().remove("a-b");
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'a-b'"));

        let mut l = layout();
        l["routes"]["a-b"] = json!([[1, 2]]);
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'a-b'"));

        let mut l = layout();
        l["routes"]["a-b"] = json!([[1, 2], [2, 3], [3, 4], [4, 5]]);
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'a-b'"));
    }

    #[test]
    fn layout_with_bad_size_label_or_font_is_rejected() {
        let mut l = layout();
        l["width"] = json!(0);
        assert!(validate_layout(&map(), &l).unwrap_err().contains("width"));

        let mut l = layout();
        l["labels"]["a"]["anchor"] = json!("left");
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'a'"));

        let mut l = layout();
        l["labels"]["b"] = json!({"dx": "0", "dy": 1, "anchor": "start"});
        assert!(validate_layout(&map(), &l).unwrap_err().contains("'b'"));

        let mut l = layout();
        l["font"] = json!({"family": "X"});
        assert!(validate_layout(&map(), &l).unwrap_err().contains("font"));
    }

    #[test]
    fn layout_with_extra_valid_entries_is_accepted() {
        let mut l = layout();
        l["cities"]["z"] = json!({"x": 5, "y": 6});
        l["labels"]["z"] = json!({"dx": 0, "dy": 1, "anchor": "end"});
        l["routes"]["z-z"] = json!([[1, 2], [3, 4]]);
        l["font"] = json!({"family": "X", "weight": 600, "size": 15.5});
        l["doubleGap"] = json!(5);
        assert_eq!(validate_layout(&map(), &l), Ok(()));
    }

    #[test]
    fn layout_without_numeric_size_is_rejected() {
        let mut l = layout();
        l["width"] = json!("100");
        assert!(validate_layout(&map(), &l).unwrap_err().contains("width"));
    }
}
