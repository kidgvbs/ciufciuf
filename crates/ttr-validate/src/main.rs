//! CiufCiuf! — Validatore dati mappa e ruleset.
//!
//! Carica una mappa e un ruleset e verifica tutti i check del piano §4.2; la
//! coerenza tra ruleset, mappa e mazzi è la stessa che `ttr-core` applica
//! all'avvio di una partita (`Ruleset::validate`).

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;

use ttr_core::types::{MapData, Route, Ruleset};

// ── Validazione ─────────────────────────────────────────────────────

struct Report {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl Report {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
    fn err(&mut self, msg: String) {
        self.errors.push(msg);
    }
    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }
}

fn validate(map: &MapData, ruleset: &Ruleset) -> Report {
    let mut r = Report::new();
    let city_set: BTreeSet<&str> = map.cities.iter().map(|c| c.id.as_str()).collect();

    // Build route graph for connectivity
    let mut route_graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for rt in &map.routes {
        route_graph.entry(&rt.from).or_default().push(&rt.to);
        route_graph.entry(&rt.to).or_default().push(&rt.from);
    }

    // ── Check 1: cities in routes/tickets exist ──
    for rt in &map.routes {
        if !city_set.contains(rt.from.as_str()) {
            r.err(format!("Rotta '{}': città '{}' non esiste", rt.id, rt.from));
        }
        if !city_set.contains(rt.to.as_str()) {
            r.err(format!("Rotta '{}': città '{}' non esiste", rt.id, rt.to));
        }
    }
    for t in &map.tickets {
        if !city_set.contains(t.from.as_str()) {
            r.err(format!(
                "Biglietto '{}': città '{}' non esiste",
                t.id, t.from
            ));
        }
        if !city_set.contains(t.to.as_str()) {
            r.err(format!("Biglietto '{}': città '{}' non esiste", t.id, t.to));
        }
    }

    // ── Check 2: ticket reachability (BFS) ──
    for t in &map.tickets {
        if !city_set.contains(t.from.as_str()) || !city_set.contains(t.to.as_str()) {
            continue;
        }
        let mut visited: BTreeSet<&str> = BTreeSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        visited.insert(&t.from);
        queue.push_back(&t.from);
        let mut found = false;
        while let Some(city) = queue.pop_front() {
            if city == t.to.as_str() {
                found = true;
                break;
            }
            if let Some(neighbors) = route_graph.get(city) {
                for &n in neighbors {
                    if visited.insert(n) {
                        queue.push_back(n);
                    }
                }
            }
        }
        if !found {
            r.err(format!(
                "Biglietto '{}': {} → {} non raggiungibile",
                t.id, t.from, t.to
            ));
        }
    }

    // ── Check 3: deck counts ──
    let deck = &ruleset.deck;
    let total = deck.per_color * 8 + deck.locomotives;
    if total != 110 {
        r.err(format!(
            "Mazzo treno: {}×8 + {} = {} (atteso 110)",
            deck.per_color, deck.locomotives, total
        ));
    }
    if deck.per_color != 12 {
        r.warn(format!("perColor={} (atteso 12 per USA)", deck.per_color));
    }
    if deck.locomotives != 14 {
        r.warn(format!(
            "locomotives={} (atteso 14 per USA)",
            deck.locomotives
        ));
    }

    // ── Check 4: coerenza ruleset/mappa (stessa di Engine::new) ──
    if let Err(e) = ruleset.validate(map) {
        r.err(format!("Ruleset: {e}"));
    }

    // ── Check 5: route lengths in routePoints ──
    let lengths: BTreeSet<u32> = map.routes.iter().map(|rt| rt.len).collect();
    for len in &lengths {
        let key = len.to_string();
        if !ruleset.route_points.contains_key(&key) {
            r.err(format!(
                "Lunghezza {} presente nella mappa ma non in routePoints",
                len
            ));
        }
    }
    for key in ruleset.route_points.keys() {
        if let Ok(len) = key.parse::<u32>()
            && !lengths.contains(&len)
        {
            r.warn(format!(
                "routePoints ha lunghezza {} non presente nella mappa",
                len
            ));
        }
    }

    // ── Check 6: double routes ──
    let mut pair_segments: BTreeMap<(&str, &str), Vec<&Route>> = BTreeMap::new();
    for rt in &map.routes {
        pair_segments
            .entry((&rt.from, &rt.to))
            .or_default()
            .push(rt);
    }
    for ((a, b), routes) in &pair_segments {
        let count = routes.len();
        if count > 2 {
            r.err(format!("Coppia {}–{}: {} rotte (max 2)", a, b, count));
        }
        if count == 2 {
            let both = routes.iter().all(|rt| rt.double);
            if !both {
                r.err(format!(
                    "Coppia {}–{}: 2 rotte ma non entrambe double=true",
                    a, b
                ));
            }
        }
        if count == 1 && routes[0].double {
            r.warn(format!(
                "Rotta {}: double=true ma unica tra {} e {}",
                routes[0].id, a, b
            ));
        }
    }

    // ── Check 7: total segments ──
    let total = map.routes.len();
    let double_pairs = pair_segments.values().filter(|v| v.len() == 2).count();
    let single_pairs = pair_segments.values().filter(|v| v.len() == 1).count();
    println!(
        "  Segmenti totali: {} ({} doppie + {} singole)",
        total, double_pairs, single_pairs
    );
    if total < 10 {
        r.err(format!("Solo {} segmenti — sospetto", total));
    }

    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Uso: ttr-validate <map.json> <ruleset.json>");
        std::process::exit(2);
    }
    let map_path = PathBuf::from(&args[1]);
    let ruleset_path = PathBuf::from(&args[2]);

    let map_json = fs::read_to_string(&map_path).unwrap_or_else(|e| {
        eprintln!("Errore: {}: {}", map_path.display(), e);
        std::process::exit(1);
    });
    let ruleset_json = fs::read_to_string(&ruleset_path).unwrap_or_else(|e| {
        eprintln!("Errore: {}: {}", ruleset_path.display(), e);
        std::process::exit(1);
    });
    let map: MapData = ttr_core::load_map_from_json(&map_json).unwrap_or_else(|e| {
        eprintln!("Errore parsing mappa: {}", e);
        std::process::exit(1);
    });
    let ruleset: Ruleset = ttr_core::load_ruleset_from_json(&ruleset_json).unwrap_or_else(|e| {
        eprintln!("Errore parsing ruleset: {}", e);
        std::process::exit(1);
    });

    println!("=== CiufCiuf! Validatore mappa/ruleset ===");
    println!("Mappa: {}", map_path.display());
    println!("Ruleset: {}", ruleset_path.display());
    println!(
        "Città: {}, Rotte: {}, Biglietti: {}",
        map.cities.len(),
        map.routes.len(),
        map.tickets.len()
    );

    let report = validate(&map, &ruleset);

    println!();
    println!("--- Check ---");
    let checks = [
        "Città: tutte esistono per rotte e biglietti",
        "Biglietti: tutti raggiungibili sul grafo",
        "Mazzo treno: conteggi coerenti",
        "Ruleset/mappa: colori rotte, bonus, mazzi biglietti, giocatori",
        "RoutePoints: ogni lunghezza rotta ha entry",
        "Rotte doppie: max 2 per coppia, double marcato",
    ];
    for c in &checks {
        println!("  ✓ {}", c);
    }

    for w in &report.warnings {
        println!("  ⚠ {}", w);
    }

    if !report.errors.is_empty() {
        println!();
        println!("--- ERRORI ---");
        for e in &report.errors {
            println!("  ✗ {}", e);
        }
        std::process::exit(1);
    }

    println!();
    println!("✅ Validazione completata con successo.");
}
