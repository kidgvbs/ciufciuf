//! CiufCiuf! — Validatore dati mappa e ruleset.
//!
//! Carica una mappa e un ruleset e verifica tutti i check del piano §4.2; la
//! coerenza tra ruleset, mappa e mazzi è la stessa che `ttr-core` applica
//! all'avvio di una partita (`Ruleset::validate`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use ttr_core::scoring::{MAX_CYCLOMATIC_NUMBER, UnionFind};
use ttr_core::types::{
    LONGEST_PATH_BONUS_TYPE, MAX_PARALLEL_LANES, MapData, Route, Ruleset, TieBreaker,
};

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
            match ruleset.validate(map) {
                Ok(()) => {
                    let estimate = ruleset.max_cyclomatic_estimate(map);
                    let path_needed = ruleset.bonus_points(LONGEST_PATH_BONUS_TYPE).is_some()
                        || ruleset.tie_breaker_keys().contains(&TieBreaker::LongestPath);
                    if path_needed {
                        println!(
                            "  Percorso più lungo: numero ciclomatico stimato {estimate} (massimo {MAX_CYCLOMATIC_NUMBER})"
                        );
                    } else {
                        println!(
                            "  Percorso più lungo: non applicato (nessun bonus né spareggio), numero ciclomatico stimato {estimate}"
                        );
                    }
                }
                Err(e) => r.err(format!("Ruleset: {e}")),
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
    r.check("Rotte parallele: max 4 per coppia, double marcato", |r| {
        let (total, groups, single_pairs) = check_double_routes(r, map);
        println!(
            "  Segmenti totali: {} ({} gruppi paralleli + {} altre coppie)",
            total, groups, single_pairs
        );
    });
    r.check(
        "Rotte di montagna: niente tunnel, X ≤ lunghezza, mountains nel ruleset",
        |r| {
            check_mountains(r, map, ruleset);
        },
    );
    r.check("Paesi: flag ISO alpha-2", |r| {
        check_flags(r, map);
    });
    r.check("Mappa: numero di segmenti plausibile", |r| {
        if map.routes.len() < 10 {
            r.err(format!("Solo {} segmenti — sospetto", map.routes.len()));
        }
    });

    r
}

/// Rotte di montagna: nessuna su un tunnel, simboli X (`mountains`) tra 1 e la lunghezza, e il
/// ruleset con `mountains.pointsPerTrain` ≥ 1 se e solo se la mappa ha montagne. Segnala ogni
/// rotta fuori regola, non solo la prima come `Ruleset::validate`.
fn check_mountains(r: &mut Report, map: &MapData, ruleset: &Ruleset) {
    let mountain_routes: Vec<&Route> = map.routes.iter().filter(|rt| rt.mountains > 0).collect();
    for rt in &mountain_routes {
        if rt.tunnel {
            r.err(format!("Rotta {}: montagna e tunnel insieme", rt.id));
        }
        if rt.mountains > rt.len {
            r.err(format!(
                "Rotta {}: {} X su una rotta lunga {}",
                rt.id, rt.mountains, rt.len
            ));
        }
    }
    match (&ruleset.mountains, mountain_routes.is_empty()) {
        (None, false) => {
            r.err("La mappa ha rotte di montagna ma il ruleset non ha mountains".into())
        }
        (Some(_), true) => {
            r.err("Il ruleset ha mountains ma la mappa non ha rotte di montagna".into())
        }
        (Some(config), false) if config.points_per_train == 0 => {
            r.err("mountains.pointsPerTrain deve essere almeno 1".into());
        }
        _ => {}
    }
    if !mountain_routes.is_empty() {
        let symbols: u32 = mountain_routes.iter().map(|rt| rt.mountains).sum();
        println!(
            "  Montagne: {} rotte, {} simboli X",
            mountain_routes.len(),
            symbols
        );
    }
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

/// Ogni biglietto deve collegare la partenza a ciascuna destinazione, raggiungibili sul
/// grafo delle rotte, con la stessa
/// semantica del motore: un Paese (`country`) è un vicolo cieco, quindi due rotte che vi
/// entrano non sono collegate tra loro, e un biglietto con estremo Paese è completato da
/// una qualunque rotta che vi entra.
fn check_ticket_reachability(r: &mut Report, map: &MapData) {
    let city_set: BTreeSet<&str> = map.cities.iter().map(|c| c.id.as_str()).collect();
    let countries: BTreeSet<&str> = map
        .cities
        .iter()
        .filter(|c| c.country)
        .map(|c| c.id.as_str())
        .collect();
    let node = |rt: &Route, city: &str| {
        if countries.contains(city) {
            format!("{city}#{}", rt.id)
        } else {
            city.to_string()
        }
    };
    let mut connectivity = UnionFind::new();
    for rt in &map.routes {
        connectivity.union(&node(rt, &rt.from), &node(rt, &rt.to));
    }

    for t in &map.tickets {
        for (to, _) in t.destinations() {
            // Le città inesistenti le segnala `Ruleset::validate`.
            if !city_set.contains(t.from.as_str()) || !city_set.contains(to) {
                continue;
            }
            if !connectivity.connected(&t.from, to) {
                r.err(format!(
                    "Biglietto '{}': {} → {} non raggiungibile",
                    t.id, t.from, to
                ));
            }
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

/// Al massimo `MAX_PARALLEL_LANES` rotte per coppia non ordinata di città; con più rotte
/// sulla stessa coppia tutte `double`, tranne quando un estremo è un Paese: ogni rotta è un
/// varco di confine a sé (il motore le dà un nodo privato ciascuna) e può essere `double: false`.
/// Restituisce (rotte, gruppi paralleli `double`, coppie senza gruppo parallelo).
fn check_double_routes(r: &mut Report, map: &MapData) -> (usize, usize, usize) {
    let countries: BTreeSet<&str> = map
        .cities
        .iter()
        .filter(|c| c.country)
        .map(|c| c.id.as_str())
        .collect();
    let mut pair_segments: BTreeMap<(&str, &str), Vec<&Route>> = BTreeMap::new();
    for rt in &map.routes {
        let key = if rt.from <= rt.to {
            (rt.from.as_str(), rt.to.as_str())
        } else {
            (rt.to.as_str(), rt.from.as_str())
        };
        pair_segments.entry(key).or_default().push(rt);
    }
    let mut groups = 0;
    let mut single_pairs = 0;
    for ((a, b), routes) in &pair_segments {
        let count = routes.len();
        let to_country = countries.contains(a) || countries.contains(b);
        let doubles = routes.iter().filter(|rt| rt.double).count();
        if count > MAX_PARALLEL_LANES {
            r.err(format!(
                "Coppia {}–{}: {} rotte (max {})",
                a, b, count, MAX_PARALLEL_LANES
            ));
        }
        if count >= 2 && doubles != 0 && doubles != count {
            r.err(format!(
                "Coppia {}–{}: {} rotte, mix di double e non double",
                a, b, count
            ));
        } else if count >= 2 && doubles == 0 && !to_country {
            r.err(format!(
                "Coppia {}–{}: {} rotte ma non tutte double=true",
                a, b, count
            ));
        }
        if count == 1 && routes[0].double {
            r.warn(format!(
                "Rotta {}: double=true ma unica tra {} e {}",
                routes[0].id, a, b
            ));
        }
        if doubles >= 2 {
            groups += 1;
        } else {
            single_pairs += 1;
        }
    }
    (map.routes.len(), groups, single_pairs)
}

/// `flag` (codice ISO 3166-1 alpha-2 maiuscolo) solo sui Paesi; un Paese senza `flag` è un avviso.
fn check_flags(r: &mut Report, map: &MapData) {
    for c in &map.cities {
        match (&c.flag, c.country) {
            (Some(f), true) => {
                let b = f.as_bytes();
                if b.len() != 2 || !b.iter().all(u8::is_ascii_uppercase) {
                    r.err(format!(
                        "Città {}: flag '{}' non è un codice ISO alpha-2 maiuscolo",
                        c.id, f
                    ));
                }
            }
            (Some(f), false) => r.err(format!(
                "Città {}: flag '{}' su una città che non è un Paese",
                c.id, f
            )),
            (None, true) => r.warn(format!("Paese {}: manca flag", c.id)),
            (None, false) => {}
        }
    }
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
    println!(
        "Modalità: {} (set '{}'), biglietti in gioco: {}",
        ruleset.mode,
        ruleset.ticket_set,
        map.tickets_in_play(&ruleset).count()
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
