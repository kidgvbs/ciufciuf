//! Test del binario `ttr-cli`: argomenti mancanti o non validi non provocano panic.

use std::process::{Command, Output};

const MAP: &str = "../../data/maps/usa.json";
const RULESET: &str = "../../data/rulesets/usa_v2.json";

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ttr-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn assert_usage_error(args: &[&str]) {
    let output = run(args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
    assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
    assert!(stderr.contains("Errore"), "{args:?}: {stderr}");
}

#[test]
fn ttr_cli_missing_arg_exits_2() {
    assert_usage_error(&[]);
    assert_usage_error(&["--map"]);
    assert_usage_error(&["--ruleset"]);
    assert_usage_error(&["--map", MAP]);
    assert_usage_error(&["--map", MAP, "--ruleset"]);
    assert_usage_error(&["--map", MAP, "--ruleset", RULESET, "--players"]);
    assert_usage_error(&["--map", MAP, "--ruleset", RULESET, "--seed"]);
    assert_usage_error(&["--map", MAP, "--ruleset", RULESET, "--replay"]);
}

#[test]
fn ttr_cli_prints_initial_state() {
    let output = run(&["--map", MAP, "--ruleset", RULESET, "--players", "2"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Giocatori: 2"));
}
