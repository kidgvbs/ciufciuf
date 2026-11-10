//! Test della riga di comando del binario `ttr-server`.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ttr-server"))
        .args(args)
        .output()
        .expect("avvio di ttr-server")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn port_without_value_fails() {
    let out = run(&["--port"]);
    assert!(!out.status.success(), "--port senza valore deve fallire");
    assert!(stderr(&out).contains("--port"), "{}", stderr(&out));
}

#[test]
fn invalid_port_fails() {
    for bad in ["abc", "70000", "-1"] {
        let out = run(&["--port", bad]);
        assert!(!out.status.success(), "--port {bad} deve fallire");
    }
}

#[test]
fn invalid_bind_fails() {
    let out = run(&["--bind", "non-un-ip"]);
    assert!(!out.status.success());
}

#[test]
fn unknown_option_fails() {
    let out = run(&["--porta", "8080"]);
    assert!(
        !out.status.success(),
        "le opzioni sconosciute non vanno ignorate"
    );
}

#[test]
fn help_lists_the_options() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for option in [
        "--port",
        "--bind",
        "--data-dir",
        "--saves-dir",
        "--client-dist",
        "--resume",
    ] {
        assert!(text.contains(option), "manca {option} in --help:\n{text}");
    }
}
