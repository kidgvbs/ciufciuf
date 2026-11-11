//! Entry point CLI: parsing argomenti, caricamento dati, avvio server.
//!
//! Uso: `ttr-server --port 8080 --data-dir ./data --saves-dir ./saves [--resume <file>]`

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use ttr_server::save::archive_completed_saves;
use ttr_server::{AppState, build_router, resume_game, shutdown_sessions};

/// Attesa massima per le scritture in coda alla chiusura.
const SHUTDOWN_FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Tempo massimo tra il primo segnale e l'uscita del server, connessioni
/// ancora aperte comprese.
const SHUTDOWN_TOTAL_TIMEOUT: Duration = Duration::from_secs(15);

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

    /// Ammette il seed scelto dal client in `POST /api/games` (solo per test e
    /// partite riproducibili: con il seed si ricostruiscono le mani).
    #[arg(long)]
    allow_client_seed: bool,
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
        Ok(s) => s.with_allow_client_seed(args.allow_client_seed),
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

    // Dopo il resume: un save concluso passato a `--resume` va segnalato come
    // tale, non archiviato prima che il resume lo legga.
    let archived = archive_completed_saves(&state);
    if archived > 0 {
        info!("{archived} save conclusi archiviati in classifica all'avvio");
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

    // Primo segnale: parte lo shutdown. Secondo segnale: uscita immediata.
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = stop_tx.send(true);
        shutdown_signal().await;
        tracing::warn!(
            "secondo segnale: uscita immediata, i save in coda potrebbero non essere su disco"
        );
        std::process::exit(130);
    });

    // Le partite si fermano prima del rilascio delle connessioni: i WebSocket
    // si chiudono quando il loro attore termina.
    let shutdown_state = state.clone();
    let mut graceful_stop = stop_rx.clone();
    let serve = axum::serve(listener, router).with_graceful_shutdown(async move {
        let _ = graceful_stop.wait_for(|stopped| *stopped).await;
        shutdown_sessions(&shutdown_state, SHUTDOWN_FLUSH_TIMEOUT).await;
    });
    let hard_deadline = async {
        let _ = stop_rx.wait_for(|stopped| *stopped).await;
        tokio::time::sleep(SHUTDOWN_TOTAL_TIMEOUT).await;
    };
    let served = tokio::select! {
        served = serve => served,
        _ = hard_deadline => {
            tracing::warn!("chiusura: connessioni ancora aperte dopo {:?}, uscita forzata", SHUTDOWN_TOTAL_TIMEOUT);
            Ok(())
        }
    };

    info!("server fermato");

    if let Err(e) = served {
        eprintln!("ERRORE server: {}", e);
        std::process::exit(1);
    }
}
