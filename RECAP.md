# CiufCiuf! — RECAP (context-pack)

> **Sei un agente (o un umano) appena entrato in questo progetto?** Questo file è il punto d'ingresso ufficiale (decisione D23): leggilo per orientarti, poi apri il piano per i dettagli tecnici.

**Aggiornato:** 3 novembre 2026 (sera) — piano **v1.3**; **Fase 0+1+2 completate** (pipeline coding→reviewer; Fase 2 chiusa con review + rework mirato F1–F8 verificato dal planner).
**Repo:** GitHub **privato** `kidgvbs/ciufciuf` — clone canonico: `~/Rust/ciufciuf`.

---

## 1. Cos'è CiufCiuf!

Un gioco da tavolo in stile **Ticket to Ride** (Days of Wonder) ricreato come app **personale per la LAN di casa**:

- Un **Mac host** (binario statico) serve tutto e mostra la mappa condivisa sulla TV via browser.
- Ogni giocatore (**2–5**, solo amici in casa) collega il telefono scansionando un **QR code** con l'IP numerico della LAN: il telefono è il controller con la **mano privata** (carte, biglietti, mosse legali).
- **Zero internet** a runtime: niente account, matchmaking, negozio, classifiche.
- Nome originale **CiufCiuf!** per non toccare il marchio: mai "Ticket to Ride" nel codice o negli artefatti (v. regola d'oro 2).

Primo traguardo: mappa **USA** (`usa_v2`, regole edizione V2 2025) giocabile da telefoni veri via QR. Poi **Europe** (tunnel, traghetti, stazioni). Poi bot euristico e statistiche cumulative per profilo.

## 2. Stato attuale e prossimo passo

| Voce | Stato |
|---|---|
| Piano | **v1.3, completo e vincolante** — `docs/plan/ciufciuf-v1.3.md`. Punti aperti (§14): chiusi; restano solo i ruleset futuri (1910/Mega/Big Cities), rinviati per design |
| Fase 0+1 | **Completata e verificata** (3/11/2026): workspace Cargo, `ttr-proto` (contratto §7 completo con binding ts-rs), dati USA `usa_v2` (36 città, 100 segmenti, 22 doppie, 33 biglietti — 7 check di `ttr-validate` verdi), motore `ttr-core` (regole USA complete, macchina a stati §6, RNG seeded + replay deterministico, `legalMoves` per-fase, CLI `ttr-cli`). 66 test verdi + fuzzing 1000 partite con invarianti. Review: 2 fix veri applicati (variante WS `DRAW_TICKETS`, gate per-fase di legalMoves) + test edge "trigger ultimo dell'ordine" |
| Fase 2 | **Completata e verificata** (3/11/2026 sera): crate `ttr-server` — attore per game (`GameCommand` via mpsc), WS §7 con envelope `ServerMessage`, lobby REST (7 endpoint, seed opzionale per test), QR multi-IP + `/api/lan`, reconnect/`SNAPSHOT`, persistenza D18 (`saves/` atomic write, replay, `resume_game`), auth minima LAN (E9). Contratto proto esteso E1–E8 + binding ts rigenerati; unico fix al core: E10 (`update_ticket_completion` mid-game, con golden test). Commit: `cb3d059` + `14c84fc` + rework `de8ff94` (F1 auth WS, F2 chiusura socket su token errato, F3 resume per player_id, F4 lobby via attore `GetInfo`, F5 dedup `client_seq=0`, F6 disconnect per connessione, F7 log save, F8 golden E10 + test §9). **99 test verdi**, clippy `-D warnings` pulito |
| Prossimo passo | Pacchetto **Fase 3** alla pipeline coding→reviewer: client **Svelte 5** (D22) — viste TV (mappa) + controller telefono, SDK dai binding `crates/ttr-proto/bindings/`, connessione WS con reconnect/`SNAPSHOT` su `visibilitychange` (D17) |

### Decisioni wire della Fase 2 (registro E1–E10 — spec completa: `docs/packages/fase2-ttr-server.md`)

- **E1** envelope `ServerMessage`: tag `type` + `payload` annidato; `ErrorPayload` al posto di `ErrorMessage`.
- **E2** `GameState::WaitingForPlayers` pre-start. **E3** `PrivateState.offered_tickets` con guardie (draft iniziale vs `TicketChoice` mid-game solo current player).
- **E4** `PublicState.final_scores: Option<Vec<FinalScoreInfo>>` + `longest_path_player_ids` a fine partita.
- **E5** `visible_deck: Vec<Option<CardColor>>` (indici allineati alle face-up). **E6** `client_seq` su tutti i payload inbound + `PrivateState.seq` monotono per giocatore su ogni unicast (dedup idempotenza, survives reconnect).
- **E7** `Snapshot.private: Option<PrivateState>` (`None` = spettatore). **E8** `ErrorReason` esteso con `GameNotFound`/`Unauthorized`/`GameFull`/`NameTaken`.
- **E9** auth minima LAN: playerId+token (uuid v4) da REST, `hostToken` per start, WS auth via query string; azioni inbound autorizzate dall'identità della **connessione**, mai dal payload.
- **E10** unico intervento consentito a `ttr-core`: `update_ticket_completion` in `handle_mid_game_choice` (biglietti mid-game già completati prima della scelta).

**Regola di manutenzione: chi completa uno step o prende una decisione, aggiorna questo RECAP.**

## 3. Mappa del piano (`docs/plan/ciufciuf-v1.3.md`)

| Sezione | Contenuto |
|---|---|
| §2 | Registro completo delle decisioni **D1–D23** (vincolanti) |
| §3 | **Regole di gioco vincolanti**: USA §3.1, Europe §3.2, checklist edge case §3.3, varianti come parametri §3.4 |
| §4 | Dati mappa + schema del ruleset descriptor + validazione (§4.2) |
| §5 | Architettura: workspace §5.1, core §5.2, server §5.3, client §5.4 |
| §6 | Macchina a stati (9 stati, da `WAITING_FOR_PLAYERS` a `GAME_OVER`) |
| §7 | Contratto WebSocket (broadcast pubblico, unicast privato, snapshot/reconnect) |
| §8 | UI/UX (table view TV + controller view telefono) |
| §9 | Algoritmi (Union-Find, stazioni, longest path, determinismo) |
| §10 | Roadmap a fasi 0–5 con criteri di accettazione |
| §11 | Registro rischi R1–R8 |
| §12 | Note legali (IP Asmodee/Days of Wonder) |

## 4. Stack e decisioni chiave (registro completo: piano §2)

- **D3/D4/D22 — Stack**: Rust per motore e server (`ttr-core` puro; `ttr-server` Axum/Tokio; WebSocket raw + JSON, niente STOMP — D6) + client **TypeScript con Svelte 5** (D22, scelta diretta dell'utente). Contratto WS framework-agnostic: l'unica superficie Svelte sono le due viste, sostituibili senza toccare il backend.
- **D7 — Mappe**: USA (`usa_v2`) + Europe; ogni mappa = dati JSON + ruleset descriptor, zero fork del motore.
- **D8 — Host**: Mac esistente; binario statico per il gioco serale; Docker per CI/riproducibilità (macOS non ha `network_mode: host`).
- **D9 — Fonte di verità**: regolamenti ufficiali DoW (PDF); BGA solo riferimento implementativo.
- **D10 — Accesso**: solo QR + un telefono per giocatore; pass & play fuori dal primo traguardo.
- **D11 — Ultimo giro**: tutti giocano un ultimo turno, incluso chi innesca (≤2 treni rimasti).
- **D14 — Europe**: due mazzi biglietti (6 lunghi + 40 regolari); scarti di setup fuori dalla partita (`setupDiscards: "removed"`).
- **D16 — QR**: IP numerico LAN; `ttr.local`/mDNS solo fallback testuale.
- **D17 — Standby telefoni**: primario reconnect + `SNAPSHOT` su `visibilitychange`; HTTPS self-signed per Wake Lock (best-effort).
- **D18 — Persistenza**: una partita = un file JSON in `saves/` (seed + log azioni); niente DB.
- **D19 — Nome**: CiufCiuf! — **D20 — Statistiche profili**: `profiles.json`, Fase 5 — **D21 — Audio**: posticipato.

## 5. Regole d'oro (non negoziabili)

1. **Fedeltà ai regolamenti ufficiali per-mappa**: la specifica vincolante è il piano §3 — non reinventare le regole, verificarle lì (es. locomotiva scoperta = pescata intera; reset pool a ≥3 locomotive; scarti biglietti in fondo in ordine; ultimo giro per tutti).
2. **Zero IP Days of Wonder/Asmodee**: niente nome ufficiale, niente artwork o asset estratti/scaricati. Mappa SVG propria da coordinate geografiche reali; icone libere (es. Lucide).
3. **Determinismo totale**: RNG seeded; mai iterare `HashMap` in percorsi che influenzano stato o serializzazione → `BTreeMap`/ordinamenti espliciti (piano §9.5, rischio R3). Un replay = seed + log azioni.
4. **Il motore non conosce UI né rete**: `ttr-core` espone `Applica(Azione) → [Eventi]`; tutto il resto vive fuori.
5. **Il client è scemo**: le mosse legali arrivano dal server (`legalMoves` nel canale privato); nessuna regola duplicata lato client.
6. **Solo LAN**: nessuna chiamata esterna a runtime.
7. **Cambiamenti minimi**: si segue la roadmap del piano §10; niente refactor fuori scope. Motore prima della UI, dati prima del motore.

## 6. Struttura target del repo (piano §5.1)

```
ciufciuf/
├── RECAP.md            # questo file
├── crates/
│   ├── ttr-core/       # motore: puro, deterministico, map-agnostic
│   ├── ttr-server/     # Axum: WebSocket, lobby, QR/mDNS, statici, persistenza
│   ├── ttr-validate/   # CLI validazione dati mappa
│   └── ttr-proto/      # tipi contratto WS (serde) + export ts-rs
├── data/
│   ├── maps/           # cities, routes, tickets per mappa
│   └── rulesets/       # descriptor per ruleset
├── client/             # TypeScript + Svelte 5 (D22)
└── docs/plan/          # piano ufficiale + archivio versioni
```

## 7. Workflow operativo (per agenti)

- **Planner** (orchestra, confeziona pacchetti di contesto) → **coding** (implementa + self-check: build/test/lint prima dell'handoff) → **reviewer** (review diff-based: correttezza, memory leak, errori, logica; non modifica). Rework mirato se la review trova problemi.
- Prima di scrivere codice: leggere le sezioni del piano citate dal pacchetto (di solito §3 regole, §4 dati, §6 stati, §7 protocollo).
- **Aggiornare questo RECAP** a ogni decisione o step completato.
- tokensave è inizializzato in questo repo: dopo modifiche al codice eseguire `tokensave sync` dalla root (`.tokensave/` è nel `.gitignore`).

## 8. FAQ rapida

- **Perché Svelte?** Scelta dell'utente (D22): bundle piccolo, reattività semplice per la mano privata, SVG naturale. Se delude, si rimpiazzano due viste e basta.
- **Perché il repo è privato?** D23: è la documentazione personale del progetto; il piano §12 nota che potrebbe diventare pubblico senza toccare IP altrui, ma non è previsto.
- **Dove sono le regole precise?** Piano §3 (vincolante) e §9 (algoritmi). Dati di mappa/ruleset: §4.
