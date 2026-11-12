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
        expected: "_1 o _2",
    },
    Case {
        name: "double-route-length-mismatch",
        mutate: |map, _| {
            let i = first_double_route(map);
            map["routes"][i]["len"] = Value::from(6);
        },
        expected: "gemella",
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
