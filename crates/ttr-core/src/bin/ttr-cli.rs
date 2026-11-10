//! `ttr-cli` — CLI di debug per il motore CiufCiuf! (piano §10, Fase 1).
//!
//! Crea una partita da data/ + seed, applica azioni da file o stdin,
//! stampa stato ed eventi.
//!
//! Uso:
//!   ttr-cli --map data/maps/usa.json --ruleset data/rulesets/usa_v2.json \
//!            --players 3 --seed 42 [--replay replay.json]

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ttr_core::{
    Action, DrawSource, Engine, default_player_colors, load_map_from_json, load_ruleset_from_json,
};

/// Azione serializzata per replay file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayAction {
    #[serde(rename = "type")]
    action_type: String,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    player_index: Option<usize>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    keep: Option<Vec<String>>,
    #[serde(default)]
    route_id: Option<String>,
    #[serde(default)]
    cards_discarded: Option<Vec<String>>,
}

/// Stampa un errore d'uso ed esce con codice 2.
fn usage_error(message: &str) -> ! {
    eprintln!("Errore: {message}");
    std::process::exit(2);
}

/// Valore che segue il flag in `args[*i]`; avanza `*i` oltre il valore.
fn flag_value<'a>(args: &'a [String], i: &mut usize) -> &'a str {
    let flag = &args[*i];
    *i += 1;
    match args.get(*i) {
        Some(value) => value,
        None => usage_error(&format!("manca il valore dopo {flag}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parsing semplice degli argomenti
    let mut map_path: Option<PathBuf> = None;
    let mut ruleset_path: Option<PathBuf> = None;
    let mut num_players: usize = 3;
    let mut seed: u64 = 42;
    let mut first_player: usize = 0;
    let mut replay_path: Option<PathBuf> = None;
    let mut interactive = false;

    let mut i = 1;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--map" => map_path = Some(PathBuf::from(flag_value(&args, &mut i))),
            "--ruleset" => ruleset_path = Some(PathBuf::from(flag_value(&args, &mut i))),
            "--players" => num_players = flag_value(&args, &mut i).parse().unwrap_or(3),
            "--seed" => seed = flag_value(&args, &mut i).parse().unwrap_or(42),
            "--first" => first_player = flag_value(&args, &mut i).parse().unwrap_or(0),
            "--replay" => replay_path = Some(PathBuf::from(flag_value(&args, &mut i))),
            "--interactive" | "-i" => {
                interactive = true;
            }
            _ => {
                eprintln!("Argomento sconosciuto: {flag}");
            }
        }
        i += 1;
    }

    let map_path = map_path.unwrap_or_else(|| usage_error("Specifica --map <file.json>"));
    let ruleset_path =
        ruleset_path.unwrap_or_else(|| usage_error("Specifica --ruleset <file.json>"));

    let map_json = fs::read_to_string(&map_path).unwrap_or_else(|e| {
        eprintln!("Errore leggendo {}: {}", map_path.display(), e);
        std::process::exit(1);
    });
    let ruleset_json = fs::read_to_string(&ruleset_path).unwrap_or_else(|e| {
        eprintln!("Errore leggendo {}: {}", ruleset_path.display(), e);
        std::process::exit(1);
    });

    let map = load_map_from_json(&map_json).unwrap_or_else(|e| {
        eprintln!("Errore parsing mappa: {}", e);
        std::process::exit(1);
    });
    let ruleset = load_ruleset_from_json(&ruleset_json).unwrap_or_else(|e| {
        eprintln!("Errore parsing ruleset: {}", e);
        std::process::exit(1);
    });

    let player_ids: Vec<String> = (0..num_players).map(|i| format!("p{}", i + 1)).collect();
    let player_names: Vec<String> = (0..num_players)
        .map(|i| format!("Giocatore {}", i + 1))
        .collect();
    let colors = default_player_colors(num_players);

    let mut engine = Engine::new(
        seed,
        ruleset,
        map,
        player_ids,
        player_names,
        colors,
        first_player,
    )
    .unwrap_or_else(|e| {
        eprintln!("Errore creando la partita: {}", e);
        std::process::exit(1);
    });

    println!("=== CiufCiuf! CLI ===");
    println!("Seed: {}, Giocatori: {}", seed, num_players);
    println!("Fase: {:?}", engine.phase());
    println!();

    if let Some(replay) = replay_path {
        run_replay(&mut engine, &replay);
    } else if interactive {
        run_interactive(&mut engine);
    } else {
        print_state(&engine);
    }
}

fn print_state(engine: &Engine) {
    println!("--- Stato ---");
    println!("Fase: {:?}", engine.phase());
    println!("Giocatore corrente: {}", engine.current_player());
    println!("Last round: {}", engine.last_round());
    if let Some(t) = engine.last_round_trigger_player() {
        println!("  Trigger player: {}", t);
    }
    println!();

    println!("Face-up:");
    for (i, card) in engine.face_up_cards().iter().enumerate() {
        match card {
            Some(c) => println!("  [{}] {:?}", i, c),
            None => println!("  [{}] (vuoto)", i),
        }
    }
    println!(
        "Mazzo: {} carte, Scarti: {}",
        engine.draw_pile_len(),
        engine.discards_len()
    );
    println!();

    for pi in 0..engine.num_players() {
        if let Some(player) = engine.player(pi) {
            println!(
                "P{}: {} treni, {} punti, {} carte in mano, {} biglietti",
                pi + 1,
                player.trains_remaining,
                player.score,
                player.hand.len(),
                player.tickets.len()
            );
            let lm = engine.legal_moves(pi);
            println!(
                "  Mosse legali: {} rotte, face-up: {:?}, deck: {}, tickets: {}",
                lm.routes.len(),
                lm.draw_face_up,
                lm.can_draw_deck,
                lm.can_draw_tickets
            );
        }
    }
    println!();

    println!("Rotte reclamate ({}):", engine.claimed_route_ids().len());
    for rid in engine.claimed_route_ids() {
        println!("  {}", rid);
    }
}

fn run_replay(engine: &mut Engine, path: &PathBuf) {
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Errore: {}: {}", path.display(), e);
        std::process::exit(1);
    });

    let actions: Vec<ReplayAction> = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Errore parsing replay: {}", e);
        std::process::exit(1);
    });

    for (step, ra) in actions.iter().enumerate() {
        let action = parse_replay_action(ra, engine).unwrap_or_else(|e| {
            eprintln!("Step {}: ERRORE: {}", step + 1, e);
            std::process::exit(1);
        });
        match engine.apply(action) {
            Ok(events) => {
                println!("Step {}: OK ({:?})", step + 1, ra.action_type);
                for ev in &events {
                    println!("  -> {:?}", ev);
                }
            }
            Err(e) => {
                eprintln!("Step {}: ERRORE: {}", step + 1, e);
                std::process::exit(1);
            }
        }
    }

    println!();
    print_state(engine);
}

fn parse_replay_action(ra: &ReplayAction, engine: &Engine) -> Result<Action, String> {
    let pi = ra.player_index.unwrap_or(engine.current_player());
    match ra.action_type.as_str() {
        "INITIAL_DRAFT_CHOOSE" | "CHOOSE_TICKETS" => Ok(Action::ChooseTickets {
            player_index: pi,
            keep: ra.keep.clone().unwrap_or_default(),
        }),
        "DRAW_TICKETS" => Ok(Action::DrawTickets { player_index: pi }),
        "DRAW_DECK" => Ok(Action::DrawTrainCard {
            player_index: pi,
            source: DrawSource::Deck,
        }),
        "DRAW_FACEUP" => {
            let idx = ra.index.unwrap_or(0);
            Ok(Action::DrawTrainCard {
                player_index: pi,
                source: DrawSource::FaceUp { index: idx },
            })
        }
        "CLAIM_ROUTE" => {
            let cards: Result<Vec<_>, String> = ra
                .cards_discarded
                .iter()
                .flatten()
                .map(|s| parse_card_color(s))
                .collect();
            Ok(Action::ClaimRoute {
                player_index: pi,
                route_id: ra.route_id.clone().unwrap_or_default(),
                cards_discarded: cards?,
            })
        }
        other => Err(format!("Azione sconosciuta: {other}")),
    }
}

fn parse_card_color(s: &str) -> Result<ttr_proto::CardColor, String> {
    use ttr_proto::CardColor;
    Ok(match s.to_uppercase().as_str() {
        "RED" => CardColor::Red,
        "ORANGE" => CardColor::Orange,
        "YELLOW" => CardColor::Yellow,
        "GREEN" => CardColor::Green,
        "BLUE" => CardColor::Blue,
        "PURPLE" => CardColor::Purple,
        "BLACK" => CardColor::Black,
        "WHITE" => CardColor::White,
        "LOCOMOTIVE" | "LOCO" => CardColor::Locomotive,
        _ => return Err(format!("Colore sconosciuto: {s}")),
    })
}

fn run_interactive(engine: &mut Engine) {
    let stdin = io::stdin();
    print_state(engine);

    loop {
        if matches!(engine.phase(), ttr_core::error::GamePhase::GameOver) {
            println!("GAME OVER!");
            break;
        }

        print!("\n> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break;
        }
        let line = line.trim().to_string();
        if line.is_empty() || line == "q" {
            break;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let result = match parts[0] {
            "state" | "s" => {
                print_state(engine);
                Ok(vec![])
            }
            "legal" | "l" => {
                let pi = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(engine.current_player());
                let lm = engine.legal_moves(pi);
                println!(
                    "P{}: rotte={}, face-up={:?}, deck={}, tickets={}",
                    pi + 1,
                    lm.routes.len(),
                    lm.draw_face_up,
                    lm.can_draw_deck,
                    lm.can_draw_tickets
                );
                for r in &lm.routes {
                    println!("  {}", r);
                }
                Ok(vec![])
            }
            "draw" | "d" => {
                if parts.len() < 2 {
                    Err(ttr_core::EngineError::InvalidPayload {
                        reason: "usa: draw deck|faceup <n>".into(),
                    })
                } else {
                    match parts[1] {
                        "deck" => engine.apply(Action::DrawTrainCard {
                            player_index: engine.current_player(),
                            source: DrawSource::Deck,
                        }),
                        "faceup" | "f" => {
                            let idx = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                            engine.apply(Action::DrawTrainCard {
                                player_index: engine.current_player(),
                                source: DrawSource::FaceUp { index: idx },
                            })
                        }
                        "tickets" | "t" => engine.apply(Action::DrawTickets {
                            player_index: engine.current_player(),
                        }),
                        _ => Err(ttr_core::EngineError::InvalidPayload {
                            reason: "sconosciuto".into(),
                        }),
                    }
                }
            }
            "keep" | "k" => {
                let keep: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                engine.apply(Action::ChooseTickets {
                    player_index: engine.current_player(),
                    keep,
                })
            }
            "initkeep" | "ik" => {
                let pi = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                let keep: Vec<String> = parts
                    .get(2..)
                    .unwrap_or_default()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                engine.apply(Action::ChooseTickets {
                    player_index: pi,
                    keep,
                })
            }
            "claim" | "c" => {
                if parts.len() < 3 {
                    Err(ttr_core::EngineError::InvalidPayload {
                        reason: "usa: claim <route_id> <colori...>".into(),
                    })
                } else {
                    let rid = parts[1].to_string();
                    let cards: Result<Vec<ttr_proto::CardColor>, String> =
                        parts[2..].iter().map(|s| parse_card_color(s)).collect();
                    match cards {
                        Ok(cards_discarded) => engine.apply(Action::ClaimRoute {
                            player_index: engine.current_player(),
                            route_id: rid,
                            cards_discarded,
                        }),
                        Err(reason) => Err(ttr_core::EngineError::InvalidPayload { reason }),
                    }
                }
            }
            _ => {
                println!(
                    "Comandi: state, legal [p], draw deck|faceup [n]|tickets, keep <ids>, initkeep <p> <ids>, claim <rid> <colori>, q"
                );
                Ok(vec![])
            }
        };

        match result {
            Ok(events) => {
                for ev in &events {
                    println!("  EV: {:?}", ev);
                }
            }
            Err(e) => {
                println!("  ERR: {}", e);
            }
        }
    }
}
