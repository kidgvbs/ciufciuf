//! Test di integrazione Fase 3b: serving dei statici del client con fallback SPA.
//!
//! Con `--client-dist` presente, il server serve `index.html` per `/` e per
//! le rotte client-side (`/controller`), senza oscurare le API.

mod common;

use common::*;
use std::path::PathBuf;

/// Crea una directory temporanea con un `index.html` marcatore.
fn temp_client_dist(marker: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ciufciuf-test-dist-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&p).expect("creazione dir dist di test");
    std::fs::write(p.join("index.html"), format!("<html><body>{marker}</body></html>"))
        .expect("scrittura index.html di test");
    p
}

/// GET `/` e `/controller` servono `index.html` (fallback SPA);
/// le API restano raggiungibili e non sono oscurate dal fallback.
#[tokio::test]
async fn spa_fallback_serves_index() {
    let marker = "ciufciuf-spa-test";
    let dist = temp_client_dist(marker);

    let saves = temp_saves_dir();
    let state = ttr_server::AppState::new(data_dir(), saves.clone(), 0).expect("AppState::new");
    let server = TestServer::start_with_client_dist(state, saves, dist.clone()).await;

    let client = reqwest::Client::new();

    // `/` → index.html
    let resp = client
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("GET /");
    assert_eq!(resp.status(), 200, "GET / deve tornare 200");
    let body = resp.text().await.expect("body /");
    assert!(body.contains(marker), "GET / serve index.html");

    // `/controller` → fallback SPA a index.html
    let resp = client
        .get(format!("{}/controller", server.base_url))
        .send()
        .await
        .expect("GET /controller");
    assert_eq!(resp.status(), 200, "GET /controller deve tornare 200");
    let body = resp.text().await.expect("body /controller");
    assert!(body.contains(marker), "GET /controller serve index.html (SPA)");

    // Le API non sono oscurate dal fallback statico.
    let resp = client
        .get(format!("{}/api/lan", server.base_url))
        .send()
        .await
        .expect("GET /api/lan");
    assert_eq!(resp.status(), 200, "GET /api/lan deve tornare 200");

    let _ = std::fs::remove_dir_all(&dist);
}

/// Senza `--client-dist` (directory inesistente), i statici non sono serviti:
/// GET `/` risponde 404 mentre le API restano attive.
#[tokio::test]
async fn no_client_dist_returns_404() {
    let server = TestServer::start().await; // client_dist = PathBuf::new() (inesistente)

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("GET /");
    assert_eq!(
        resp.status(),
        404,
        "senza client-dist GET / deve tornare 404"
    );

    let resp = client
        .get(format!("{}/api/lan", server.base_url))
        .send()
        .await
        .expect("GET /api/lan");
    assert_eq!(resp.status(), 200);
}
