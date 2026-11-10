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
        stdout(&output).contains("longest_path"),
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
