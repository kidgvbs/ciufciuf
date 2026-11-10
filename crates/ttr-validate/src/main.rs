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
    /// Nome di ogni check eseguito e se è passato.
    checks: Vec<(&'static str, bool)>,
}

impl Report {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            checks: Vec::new(),
        }
    }

    fn err(&mut self, msg: String) {
        self.errors.push(msg);
    }

    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Esegue un check e lo registra come passato solo se non ha aggiunto errori.
    fn check(&mut self, name: &'static str, run: impl FnOnce(&mut Self)) {
        let before = self.errors.len();
        run(self);
        self.checks.push((name, self.errors.len() == before));
    }
}

fn validate(map: &MapData, ruleset: &Ruleset) -> Report {
    let mut r = Report::new();

    r.check(
        "Ruleset/mappa: id, città, rotte, biglietti, mazzi, giocatori",
        |r| {
            if let Err(e) = ruleset.validate(map) {
                r.err(format!("Ruleset: {e}"));
            }
            warn_unused_route_points(r, map, ruleset);
        },
    );
    r.check("Biglietti: tutti raggiungibili sul grafo", |r| {
        check_ticket_reachability(r, map);
    });
    r.check("Mazzo treno: conteggi coerenti", |r| {
        check_deck_counts(r, ruleset);
    });
    r.check("Rotte doppie: max 2 per coppia, double marcato", |r| {
        let (total, double_pairs, single_pairs) = check_double_routes(r, map);
        println!(
            "  Segmenti totali: {} ({} doppie + {} singole)",
            total, double_pairs, single_pairs
        );
    });
    r.check("Mappa: numero di segmenti plausibile", |r| {
        if map.routes.len() < 10 {
            r.err(format!("Solo {} segmenti — sospetto", map.routes.len()));
        }
    });

    r
}

/// Avvisa delle lunghezze di `routePoints` che nessuna rotta usa.
fn warn_unused_route_points(r: &mut Report, map: &MapData, ruleset: &Ruleset) {
    let lengths: BTreeSet<u32> = map.routes.iter().map(|rt| rt.len).collect();
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
}

/// Ogni biglietto deve collegare due città raggiungibili sul grafo delle rotte.
fn check_ticket_reachability(r: &mut Report, map: &MapData) {
    let city_set: BTreeSet<&str> = map.cities.iter().map(|c| c.id.as_str()).collect();
    let mut route_graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for rt in &map.routes {
        route_graph.entry(&rt.from).or_default().push(&rt.to);
        route_graph.entry(&rt.to).or_default().push(&rt.from);
    }

    for t in &map.tickets {
        // Le città inesistenti le segnala `Ruleset::validate`.
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
}

/// Il mazzo treno completo ha 110 carte (USA); un totale che non entra in `u32` è un errore.
fn check_deck_counts(r: &mut Report, ruleset: &Ruleset) {
    let deck = &ruleset.deck;
    let total = deck
        .per_color
        .checked_mul(8)
        .and_then(|colored| colored.checked_add(deck.locomotives));
    match total {
        None => r.err(format!(
            "Mazzo treno: {}×8 + {} non entra in u32",
            deck.per_color, deck.locomotives
        )),
        Some(110) => {}
        Some(total) => r.err(format!(
            "Mazzo treno: {}×8 + {} = {} (atteso 110)",
            deck.per_color, deck.locomotives, total
        )),
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
}

/// Max 2 rotte per coppia di città, entrambe `double`. Restituisce (rotte, coppie doppie,
/// coppie singole).
fn check_double_routes(r: &mut Report, map: &MapData) -> (usize, usize, usize) {
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
        if count == 2 && !routes.iter().all(|rt| rt.double) {
            r.err(format!(
                "Coppia {}–{}: 2 rotte ma non entrambe double=true",
                a, b
            ));
        }
        if count == 1 && routes[0].double {
            r.warn(format!(
                "Rotta {}: double=true ma unica tra {} e {}",
                routes[0].id, a, b
            ));
        }
    }
    let double_pairs = pair_segments.values().filter(|v| v.len() == 2).count();
    let single_pairs = pair_segments.values().filter(|v| v.len() == 1).count();
    (map.routes.len(), double_pairs, single_pairs)
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
    for (name, passed) in &report.checks {
        println!("  {} {}", if *passed { "✓" } else { "✗" }, name);
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
