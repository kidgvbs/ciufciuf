//! Entry point CLI: parsing argomenti, caricamento dati, avvio server.
//!
//! Uso: `ttr-server --port 8080 --data-dir ./data --saves-dir ./saves [--resume <file>]`

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use ttr_server::{AppState, build_router, resume_game, shutdown_sessions};

/// Attesa massima per le scritture in coda alla chiusura.
const SHUTDOWN_FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Argomenti da riga di comando.
#[derive(Parser, Debug)]
#[command(name = "ttr-server", version, about = "CiufCiuf! — server di gioco")]
struct Args {
    /// Porta TCP su cui ascoltare.
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Indirizzo IP su cui ascoltare.
    #[arg(long, default_value = "0.0.0.0")]
    bind: IpAddr,

    /// Cartella con mappe e ruleset.
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,

    /// Cartella con save, profili e risultati.
    #[arg(long, default_value = "saves")]
    saves_dir: PathBuf,

    /// Cartella con i file statici del client.
    #[arg(long, default_value = "../client/dist")]
    client_dist: PathBuf,

    /// Ripristina la partita di questo save.
    #[arg(long, value_name = "FILE")]
    resume: Option<PathBuf>,
}

/// Completa all'arrivo di SIGINT o SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("segnale di chiusura ricevuto");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

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

    // Flusso di resume: carica il save, ricostruisce il motore,
    // registra la partita in lobby col roster salvato.
    if let Some(resume_path) = &args.resume {
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

    let router = build_router(state.clone(), args.client_dist);
    let addr = SocketAddr::new(args.bind, args.port);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERRORE: impossibile bindare {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    info!("CiufCiuf! server in ascolto su http://{}", addr);

    // Le partite si fermano prima del rilascio delle connessioni: i WebSocket
    // si chiudono quando il loro attore termina.
    let shutdown_state = state.clone();
    let served = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown_sessions(&shutdown_state, SHUTDOWN_FLUSH_TIMEOUT).await;
        })
        .await;

    info!("server fermato");

    if let Err(e) = served {
        eprintln!("ERRORE server: {}", e);
        std::process::exit(1);
    }
}
