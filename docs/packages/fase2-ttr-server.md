# Pacchetto Fase 2 — `ttr-server`

> Pacchetto di contesto preparato dal Planner per la pipeline coding→reviewer.
> Fonti vincolanti: piano `docs/plan/ciufciuf-v1.3.md` §5.3, §6, §7, §10 (Fase 2), D16–D18;
> RECAP.md §4–5. Questo documento precisa le decisioni rimaste aperte a livello wire.

**Data:** 3 novembre 2026 · **Stato repo:** Fase 0+1 completata, git pulito su `main`.

---

## 1. Missione

Creare il crate `ttr-server` (Axum/Tokio): WebSocket + protocollo §7 + lobby (creazione,
join, nome+colore) + QR multi-IP (D16) + reconnect/snapshot (D17) + persistenza JSON in
`saves/` (D18) + i due backlog noti (mapping `EngineError`→`ErrorReason`, modulo di
conversione core↔proto).

**Dentro scope:** quanto sopra, con test di integrazione su WS reale (client simulati).

**Fuori scope (NON fare):** mDNS advertise (D16: solo testo fallback), TLS/HTTPS (Fase 3),
servizio statici del client (Fase 3), client Svelte (Fase 3), azioni Europe
`TUNNEL_DECISION`/`BUILD_STATION` (Fase 4), statistiche `profiles.json` (Fase 5, D20),
Docker (fase aperta), timeout/auto-skip dei turni.

## 2. Vincoli d'ingaggio (non negoziabili)

1. **Non toccare `data/`**: `usa.json` è **corretto** (22 rotte doppie verificate con
   aritmetica + script; i bug della fonte geckods sono documentati in
   `docs/data-sources-usa.md`). Nessuna " correzione" ai dati.
2. **Non alterare la semantica dell'ultimo giro**: chi innesca la fine **gioca** l'ultimo
   turno, come tutti (§3.1/D11; `advance_turn` è corretto così com'è; c'è un golden test
   che la pinna). Il server NON aggiunge logica di fine-partita sopra il motore: la fine
   arriva solo dall'evento `GameOver` del motore.
3. **`DRAW_TICKETS` esiste ed è l'azione nº 3 del turno** (§3.1/§7.1). È stata dimenticata
   una volta: non deve sparire di nuovo. Il server la instrada al motore come le altre.
4. Golden rules del RECAP §5: determinismo (mai iterare `HashMap` in percorsi che
   influenzano serializzazione/stato → `BTreeMap`/ordinamenti espliciti); il motore non
   conosce UI/rete; il client è scemo (le regole vivono nel motore); solo LAN; nessuna
   chiamata esterna a runtime; cambiamenti minimi.
5. Convenzioni del repo: commenti/docstring in italiano, moduli con header `//!`,
   edition 2024, `Cargo.lock` tracciato (il binario è un'applicazione).
6. `ttr-proto` è modificabile (il client non esiste ancora; ts-rs rigenera i binding), ma
   ogni cambiamento va motivato dalle decisioni E* qui sotto e i test round-trip esistenti
   vanno aggiornati, non cancellati.

## 3. Decisioni di protocollo (E1–E10)

Le shape §7 restano; queste decisioni colmano i dettagli che raw-WebSocket (D6) lascia
aperti e i gap trovati esplorando il contratto. Diventeranno E1–E10 nel RECAP.

- **E1 — Una connessione WS per client, envelope `ServerMessage`.** I "canali" §7
  (broadcast pubblico / unicast privato / errori) diventano varianti di un enum
  internamente taggato, payload annidato (niente `#[serde(flatten)]`): 
  `{"type":"PUBLIC_STATE","payload":{…PublicState…}}`, idem `PRIVATE_STATE`, `ERROR`,
  `SNAPSHOT`. I campi `msg_type: String` di `ErrorMessage`/`Snapshot` **vengono rimossi**
  (sostituiti dal tag dell'envelope). Inbound resta `InboundAction` com'è (tag
  `actionType`, §7.1 verbatim).
- **E2 — `GameState::WaitingForPlayers`** (variante nuova; l'enum è già
  `#[non_exhaustive]` per questo). La lobby esiste nel ciclo WS: i client collegati in
  lobby ricevono `PUBLIC_STATE` con `state=WAITING_FOR_PLAYERS`, `playersSummary` dei
  giocatori joinati, `claimed_routes: []`, `visible_deck: []`, `currentTurnPlayerId:
  null`. Così una connessione copre lobby→gioco→game over e il "rientro in lobby" di §6.1
  è un semplice snapshot.
- **E3 — `PrivateState.offered_tickets: Vec<TicketInfo>`** (campo nuovo). È il modo in cui
  il telefono scopre i biglietti tra cui scegliere (gap §7.3): nel draft iniziale i
  propri 4 finché `initial_draft_done` è false; in `TICKET_CHOICE` i `pending_tickets`
  (solo se sei il giocatore corrente); vuoto negli altri casi. I punti arrivano dai
  `MapData` che il server ha già caricato per costruire il motore. Attenzione:
  `initial_tickets_for()` **non** si svuota dopo la scelta (guardia = `initial_draft_done`)
  e `pending_tickets()` **non** si svuota dopo `CHOOSE_TICKETS` (guardia = fase
  `TicketChoice` + giocatore corrente) — deriva con quelle guardie.
- **E4 — Punteggi finali nel canale pubblico**: `PublicState` += `final_scores: Option<Vec<FinalScoreInfo>>`
  e `longest_path_player_ids: Option<Vec<String>>` (id, non indici), `None` fino al
  `GAME_OVER`, `Some` nel broadcast e negli snapshot a fine partita. Serve alla
  schermata di ripartizione punti (§6.9) che la Fase 3 renderà. Nuovo struct proto
  `FinalScoreInfo { player_id, route_points, ticket_completed, ticket_failed,
  longest_path_bonus, total }` (i32 per i punti firmati). Il server li ricava
  dall'evento `GameOver { final_scores, longest_path_players, … }` e li conserva nella
  sessione.
- **E5 — `visible_deck: Vec<Option<CardColor>>`** (era `Vec<CardColor>`): gli slot vuoti
  esistono (reset pool con mazzo esausto) e gli indici di `legal_moves.draw_face_up`
  devono restare allineati all'array. `None` = slot vuoto.
- **E6 — Idempotenza inbound + seq outbound.** Ogni payload inbound += `client_seq:
  Option<u64>` (5 struct). Regola server: per giocatore, monotono; se `Some(seq)` con
  `seq <= last` → azione **droppata silenziosamente** + re-push del `PRIVATE_STATE`
  corrente (il client si riallinea); altrimenti aggiorna `last` ed esegui. `None` →
  esegui (client semplici). Outbound: `PrivateState.seq` è un contatore per-giocatore
  strettamente crescente che avanza su **ogni** messaggio unicast al giocatore
  (`PRIVATE_STATE`, `ERROR`, e il `private` dentro `SNAPSHOT`); il client scarta
  messaggi con `seq <=` ultimo visto.
- **E7 — `Snapshot.private: Option<PrivateState>`**: `None` per gli spettatori (table
  view senza `playerId`), `Some` per i giocatori.
- **E8 — `ErrorReason` += `GameNotFound`, `Unauthorized`, `GameFull`, `NameTaken`** (usi
  server-level: game inesistente, token sbagliato, game al completo, nome duplicato).
  Le azioni di gioco mappano `EngineError`→`ErrorReason` in modo **esaustivo** (tabella
  in §7.2 qui sotto).
- **E9 — Auth minima LAN:** `POST /api/games/{id}/join` restituisce `playerId`+`token`
  (uuid v4, per sessione); il creatore riceve anche `hostToken` (per `/start`). Il WS si
  autentica via query string `?playerId=…&token=…` (i browser non permettono header sul
  WS): token errato → chiusura WS con `ERROR{Unauthorized}`.
- **E10 — Fix isolato al motore (l'unico consentito):** in `handle_mid_game_choice`
  (`engine_tickets.rs`), dopo aver inserito i biglietti tenuti, chiamare
  `update_ticket_completion(player_index)`: oggi un biglietto pescato a metà partita e
  **già completato** dalle rotte esistenti resta `completed: None` (→ false nel canale
  privato) finché il giocatore non reclama di nuovo — rotto proprio per il live feedback
  §7.3 che la Fase 2 inizia a esporre. Non toccare il draft iniziale (lì la connettività
  è vuota, `None ≡ false` è corretto). Golden test dedicato: claim di una rotta, poi
  `DrawTickets`, poi `ChooseTickets` di un biglietto già connesso → `completed == Some(true)`
  nel PrivateState derivato (o nel ticket status del motore).

## 4. Contratto wire — esempi concreti

Outbound (unico socket per client):

```jsonc
{"type":"PUBLIC_STATE","payload":{"gameId":"g1","protocolVersion":1,
 "state":"WAITING_FOR_PLAYERS","currentTurnPlayerId":null,"lastRound":false,
 "lastRoundTriggerPlayerId":null,"visibleDeck":[],"claimedRoutes":[],
 "stations":[],"tunnelReveal":null,
 "playersSummary":[{"playerId":"p1","name":"Gino","color":"BLUE","trainsRemaining":45,
                    "stationsRemaining":0,"score":0,"cardsHandCount":0,"ticketsCount":0}],
 "finalScores":null,"longestPathPlayerIds":null}}

{"type":"PRIVATE_STATE","payload":{"seq":7,"hand":["RED","BLUE"],
 "tickets":[{"id":"denver_phoenix","points":11,"completed":false}],
 "offeredTickets":[{"id":"seattle_new_york","points":22,"completed":false}],
 "legalMoves":{"routes":[],"drawFaceUp":[],"canDrawDeck":true,"canDrawTickets":true}}}

{"type":"ERROR","payload":{"seq":8,"reason":"INSUFFICIENT_CARDS",
 "actionRef":"CLAIM_ROUTE denver_phoenix_1"}}

{"type":"SNAPSHOT","payload":{"protocolVersion":1,"public":{…},"private":{…o null…}}}
```

Inbound = `InboundAction` §7.1, con `client_seq` opzionale in ogni payload:
`{"actionType":"DRAW_TICKETS","player_id":"p1","client_seq":42}`.

`actionRef` per ERROR: `"CLAIM_ROUTE denver_phoenix_1"`, `"DRAW_TRAIN_CARD deck"` o
`"… faceUp[2]"`, `"DRAW_TICKETS"`, `"CHOOSE_TICKETS …"`, `"REQUEST_SNAPSHOT"`.

## 5. API REST (per lobby; JSON, errori `{"error":"REASON","detail":"…"}`)

```
GET  /api/lan        → {"port":8080,"candidates":[{"ip":"192.168.1.10",
                       "url":"http://192.168.1.10:8080","preferred":true}],
                       "fallback":"http://ttr.local:8080"}
GET  /qr?data=<url>  → 200 image/svg+xml (QR dell'URL; crate `qrcode`)
POST /api/games      {"rulesetId":"usa_v2","name":"Serata"} → 201
                       {"gameId","hostToken","rulesetId","maxPlayers"}
GET  /api/games      → {"games":[{gameId,name,rulesetId,playersJoined,maxPlayers,started}]}
GET  /api/games/{id} → lobby: {gameId,name,rulesetId,players:[{playerId,name,color}],started,maxPlayers}
POST /api/games/{id}/join  {"name":"Gino","color":"BLUE"} → {"playerId","token","wsUrl":"/ws/{id}"}
                       (color facoltativa → primo colore libero in ordine
                        RED,BLUE,GREEN,YELLOW,BLACK; nome univoco case-insensitive;
                        pieno → 409 GAME_FULL; già started → 409 GAME_STARTED)
POST /api/games/{id}/start {"hostToken"} → 204; richiede ≥ ruleset.players.min
                       (già started → 409)
WS   /ws/{gameId}?playerId=…&token=…   (senza parametri = spettatore/table view)
```

Semantica lobby: il creatore joina come tutti gli altri. `start` costruisce il motore
(`Engine::new`) con i giocatori **in ordine di join**, `first_player = 0`, seed
time-based (registrato nel save). `playersSummary` in lobby: treni = `trainsPerPlayer`
del ruleset, tutto il resto 0.

`/api/lan`: enumera gli IPv4 con `local-ip-address`, esclude loopback; `preferred` le
range private (10/8, 172.16/12, 192.168/16). Non serve rilevare "l'interfaccia verso il
telefono": lista completa + preferred basta (D16).

## 6. Architettura del crate

```
crates/ttr-server/
├── Cargo.toml      # lib + [[bin]] ttr-server; axum(ws), tokio, serde, serde_json,
│                   # uuid(v4), local-ip-address, qrcode, tracing(+subscriber),
│                   # futures-util, ttr-core(path), ttr-proto(path)
│                   # dev-deps: tokio-tungstenite, reqwest(default-features=false,json)
└── src/
    ├── main.rs     # CLI (--port 8080, --data-dir ./data, --saves-dir ./saves,
    │               #  --resume <file.json>), bind 0.0.0.0, avvio
    ├── lib.rs      # build_router(state) → Router (testabile), moduli
    ├── state.rs    # AppState: registry BTreeMap<GameId, GameHandle>, config,
    │               #  ruleset registry (scan di data/rulesets/*.json, id = file stem,
    │               #  mappa via ruleset.map → data/maps/{map}.json)
    ├── lobby.rs    # endpoint REST di cui sopra
    ├── session.rs  # GameSession + attore: mpsc<GameCommand>; possiede Option<Engine>,
    │               #  roster (playerId→index, nomi, colori, token), collegamenti
    │               #  (BTreeMap<playerId, Vec<mpsc::Sender<ServerMessage>>>, spettatori),
    │               #  contatori seq, client_seq, final_scores, save path
    ├── ws.rs       # upgrade WS, auth, ciclo di vita connessione (split sink/stream),
    │               # inbound→GameCommand, outbound→sink
    ├── bridge.rs   # conversione core↔proto + mapping errori (§7 qui sotto)
    └── save.rs     # formato D18, scrittura atomica (tmp+rename), replay da save
```

**Attore per game** (niente lock attraversanti await): ogni `GameSession` è un task
tokio che possiede `Option<Engine>` e serve comandi da un `mpsc` — `Join`, `Start`,
`Action { player_id, action, client_seq }`, `Connect/Disconnect`, `RequestSnapshot`,
`Shutdown`. Ogni mutazione del motore passa dall'attore → nessuna race, replay sicuro.
Il registry espone `GameHandle { tx: mpsc::Sender<GameCommand>, public info cache }`.

**Flusso dopo ogni azione applicata (Ok):** ricomputa `PublicState` → broadcast a
tutti (giocatori + spettatori); pusha `PrivateState` fresca a ogni giocatore collegato
(o ognuno col proprio seq — 2–5 giocatori in LAN, il costo è irrilevante); appende
l'azione al save (atomico). **Su Err:** `ERROR` solo all'attore, con seq del giocatore.
**Eventi:** il bridge deriva gli snapshot dai getter del motore, non dagli eventi; gli
eventi servono solo per catturare `GameOver` (→ `final_scores` in sessione) e per il log.

## 7. `bridge.rs` — regole di conversione

### 7.1 Inbound → `ttr_core::Action`

`playerId` → `player_index` via roster (Unknown → `ErrorReason::InvalidAction` con
`actionRef` del tipo). `DrawSource` proto→core 1:1. `ChooseTickets.keep`,
`ClaimRoute.route_id/cards_discarded` passano attraverso. Nota: il motore **richiede**
`cards_discarded.len() == len(rotta)` (hint = pagamento completo; le locomotive mancanti
le sostituisce lui) — il bridge non risolve il pagamento.

### 7.2 `EngineError` → `ErrorReason` (match esaustivo, test unit dedicato)

| EngineError | ErrorReason |
|---|---|
| `WrongState` | `WrongState` |
| `NotYourTurn` | `NotYourTurn` |
| `RouteAlreadyClaimed` | `RouteAlreadyClaimed` |
| `RouteBlockedDouble`, `SamePlayerDouble` | `RouteBlockedDoubleVariant` |
| `InsufficientCards` | `InsufficientCards` |
| `FaceUpIndexOutOfRange`, `CardNotAvailable`, `LocomotiveAsSecondCard`, `DeckEmpty`, `PlayerNotFound`, `RouteNotFound`, `InvalidRouteColor`, `InvalidPayload`, `DraftNotComplete` | `InvalidAction` |
| `TicketChoiceBelowMin`, `TicketChoiceTooMany`, `TicketNotOffered` | `TicketChoiceOutOfRange` |

Errori server-level (già in E8): game inesistente `GameNotFound`, auth `Unauthorized`,
pieno `GameFull`, nome dup. `NameTaken`.

### 7.3 Derivazione `PublicState` (dai getter, ordine deterministico)

- `state`: lobby → `WaitingForPlayers`; `GamePhase` → `GameState` 1:1
  (InitialDraft/PlayerTurn/SecondCardDraw/TicketChoice/LastRound/GameOver;
  `TunnelResolution`/`StationScoring` restano Europe/Fase 4).
- `current_turn_player_id`: `Some(engine.current_player())`→id una volta started;
  `None` in lobby.
- `last_round` / `last_round_trigger_player_id`: dai getter (indice→id).
- `visible_deck`: `face_up_cards()` → `Vec<Option<CardColor>>` così com'è (E5).
- `claimed_routes`: deriva dagli `PlayerState.claimed_routes` (BTreeMap) **in ordine di
  giocatore** — niente getter per `claimed_by`, non serve tocar il core.
- `players_summary`: id, name, color, trains_remaining, score, `hand.len()`,
  `tickets.len()`, `stations_remaining: 0` (USA; Europe Fase 4).
- `tunnel_reveal: None`, `stations: []` (USA).
- `final_scores` / `longest_path_player_ids`: dalla sessione (evento `GameOver`), E4.

### 7.4 Derivazione `PrivateState` (per `player_index`)

- `seq`: contatore sessione per giocatore (E6).
- `hand`: `player_hand()`.
- `tickets`: `PlayerState.tickets` (BTreeMap → ordine deterministico) →
  `TicketInfo { id, points, completed: ts.completed.unwrap_or(false) }`.
- `offered_tickets`: E3 (guardie su `initial_draft_done` e fase+current player).
- `legal_moves`: `engine.legal_moves(player_index)` → struct proto 1:1.

## 8. Persistenza (D18) — `save.rs`

`saves/{gameId}.json`, scritto alla creazione e dopo **ogni** azione applicata
(scrittura atomica: tmp + rename), `status` → `"completed"` al `GameOver`:

```jsonc
{
  "formatVersion": 1,
  "gameId": "…", "gameName": "Serata",
  "mapId": "usa", "rulesetId": "usa_v2",
  "seed": 123456, "firstPlayer": 0,
  "players": [{"id":"p1","name":"Gino","color":"BLUE"}],   // ordine = ordine di join
  "actions": [ /* serde di ttr_core::Action, solo le applicate con Ok */ ],
  "status": "in_progress" | "completed",
  "createdAt": "…", "updatedAt": "…"
}
```

Le azioni si prendono da `engine.action_log()` (contiene solo le Ok; `Action` è già
`Serialize/Deserialize`). **Replay:** `Engine::new(seed, ruleset, map, ids, names,
colors, first)` + `apply()` in ordine → stato identico (`serialize_state()` per
verificare nei test). `mapId`/`rulesetId` = file stem con cui il server li ha caricati.

**Resume (`--resume saves/x.json`)**: il server ricostruisce il motore via replay,
registra la partita in modalità lobby con il roster salvato; `hostToken` va nel log di
avvio. `join` con un nome salvato → ri-binding al playerId/colori originali (token
nuovo); nomi non salvati → rifiuto. `/start` (dopo che l'host ha deciso, anche con
roster parziale) rimette in moto la partita ricostruita. Limitazione documentata: i
turni dei giocatori non rientrati restano in attesa (niente auto-skip in Fase 2).

## 9. Test e criteri di accettazione (Fase 2)

Tutto su **WS reale**: server axum su `127.0.0.1:0` (porta effimera) spawnato dal test,
client simulati con tokio-tungstenite; REST via reqwest; dati da
`CARGO_MANIFEST_DIR/../../data/`. Il client simulato è "scemo": decide **solo** da
`PRIVATE_STATE`/`PUBLIC_STATE` (`legalMoves`, `offeredTickets`) — l'unico punto dove
tocca conoscere le regole è costruire il pagamento del claim (colore/len dalla mappa,
che il test carica come dev-dep) e knowMin (costanti di test).

1. **`full_game_over_ws`** — create/join×2/start via REST; draft parallelo
   (`CHOOSE_TICKETS` keep 2); loop turni fino a `state=GAME_OVER` con
   `finalScores` Some e totali coerenti (>0 per il vincitore). Mossa del turno, in
   ordine: claim della prima rotta legale (pagamento deterministico: colore rotta o,
   per le grigie, primo colore con conteggio sufficiente in ordine enum, loco a
   riempimento); se no pescate → `DRAW_TRAIN_CARD deck` ×2; se no →
   `DRAW_TICKETS`+`CHOOSE_TICKETS` (keep 1). Se **nessuna** mossa legale → fail con
   messaggio chiaro (gap del motore da portare al planner, non un problema del test).
2. **`reconnect_mid_game`** — client A si disconnette quando **non** è il suo turno; B
   gioca 1–2 turni; A riconnette (stesso playerId+token) → riceve `SNAPSHOT`: mano
   identica alla pre-disconnessione, biglietti identici, `legalMoves` coerenti con lo
   stato nuovo, seq > ultimo visto. Poi la partita finisce regolarmente.
3. **`idempotency`** — stessa azione ritrasmessa con stesso `clientSeq` → nessun doppio
   effetto (mano invariata dopo ritrasmissione di una pesca) + re-push di PrivateState.
4. **`save_and_replay`** — a fine partita il file `saves/{id}.json` esiste, `status:
   completed`, N azioni > 0; replay da file → `GameOver` con stessi `final_scores`
   ( confronto `serialize_state()` tra replay e motore di riferimento del test).
5. **`resume_flow`** — partita salvata a metà (unit: motore diretto + save.rs);
   `--resume` (funzione lib, testabile senza porta); join col nome salvato → stesso
   playerId; start → `PUBLIC_STATE` uguale a quella pre-save (fase, claimedRoutes,
   playersSummary).
6. **Unit** — mapping errori esaustivo (§7.2), derivazioni bridge vs motore golden,
   lobby (nome dup. → `NameTaken`, colore auto, start < min → 400, join dopo start → 409),
   seq monotono per giocatore, `/api/lan` filtra loopback, `/qr` restituisce SVG.
7. **Proto** — aggiornare i round-trip esistenti per i campi nuovi (E3–E8); rigenerare i
   binding ts (`cargo test -p ttr-proto`) e committarli (dir `crates/ttr-proto/bindings/`).

**Guardie semantiche (devono restare verdi):** `cargo test --workspace` al completo
(66+ test Fase 0+1), in particolare il golden test "trigger ultimo dell'ordine" e i
test `DRAW_TICKETS` del proto.

## 10. Self-check prima dell'handoff

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt                      # solo sui file nuovi/modificati, nessun reformat di massa
cargo test -p ttr-proto        # rigenera + verifica binding TS
```

Poi: **un solo commit** locale su `main` (niente push), messaggio
`Fase 2: ttr-server — WS §7, lobby, QR multi-IP, reconnect/snapshot, persistenza D18`,
con i binding rigenerati inclusi. Riassumere nel handoff: file toccati, test aggiunti,
scostamenti (se inevitabili) da questo pacchetto con motivo.

*Fine pacchetto Fase 2. Fonte delle regole: piano §3/§6/§7; nulla qui sovrascrive il piano —
in caso di conflitto vince il piano e va segnalato al planner.*
