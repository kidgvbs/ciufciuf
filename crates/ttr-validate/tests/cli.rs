//! Test del binario `ttr-validate` su dati reali e su ruleset/mappe incoerenti.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const MAP: &str = "../../data/maps/usa.json";
const RULESET: &str = "../../data/rulesets/usa_v2.json";

/// Directory temporanea rimossa a fine test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ttr-validate-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn write(&self, file: &str, json: &Value) -> PathBuf {
        let path = self.0.join(file);
        std::fs::write(&path, serde_json::to_vec(json).unwrap()).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn load(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn run(map: &Path, ruleset: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ttr-validate"))
        .arg(map)
        .arg(ruleset)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn ttr_validate_accepts_shipped_data() {
    let output = run(Path::new(MAP), Path::new(RULESET));
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("numero ciclomatico stimato"),
        "{}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("Modalità: Standard (set 'standard'), biglietti in gioco: 33"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn ttr_validate_rejects_bad_ruleset() {
    let dir = TempDir::new("bad-ruleset");
    let map = dir.write("map.json", &load(MAP));

    let mut ruleset = load(RULESET);
    ruleset["ticketDraw"]["deck"] = Value::from("long");
    let output = run(&map, &dir.write("ruleset.json", &ruleset));
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("ticketDraw.deck"),
        "{}",
        stdout(&output)
    );

    let mut ruleset = load(RULESET);
    ruleset["bonuses"][0]["type"] = Value::from("longestPath");
    let output = run(&map, &dir.write("ruleset.json", &ruleset));
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("'longestPath' sconosciuto"),
        "{}",
        stdout(&output)
    );

    let mut ruleset = load(RULESET);
    ruleset["initialTickets"]["keepMin"] = Value::from(9);
    let output = run(&map, &dir.write("ruleset.json", &ruleset));
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(stdout(&output).contains("keepMin"), "{}", stdout(&output));
}

#[test]
fn ttr_validate_rejects_non_canonical_route_color() {
    let dir = TempDir::new("bad-color");
    let ruleset = dir.write("ruleset.json", &load(RULESET));

    let mut map = load(MAP);
    map["routes"][0]["color"] = Value::from("grey");
    let output = run(&dir.write("map.json", &map), &ruleset);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(stdout(&output).contains("grey"), "{}", stdout(&output));
}

/// Una mutazione dei dati spediti con il messaggio d'errore atteso in stdout.
struct Case {
    name: &'static str,
    mutate: fn(&mut Value, &mut Value),
    expected: &'static str,
}

fn first_double_route(map: &Value) -> usize {
    map["routes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r["double"] == Value::Bool(true))
        .unwrap()
}

const CASES: &[Case] = &[
    Case {
        name: "route-unknown-city",
        mutate: |map, _| map["routes"][0]["to"] = Value::from("atlantis"),
        expected: "atlantis",
    },
    Case {
        name: "route-same-city",
        mutate: |map, _| {
            let from = map["routes"][0]["from"].clone();
            map["routes"][0]["to"] = from;
        },
        expected: "rotta 'seattle_vancouver_1': partenza e arrivo coincidono",
    },
    Case {
        name: "ticket-same-city",
        mutate: |map, _| {
            let from = map["tickets"][0]["from"].clone();
            map["tickets"][0]["to"] = from;
        },
        expected: "partenza e arrivo coincidono",
    },
    Case {
        name: "ticket-unknown-city",
        mutate: |map, _| map["tickets"][0]["to"] = Value::from("atlantis"),
        expected: "atlantis",
    },
    Case {
        name: "duplicate-ticket-id",
        mutate: |map, _| {
            let id = map["tickets"][0]["id"].clone();
            map["tickets"][1]["id"] = id;
        },
        expected: "id biglietto",
    },
    Case {
        name: "duplicate-city-id",
        mutate: |map, _| {
            let id = map["cities"][0]["id"].clone();
            map["cities"][1]["id"] = id;
        },
        expected: "id città",
    },
    Case {
        name: "duplicate-route-id",
        mutate: |map, _| {
            let id = map["routes"][0]["id"].clone();
            map["routes"][1]["id"] = id;
        },
        expected: "id rotta",
    },
    Case {
        name: "double-route-id-without-suffix",
        mutate: |map, _| {
            let i = first_double_route(map);
            map["routes"][i]["id"] = Value::from("double_without_suffix");
        },
        expected: "_1, _2, _3 o _4",
    },
    Case {
        name: "double-route-sister-not-double",
        mutate: |map, _| {
            let i = first_double_route(map);
            map["routes"][i]["double"] = Value::from(false);
        },
        expected: "gruppo parallelo",
    },
    Case {
        name: "route-length-zero",
        mutate: |map, _| map["routes"][0]["len"] = Value::from(0),
        expected: "almeno 1",
    },
    Case {
        name: "route-length-without-route-points",
        mutate: |_, ruleset| {
            ruleset["routePoints"].as_object_mut().unwrap().remove("6");
        },
        expected: "routePoints",
    },
    Case {
        name: "empty-ruleset-name",
        mutate: |_, ruleset| ruleset["name"] = Value::from(""),
        expected: "nome del ruleset",
    },
    Case {
        name: "zero-trains-per-player",
        mutate: |_, ruleset| ruleset["trainsPerPlayer"] = Value::from(0),
        expected: "trainsPerPlayer",
    },
    Case {
        name: "ticket-draw-count-zero",
        mutate: |_, ruleset| {
            ruleset["ticketDraw"]["count"] = Value::from(0);
            ruleset["ticketDraw"]["keepMin"] = Value::from(0);
        },
        expected: "ticketDraw.count",
    },
    Case {
        name: "empty-ruleset-mode",
        mutate: |_, ruleset| ruleset["mode"] = Value::from(""),
        expected: "modalità del ruleset",
    },
    Case {
        name: "empty-ticket-set",
        mutate: |_, ruleset| ruleset["ticketSet"] = Value::from("nessuno"),
        expected: "nessun biglietto della mappa è nel set 'nessuno'",
    },
    Case {
        name: "ticket-without-sets",
        mutate: |map, _| map["tickets"][0]["sets"] = Value::Array(vec![]),
        expected: "sets è vuoto",
    },
    Case {
        name: "ticket-duplicate-set",
        mutate: |map, _| map["tickets"][0]["sets"] = serde_json::json!(["standard", "standard"]),
        expected: "ripetuto in sets",
    },
    Case {
        name: "big-city-unknown",
        mutate: |_, ruleset| ruleset["bigCities"] = serde_json::json!(["atlantis"]),
        expected: "bigCities",
    },
    Case {
        name: "duplicate-bonus-type",
        mutate: |_, ruleset| {
            let bonus = ruleset["bonuses"][0].clone();
            ruleset["bonuses"].as_array_mut().unwrap().push(bonus);
        },
        expected: "ripetuto nel ruleset",
    },
    Case {
        name: "keep-max-above-count",
        mutate: |_, ruleset| {
            ruleset["initialTickets"]["dealt"][0]["keepMax"] = Value::from(5);
        },
        expected: "keepMax 5",
    },
    Case {
        name: "too-many-locomotives",
        mutate: |_, ruleset| ruleset["deck"]["perColor"] = Value::from(5),
        expected: "troppe locomotive",
    },
];

#[test]
fn ttr_validate_rejects_inconsistent_data() {
    let dir = TempDir::new("cases");
    for case in CASES {
        let (mut map, mut ruleset) = (load(MAP), load(RULESET));
        (case.mutate)(&mut map, &mut ruleset);
        let output = run(
            &dir.write(&format!("{}-map.json", case.name), &map),
            &dir.write(&format!("{}-ruleset.json", case.name), &ruleset),
        );
        let out = stdout(&output);
        assert_eq!(output.status.code(), Some(1), "{}: {out}", case.name);
        assert!(out.contains(case.expected), "{}: {out}", case.name);
        assert!(!out.contains("✅"), "{}: {out}", case.name);
    }
}

#[test]
fn ttr_validate_prints_check_mark_only_for_passed_checks() {
    let dir = TempDir::new("marks");
    let map = dir.write("map.json", &load(MAP));
    let mut ruleset = load(RULESET);
    ruleset["ticketDraw"]["count"] = Value::from(0);
    ruleset["ticketDraw"]["keepMin"] = Value::from(0);
    let output = run(&map, &dir.write("ruleset.json", &ruleset));
    let out = stdout(&output);
    assert!(
        out.contains("✗ Ruleset/mappa"),
        "il check fallito va segnato: {out}"
    );
    assert!(
        out.contains("✓ Biglietti: tutti raggiungibili"),
        "i check passati restano ✓: {out}"
    );
    assert!(!out.contains("✓ Ruleset/mappa"), "{out}");
}

#[test]
fn ttr_validate_huge_per_color_does_not_panic() {
    let dir = TempDir::new("huge");
    let map = dir.write("map.json", &load(MAP));
    let mut ruleset = load(RULESET);
    ruleset["deck"]["perColor"] = Value::from(u32::MAX);
    let output = run(&map, &dir.write("ruleset.json", &ruleset));
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

const GERMANY: (&str, &str) = (
    "../../data/maps/germany.json",
    "../../data/rulesets/germany.json",
);
const POLAND: (&str, &str) = (
    "../../data/maps/poland.json",
    "../../data/rulesets/poland.json",
);

#[test]
fn ttr_validate_accepts_germany_and_poland() {
    let germany = run(Path::new(GERMANY.0), Path::new(GERMANY.1));
    assert!(germany.status.success(), "{}", stdout(&germany));
    assert!(
        stdout(&germany)
            .contains("Modalità: Deutschland (set 'deutschland'), biglietti in gioco: 62"),
        "{}",
        stdout(&germany)
    );
    let poland = run(Path::new(POLAND.0), Path::new(POLAND.1));
    assert!(poland.status.success(), "{}", stdout(&poland));
    assert!(
        stdout(&poland).contains("Modalità: Standard (set 'standard'), biglietti in gioco: 35"),
        "{}",
        stdout(&poland)
    );
}

/// Ogni ruleset spedito passa con la sua mappa (USA, Europe, Basilicata, Germany, Poland).
#[test]
fn ttr_validate_accepts_every_shipped_ruleset() {
    let mut checked = 0;
    for entry in std::fs::read_dir("../../data/rulesets").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let ruleset = load(path.to_str().unwrap());
        let map = format!("../../data/maps/{}.json", ruleset["map"].as_str().unwrap());
        let output = run(Path::new(&map), &path);
        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            stdout(&output)
        );
        checked += 1;
    }
    assert!(checked >= 12, "ruleset controllati: {checked}");
}

/// Applica `mutate` ai dati `base` e verifica che il validatore li rifiuti con `expected`.
fn expect_rejected(
    base: (&str, &str),
    name: &str,
    mutate: impl FnOnce(&mut Value, &mut Value),
    expected: &str,
) {
    let dir = TempDir::new(name);
    let (mut map, mut ruleset) = (load(base.0), load(base.1));
    mutate(&mut map, &mut ruleset);
    let output = run(
        &dir.write("map.json", &map),
        &dir.write("ruleset.json", &ruleset),
    );
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{name}: {out}");
    assert!(out.contains(expected), "{name}: {out}");
    assert!(!out.contains("✅"), "{name}: {out}");
}

/// Un Paese è un vicolo cieco: due città raggiungibili solo attraverso lo stesso Paese
/// non sono collegate, a differenza di un Paese come estremo del biglietto.
#[test]
fn ttr_validate_treats_countries_as_dead_ends_for_tickets() {
    let island = |map: &mut Value, ticket_to: &str| {
        map["cities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "inselstadt", "name": "Inselstadt", "lat": 50.0, "lon": 8.0
            }));
        map["routes"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "frankreich_inselstadt_1", "from": "frankreich", "to": "inselstadt",
                "len": 1, "color": "Gray", "double": false, "tunnel": false, "ferries": 0
            }));
        map["tickets"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "freiburg_inselstadt", "from": "inselstadt", "to": ticket_to,
                "points": 5, "deck": "regular", "sets": ["deutschland"]
            }));
    };
    expect_rejected(
        GERMANY,
        "dead-end-ticket",
        |map, _| island(map, "freiburg"),
        "Biglietto 'freiburg_inselstadt': inselstadt → freiburg non raggiungibile",
    );

    // Con il Paese come estremo il biglietto è raggiungibile.
    let dir = TempDir::new("country-endpoint");
    let (mut map, ruleset) = (load(GERMANY.0), load(GERMANY.1));
    island(&mut map, "frankreich");
    let output = run(
        &dir.write("map.json", &map),
        &dir.write("ruleset.json", &ruleset),
    );
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn ttr_validate_rejects_inconsistent_new_rules() {
    type Mutation = fn(&mut Value, &mut Value);
    let germany: &[(&str, Mutation, &str)] = &[
        (
            "merge-deck-unknown",
            |_, rs| rs["ticketDraw"]["mergeDecks"] = serde_json::json!(["nessuno"]),
            "ticketDraw.mergeDecks: il deck 'nessuno' non è in ticketDecks",
        ),
        (
            "merge-deck-duplicate",
            |_, rs| rs["ticketDraw"]["mergeDecks"] = serde_json::json!(["long", "long"]),
            "il deck 'long' è ripetuto o coincide con ticketDraw.deck",
        ),
        (
            "merge-deck-is-draw-deck",
            |_, rs| rs["ticketDraw"]["mergeDecks"] = serde_json::json!(["regular"]),
            "il deck 'regular' è ripetuto o coincide con ticketDraw.deck",
        ),
        (
            "dealt-keep-min-above-count",
            |_, rs| rs["initialTickets"]["dealt"][1]["keepMin"] = Value::from(4),
            "keepMin 4 maggiore di count 3 per il deck 'regular'",
        ),
        (
            "dealt-keep-min-above-keep-max",
            |_, rs| {
                rs["initialTickets"]["dealt"][0]["keepMin"] = Value::from(2);
                rs["initialTickets"]["dealt"][0]["keepMax"] = Value::from(1);
            },
            "keepMin 2 maggiore di keepMax 1 per il deck 'long'",
        ),
        (
            "dealt-keep-min-sum-above-keep-min",
            |_, rs| {
                rs["initialTickets"]["dealt"][0]["keepMin"] = Value::from(1);
                rs["initialTickets"]["dealt"][1]["keepMin"] = Value::from(2);
            },
            "i keepMin dei deck sommano 3, più di initialTickets.keepMin 2",
        ),
        (
            "dealt-decks-too-small",
            |_, rs| rs["initialTickets"]["dealt"][1]["count"] = Value::from(20),
            "ne servono 100",
        ),
        (
            "draw-keep-min-above-count",
            |_, rs| rs["ticketDraw"]["keepMin"] = Value::from(5),
            "ticketDraw.keepMin 5",
        ),
        (
            "reshuffle-bottom-with-merge-decks",
            |_, rs| rs["setupDiscards"] = Value::from("reshuffleBottom"),
            "'reshuffleBottom' non è ammesso",
        ),
        (
            "unknown-tie-breaker",
            |_, rs| rs["tieBreakers"] = serde_json::json!(["stations"]),
            "chiave 'stations' sconosciuta",
        ),
        (
            "duplicate-tie-breaker",
            |_, rs| rs["tieBreakers"] = serde_json::json!(["completedTickets", "completedTickets"]),
            "chiave 'completedTickets' ripetuta",
        ),
        (
            "city-id-with-hash",
            |map, _| map["cities"][0]["id"] = Value::from("a#b"),
            "'#' è riservato",
        ),
        (
            "tunnel-with-any-cards-per-space",
            |map, _| {
                let route = &mut map["routes"][0];
                route["tunnel"] = Value::from(true);
                route["anyCardsPerSpace"] = Value::from(4);
            },
            "un tunnel non può avere anyCardsPerSpace",
        ),
    ];
    for (name, mutate, expected) in germany {
        expect_rejected(GERMANY, name, *mutate, expected);
    }

    expect_rejected(
        POLAND,
        "country-cards-unknown-country",
        |_, rs| rs["countryCards"]["krakow"] = serde_json::json!([3]),
        "'krakow' non è un Paese",
    );
    expect_rejected(
        POLAND,
        "country-cards-not-a-country-flag",
        |map, _| {
            let cities = map["cities"].as_array_mut().unwrap();
            let city = cities.iter_mut().find(|c| c["id"] == "niemcy").unwrap();
            city["country"] = Value::from(false);
        },
        "'niemcy' non è un Paese",
    );
    expect_rejected(
        POLAND,
        "always-open-mismatch",
        |map, _| {
            let routes = map["routes"].as_array_mut().unwrap();
            let lane = routes
                .iter_mut()
                .find(|r| r["id"] == "bialorus_siedlce_1")
                .unwrap();
            lane["alwaysOpen"] = Value::from(false);
        },
        "alwaysOpen diverso",
    );
    expect_rejected(
        POLAND,
        "parallel-group-too-large",
        |map, _| {
            let routes = map["routes"].as_array_mut().unwrap();
            let mut fifth = routes
                .iter()
                .find(|r| r["id"] == "czechy_wroclaw_4")
                .unwrap()
                .clone();
            fifth["id"] = Value::from("czechy_wroclaw_5");
            routes.push(fifth);
        },
        "il gruppo ha 5 binari, il massimo è 4",
    );
    expect_rejected(
        POLAND,
        "parallel-group-gap",
        |map, _| {
            let routes = map["routes"].as_array_mut().unwrap();
            routes.retain(|r| r["id"] != "czechy_wroclaw_2");
        },
        "serve 'czechy_wroclaw_2'",
    );
}
