# CiufCiuf!

Gioco in stile *Ticket to Ride* per la **LAN di casa**: la TV mostra la mappa condivisa, ogni giocatore (2–5) usa il proprio telefono come controller privato, collegandosi via **QR code**. Zero internet, zero installazione, solo amici.

> [!NOTE]
> Progetto personale, uso privato. Nessun asset, artwork o testo di Days of Wonder/Asmodee: le meccaniche sono replicabili, la grafica è originale (mappa SVG disegnata da coordinate geografiche reali).

## Stato

Pre-alpha: il repo contiene per ora solo la documentazione di progetto.

- **[`RECAP.md`](RECAP.md)** — punto d'ingresso per orientarsi (scritto anche per agenti AI)
- **[`docs/plan/ciufciuf-v1.3.md`](docs/plan/ciufciuf-v1.3.md)** — piano completo: regole vincolanti, architettura, protocollo, roadmap

## Stack (previsto)

- **Motore + server**: Rust — core puro e deterministico, server Axum/Tokio con WebSocket
- **Client**: TypeScript + Svelte 5 — table view per la TV, controller view per i telefoni
- **Dati**: mappe e ruleset come JSON, validati da tool dedicato

Lo sviluppo procede a fasi (piano §10); il primo traguardo è la mappa USA giocabile da telefoni veri via QR.
