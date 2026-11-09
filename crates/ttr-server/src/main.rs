//! Entry point CLI: parsing argomenti, caricamento dati, avvio server.
//!
//! Uso: `ttr-server --port 8080 --data-dir ./data --saves-dir ./saves [--resume <file>]`

use std::net::SocketAddr;
use std::path::PathBuf;

use tracing::info;
use tracing_subscriber::EnvFilter;

use ttr_server::{AppState, build_router, resume_game};

/// Argomenti da riga di comando.
struct Args {
    port: u16,
    data_dir: PathBuf,
    saves_dir: PathBuf,
    client_dist: PathBuf,
    resume: Option<PathBuf>,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let mut port = 8080u16;
    let mut data_dir = PathBuf::from("data");
    let mut saves_dir = PathBuf::from("saves");
    let mut client_dist = PathBuf::from("../client/dist");
    let mut resume = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args[i].parse().unwrap_or(8080);
            }
            "--data-dir" => {
                i += 1;
                data_dir = PathBuf::from(&args[i]);
            }
            "--saves-dir" => {
                i += 1;
                saves_dir = PathBuf::from(&args[i]);
            }
            "--client-dist" => {
                i += 1;
                client_dist = PathBuf::from(&args[i]);
            }
            "--resume" => {
                i += 1;
                resume = Some(PathBuf::from(&args[i]));
            }
            _ => {}
        }
        i += 1;
    }

    Args {
        port,
        data_dir,
        saves_dir,
        client_dist,
        resume,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = parse_args();

    // Crea directory saves se non esiste
    if let Err(e) = std::fs::create_dir_all(&args.saves_dir) {
        eprintln!(
            "ERRORE: impossibile creare saves dir {:?}: {}",
            args.saves_dir, e
        );
        std::process::exit(1);
    }

    let state = match AppState::new(args.data_dir.clone(), args.saves_dir.clone(), args.port) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERRORE: impossibile inizializzare AppState: {}", e);
            std::process::exit(1);
        }
    };

    // Flusso di resume (D18 §8): carica il save, ricostruisce il motore,
    // registra la partita in lobby col roster salvato.
    if let Some(ref resume_path) = args.resume {
        match resume_game(&state, resume_path).await {
            Ok((game_id, host_token)) => {
                info!(
                    "Resume: partita {} ripristinata dalla lobby. hostToken={}",
                    game_id, host_token
                );
            }
            Err(e) => {
                eprintln!("ERRORE resume da {:?}: {}", resume_path, e);
                std::process::exit(1);
            }
        }
    }

    let router = build_router(state, args.client_dist);
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERRORE: impossibile bindare {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    info!("CiufCiuf! server in ascolto su http://{}", addr);

    if let Err(e) = axum::serve(listener, router).await {
        eprintln!("ERRORE server: {}", e);
        std::process::exit(1);
    }
}
