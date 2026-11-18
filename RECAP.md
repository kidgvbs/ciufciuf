# CiufCiuf! — RECAP (context-pack)

> **Sei un agente (o un umano) appena entrato in questo progetto?** Questo file è il punto d'ingresso ufficiale (decisione D23): leggilo per orientarti, poi apri il piano per i dettagli tecnici.

**Aggiornato:** 18 novembre 2026 — regole speciali a inizio partita e giro «cose da fare» (`RULES_VERSION` 3). Il RECAP contiene solo **stato, prossimi passi e decisioni valide**; la cronaca dettagliata di fasi, round UX e giri di robustezza (3–18/11) è in **`docs/history.md`**.
**Repo:** GitHub **privato** `kidgvbs/ciufciuf` — clone canonico: `~/Rust/ciufciuf`.

---

## 1. Cos'è CiufCiuf!

Un gioco da tavolo in stile **Ticket to Ride** (Days of Wonder) ricreato come app **personale per la LAN di casa**:

- Un **Mac host** (binario statico) serve tutto e mostra la mappa condivisa sulla TV via browser.
- Ogni giocatore (**2–5**, solo amici in casa) collega il telefono scansionando un **QR code** con l'IP numerico della LAN: il telefono è il controller con la **mano privata** (carte, biglietti, mosse legali).
- **Zero internet** a runtime: niente account, matchmaking, negozio. Profili (solo nome) e classifica generale sono locali all'host, in `saves/`.
- Nome originale **CiufCiuf!** per non toccare il marchio: mai "Ticket to Ride" nel codice o negli artefatti (v. regola d'oro 2).

Primo traguardo: mappa **USA** (`usa_v2`, regole edizione V2 2025) giocabile da telefoni veri via QR. Poi **Europe** (tunnel, traghetti, stazioni). Poi bot euristico.

## 2. Stato attuale e prossimo passo

| Voce | Stato |
|---|---|
| Piano | **v1.3, vincolante** — `docs/plan/ciufciuf-v1.3.md` (aggiornato il 18/11 con mappe e modalità oltre USA/Europe, colori giocatore nuovi, regola della casa Germany). §14 chiuso. |
| Fasi 0–3 | **Completate** (3–9/11): `ttr-proto`, `ttr-validate`, motore USA (`ttr-core`), server Axum con lobby/QR/reconnect/save (`ttr-server`), client Svelte 5 con TV e controller, mappa schematica generata. |
| Fase 4 — Europe | **Completata** (11/11, 4.0 + 4.1 polish): tunnel, traghetti, stazioni, European Express, D29–D33. |
| Robustezza | **Nove giri** (9–12/11) su motore, server, client e infrastruttura; esiti in `docs/history.md`. |
| Ruleset | **15**: `usa_v2`, `usa_1910`, `usa_big_cities`, `usa_mega`, `europe`, `europe_expanded`, `europe_1912`, `europe_big_cities`, `europe_mega` (12/11, D34–D37); `germany`, `germany_2017`, `poland`, `basilicata` (17/11: binari paralleli 2–4, Paesi, carte Paese D38, passeggeri, `tieBreakers`); `nordic`, `switzerland` (18/11, D43–D45). |
| Ultimo giro (18/11) | `RULES_VERSION` **3**. **Germany, regola della casa** (D40): mano iniziale 2 lunghi + 3 corti, tieni ≥2 di cui ≥1 corto (`keepMin` per voce di `dealt`); in partita `ticketDraw {deck: regular, mergeDecks: [long], count: 4, keepMin: 1}`: a draft finito i mazzi si uniscono in **una pila** mescolata dall'RNG del motore, scarti in fondo. Pesca a quote (`chooseFrom`) rimossa end-to-end (D39 superata). **Colori giocatore** (D41): `PINK`, `CYAN`, `LIME`, `BROWN`, `LAVENDER`, `INDIGO`. **`PublicState`**: `blocked_route_ids` (binari chiusi da un gruppo già preso sotto `doubleRouteMinPlayers`, nascosti sulla TV) e `ticket_deck_size`. **Client**: controller a 3 colonne con popup di reclamo (`ClaimDialog`), zoom della TV sulla rotta reclamata, mazzo biglietti con conteggio. **Regole speciali** (D42): campo `specialRules` nei ruleset Germany (entrambe le modalità) e Poland, servito da `GET /api/rulesets`; pannello sulla TV durante il draft (poi «📜 Regole») e modale sul telefono. |
| Nordic e Switzerland (18/11) | **Completata e verificata** (planner + coding ×4 in parallelo + reviewer + rework). Campi nuovi tutti opzionali, ruleset esistenti invariati (golden uguali, `RULES_VERSION` resta 3): `locomotives` (`usableOn`, `freeDraw`, `faceUpReset`), `ferries.anyCardsPerLocomotive`, rotta `anyCardsPerSpace`, biglietti `destinations`, `ticketDraw.discards: "removed"`, spareggio `longestPath`. Pagamento verificato da `ttr-core/src/payment.rs` (assegnazione esatta di tutte le carte). Dettagli in `docs/history.md`, riga «Mappe Nordic e Switzerland». |
| Verifica (18/11) | Dopo Nordic e Switzerland: `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace --locked` (343 s con `CARGO_BUILD_JOBS=1`; `proptests` 218 s), `bun run check` (0 errori), `bun run test` (329) tutti verdi in locale. **CI GitHub**: job `rust` verde, job `client` **rosso su ogni push fino al 18/11** — `lobby.test.ts` non simulava `sessionStorage` (Node ≥ 25 lo ha globale, la CI no); stub aggiunto il 18/11, da confermare al prossimo push. |
| Prossimi fix | **Giro del 18/11 chiuso** (dettagli in `docs/history.md`, riga «Prossimi fix (giro del 18/11)»): `finished` in `GET /api/games` e «conclusa» sulla TV; label di Pittsburgh fuori dal binario Pittsburgh↔Toronto (layout USA rigenerato, tutti i gate a 0, 0 label su binari propri); polish §8.2 (carte spese che rimpiccioliscono verso il mazzo sul telefono, rotte prese in prestito dalle stazioni tratteggiate a fine partita). **Da confermare**: criterio di accettazione con 2–5 telefoni veri via QR (USA, Europe, Germany) e le nuove animazioni a vista. Poi **Fase 5** (bot, statistiche cumulative). |
| Aperti noti | **Layout Switzerland provvisorio** (decisione utente 18/11): copre tutte le città e rotte ma non passa i gate del generatore (37 incroci, 2 label-CORE, 1 tratta nascosta, 1 ratio) — `docs/data-sources-switzerland.md` §Layout provvisorio; da rifare in un giro dedicato (probabile: rotte a più pieghe nel generatore). **Dati Nordic/Switzerland** non verificati su foto delle carte (dettagli nei rispettivi `docs/data-sources-*.md`; Nordic Åndalsnes–Bergen Black, Switzerland Kreuzlingen–Deutschland come gruppo `alwaysOpen`). **Dati Germany non verificati** su foto delle carte: Mannheim–Stuttgart 2, Kiel–Stuttgart 16 (fonte: 17) — `docs/data-sources-germany.md`. **Nomi**: un nome con alfabeti misti («Аnna» con А cirillica) dà `400 INVALID_NAME` e non `409 NAME_TAKEN`; lo skeleton fonde «Tim»/«Tirn» in classifica; annullare un join lento lascia il giocatore registrato finché l'host non riassegna. **Test**: nessun test di componente per `JoinScreen` (solo `join-flow.ts`); `ClaimDialog` con gruppi di carte qualsiasi provato solo nella logica (`payment.ts`), non a vista. **Longest path**: esatto fino a numero ciclomatico 14 (`MAX_CYCLOMATIC_NUMBER`, le mappe lo rispettano; la stima conta ogni gruppo parallelo una volta: USA 9, Europe 10). **LAN fidata**: elenco/resume/eliminazione dei save senza `hostToken` (vedi W2). **CI**: `actions/checkout@v4` gira su Node 20 deprecato. |

### Decisioni wire della Fase 2 (registro E1–E10 — spec completa: `docs/packages/fase2-ttr-server.md`)

- **E1** envelope `ServerMessage`: tag `type` + `payload` annidato; `ErrorPayload` al posto di `ErrorMessage`.
- **E2** `GameState::WaitingForPlayers` pre-start. **E3** `PrivateState.offered_tickets` con guardie (draft iniziale vs `TicketChoice` mid-game solo current player).
- **E4** `PublicState.final_scores: Option<Vec<FinalScoreInfo>>` + `longest_path_player_ids` a fine partita.
- **E5** `visible_deck: Vec<Option<CardColor>>` (indici allineati alle face-up). **E6** `client_seq` su tutti i payload inbound + `PrivateState.seq` monotono per giocatore su ogni unicast (dedup idempotenza, survives reconnect).
- **E7** `Snapshot.private: Option<PrivateState>` (`None` = spettatore). **E8** `ErrorReason` esteso con `GameNotFound`/`Unauthorized`/`GameFull`/`NameTaken`.
- **E9** auth minima LAN: playerId+token (uuid v4) da REST, `hostToken` per start, WS auth via query string; azioni inbound autorizzate dall'identità della **connessione**, mai dal payload.
- **E10** unico intervento consentito a `ttr-core`: `update_ticket_completion` in `handle_mid_game_choice` (biglietti mid-game già completati prima della scelta).

### Decisioni wire del consolidamento pre-Europe (11/11/2026)

- **W1** `GET /api/saves` → `{saves:[{gameId,name,rulesetId,players:[nome],createdAt,updatedAt}]}`: solo file `<uuid canonico>.json` con `status` `in_progress`, `gameId` uguale al nome del file, `formatVersion` supportata, `rulesVersion` uguale a `RULES_VERSION` e ruleset noto (controlli sull'intestazione, senza replay), non già presenti in `state.games`; ordine `updatedAt` decrescente; file illeggibili saltati con `warn!`, mai un 500. Symlink, file non regolari e file oltre 16 MiB (`MAX_SAVE_FILE_BYTES`) sono saltati con `warn!` (e rifiutati dal resume via REST: 404 / 409); un save con `gameName` oltre 40 caratteri o un giocatore oltre 24 non è riprendibile (escluso dall'elenco, `409 SAVE_NOT_RESUMABLE` al resume; ottavo giro, I3).
- **W2** `POST /api/saves/{id}/resume` (nessun body) → `201 {gameId,hostToken,rulesetId,maxPlayers}`, stessa forma di `POST /api/games`. Errori: `404 SAVE_NOT_FOUND` (id non canonico o file assente), `409 GAME_ALREADY_ACTIVE`, `409 SAVE_NOT_RESUMABLE` (concluso, regole precedenti, illeggibile con dettaglio generico, `gameId` diverso dal file), `429 TOO_MANY_GAMES`, `503 SHUTTING_DOWN`. **Fiducia**: chiunque in LAN può riprendere un save, come può creare una partita; una partita attiva non si riprende due volte, quindi nessuno ottiene un secondo `hostToken` per una partita in corso. `--resume` da CLI resta per l'operatore e stampa l'`hostToken` nel log. **Rischio accettato** (settimo giro, H1/H2/H4/H6, decisione utente): per un save fermo il primo che arriva ottiene l'`hostToken`, quindi con `reassign` anche la mano privata dei giocatori; `GET /api/saves` e `DELETE /api/saves/{id}` sono altrettanto aperti. Inoltre una lobby ripresa senza giocatori scade dopo `empty_lobby_ttl` (5 min) e il save torna riprendibile, quindi chi è in LAN può ottenere un `hostToken` nuovo a ripetizione (ottavo giro, I14). Se la LAN smette di essere fidata, il piano di fix è il Task 1 della review del settimo giro (`hostToken` obbligatorio su elenco, resume ed eliminazione).
- **W3** Il layout schematico è un dato per mappa, non un asset del client: `data/layouts/<mapId>.json` (`width`, `height`, `cities` `{x,y}`, `labels`, `routes` con 2 o 3 punti, più `unit`, `font`, `doubleGap` opzionali), caricato all'avvio in `AppState.layout_documents`. Un file presente che non copre la sua mappa (o con dimensioni fuori da (0, 100000], coordinate e `dx`/`dy` con modulo oltre 100000, `unit`/`doubleGap` fuori da [0, 1000], etichette senza `dx`/`dy`/`anchor` validi) fa fallire l'avvio, e il client applica gli stessi limiti in `isMapLayout` («Layout della mappa non valido»); un file assente è ammesso e il client degrada: `404 LAYOUT_NOT_FOUND` → mappa pronta con `layout: null`, biglietti e nomi delle città disponibili, «Layout della mappa non disponibile» al posto del disegno. `GET /api/rulesets/{id}/layout` → documento + `rulesetId` + `mapId`; `404 RULESET_NOT_FOUND` / `404 LAYOUT_NOT_FOUND`. Per Europe basta aggiungere `data/layouts/europe.json`.
- **W4** `DELETE /api/saves/{id}` (nessun body) → `204`: eliminazione **soft**, il file viene rinominato in `<id>.deleted-<unix>.bak` (mai sovrascritto un backup esistente) ed esce dall'elenco ma resta recuperabile a mano. Errori: `404 SAVE_NOT_FOUND`, `409 GAME_ALREADY_ACTIVE` (il save di una partita attiva non si tocca), `503 SHUTTING_DOWN`. Controllo "non attiva" e rinomina stanno nella stessa sezione critica di `games.write()` del resume; un resume via REST verifica sotto lo stesso lock che `<id>.json` esista ancora, così un'eliminazione arrivata durante il replay non fa rinascere il save. Stessa fiducia del resume: chiunque in LAN.

**Regola di manutenzione: chi completa uno step o prende una decisione aggiorna questo RECAP (stato, prossimi fix, aperti, decisioni) e aggiunge la cronaca dettagliata del giro in coda a `docs/history.md`. Il RECAP resta corto.**

## 3. Mappa del piano (`docs/plan/ciufciuf-v1.3.md`)

| Sezione | Contenuto |
|---|---|
| §2 | Registro delle decisioni **D1–D23** (vincolanti); D24–D45 sono nel RECAP §4 |
| §3 | **Regole di gioco vincolanti**: USA §3.1, Europe §3.2, checklist edge case §3.3, varianti come parametri §3.4, altre mappe e regole della casa §3.5 |
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
- **D18 — Persistenza**: una partita in corso = un file JSON in `saves/` (seed + log azioni); a fine partita il file viene eliminato e restano solo i punteggi in `saves/results.jsonl`; niente DB.
- **D19 — Nome**: CiufCiuf! — **D20 — Profili e classifica**: `saves/profiles.json` (solo nomi) + classifica generale da `results.jsonl` (anticipata dalla Fase 5, 10/11/2026) — **D21 — Audio**: posticipato.
- **D24 — Standby Fase 3 senza TLS** (decisione utente 4/11): in Fase 3 niente HTTPS/Wake Lock; il meccanismo primario resta reconnect + `SNAPSHOT` su `visibilitychange` (D17). HTTPS self-signed + Wake Lock rinviati alla fase "aperta". Fase 3 suddivisa in due step: 3a connettività, 3b viste — entrambi completati.
- **D25 — Glow rotte legali §8.2 rinviato** (review 3b, 4/11): la table view è public-only e la mappa vive solo lì; `legalMoves` è canale privato → il glow richiederebbe leak di stato privato o una mini-mappa sul controller. Rinviato alla fase polish, non è un difetto.
- **D26 — Reset locomotive senza uscita** (decisione utente 9/11, caso non coperto dal piano §3.1): se dopo lo scarto delle scoperte mazzo + scarti contengono ≤2 carte non-locomotiva, ogni rivelazione mostrerebbe di nuovo ≥3 locomotive. Si rimescola una sola volta, si riscoprono 5 carte e ci si ferma anche con ≥3 locomotive scoperte; se dopo la riscoperta mazzo e scarti sono vuoti, si crea subito il mazzo nuovo D27 e il controllo si ripete una volta (ottavo giro, I13); lo stesso vale se la riscoperta crea il mazzo D27 e quando una pesca dal mazzo coperto lo crea (nono giro, J1). Con ≥3 non-locomotive il reset continua come sempre.
- **D27 — Mazzo nuovo a carte finite** (decisione utente 9/11, cambia il piano §3.1): se mazzo e scarti sono entrambi vuoti, si crea un mazzo completo dal ruleset e lo si mescola. Le carte in mano restano ai giocatori, quindi il totale in gioco può superare 110.
- **D28 — Ultimo giro senza rotte** (decisione utente 9/11, §3.1 "Fine partita"): se nessun giocatore può più reclamare alcuna rotta, a prescindere dalle carte, parte l'ultimo giro come con ≤2 treni.
- **D29 — Europe sul regolamento 2026** (Fase 4, regola "edizione più recente" come per USA V2 2025): DO7202N; spareggio totale → biglietti completati → European Express, senza il "meno stazioni usate" del 2015. Coincide con lo spareggio già nel motore.
- **D30 — Tunnel senza mazzo nuovo**: la rivelazione usa mazzo e, a mazzo vuoto, scarti rimescolati; con entrambi vuoti si rivelano meno carte (anche zero → nessun extra). D27 vale solo per la pesca.
- **D31 — Decisione tunnel**: costo extra 0 → rotta presa subito; costo > 0 → sempre `TUNNEL_RESOLUTION`, anche se la mano non basta (`can_pay: false`, solo Rinuncia).
- **D32 — Assegnazione stazioni**: massimo di biglietti completati − falliti; a parità la prima in ordine canonico (stazioni in ordine di costruzione, "nessuna rotta" prima, poi rotte per id). Stessa funzione per `completed` live e punteggio finale.
- **D33 — D28 con le stazioni**: l'ultimo giro scatta quando nessuno può reclamare rotte, anche se restano stazioni costruibili.
- **D34 — Modalità come ruleset**: ogni modalità è un file `data/rulesets/<id>.json` con `ticketSet`; i biglietti dichiarano in `sets` le modalità in cui sono in gioco. Fonti: USA 1910 (2018), Europa 1912 (2015), Europe 15° anniversario (2021) per 1912/Mega/Grandi Città.
- **D35 — Europe Estesa e 1912** (decisione utente 12/11): entrambe presenti; Estesa = regolamento 2015 (46 + 19 regolari nuovi), 1912 = anniversario 2021 (solo le 55 carte 1912).
- **D36 — Globetrotter**: +15 a chi completa più biglietti (stesso conteggio dello spareggio, stazioni incluse); parità → a tutti; nessuno se il massimo è 0.
- **D37 — USA 1910 con setup V2**: il regolamento 1910 dice «regole standard», quindi 4 biglietti tieni 2 come `usa_v2`.
- **D38 — Carte Paese (Poland)** (decisione utente 17/11): una presa assegna le carte quando la rete del giocatore che contiene la rotta raggiunge almeno due Paesi e ne include uno che non era in una rete già premiata prima della presa (rete premiata = componente con almeno due Paesi con pila). Una rotta interna o una seconda città di confine verso un Paese già in rete non danno carte; l'esito non dipende dall'ordine delle prese (prima: contava solo se la rotta entrava in un Paese). `RULES_VERSION` resta 2: nessun save Poland esistente, ma i save Poland creati con regole D38-precedenti possono divergere al replay.
- **D39 — Quote del draft iniziale private** (decisione utente 17/11): **superata** il 18/11 da D40: la pesca a quote (`chooseFrom`) è stata rimossa e nel draft iniziale i biglietti sono offerti subito.
- **D40 — Germany, regola della casa** (decisione utente 18/11, diversa dal regolamento ufficiale; piano §3.5): mano iniziale distribuita 2 lunghi + 3 corti, si tengono almeno 2 biglietti di cui almeno 1 corto (`keepMin` per voce di `initialTickets.dealt`, esposto in `TicketKeepLimit.min`). A draft finito i mazzi di `ticketDraw.mergeDecks` confluiscono in `deck` in una sola pila mescolata dall'RNG del motore; `DRAW_TICKETS` pesca 4, tieni ≥1, scarti (anche lunghi) in fondo alla pila. `RULES_VERSION` 3.
- **D41 — Colori giocatore** (decisione utente 18/11): `PINK`, `CYAN`, `LIME`, `BROWN`, `LAVENDER`, `INDIGO` (6, fuori dalla palette delle tratte) al posto di `RED`/`BLUE`/`GREEN`/`YELLOW`/`BLACK`.
- **D42 — Regole speciali come dato** (18/11): i ruleset che differiscono dal gioco base hanno `specialRules` (elenco di testi in italiano) servito da `GET /api/rulesets`; il client le mostra senza conoscerle (TV durante il draft + «📜 Regole», modale sul telefono).
- **D43 — Locomotive e pagamenti come dato** (18/11): `locomotives.usableOn` (`tunnel`/`ferry`; assente = ovunque), `freeDraw` (loco scoperta = carta normale), `faceUpReset` (reset con 3 loco scoperte); `ferries.anyCardsPerLocomotive` e `anyCardsPerSpace` per rotta. Un pagamento è valido solo se tutte le carte dichiarate si assegnano agli spazi; senza sostituzioni resta il percorso storico. Tunnel con `ferries` o `anyCardsPerSpace` rifiutati dalla validazione.
- **D44 — Biglietti multi-destinazione** (18/11, Switzerland): `destinations` (≥2) al posto di `to`+`points`; vale il massimo delle destinazioni collegate, altrimenti −minimo. Esiti per biglietto in `Engine::final_ticket_results` e `FinalScoreInfo.tickets`, non in `FinalScore` (hash dei golden invariato). `ticketDraw.discards: "removed"` per gli scarti a metà partita.
- **D45 — Gruppi paralleli con lunghezze diverse** (18/11, Nordic): ammessi; la stima ciclomatica conta ogni gruppo come uno span di lunghezza minima.

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
│   ├── rulesets/       # descriptor per ruleset
│   └── layouts/        # layout schematico per mappa (servito da /api/rulesets/{id}/layout)
├── client/             # TypeScript + Svelte 5 (D22)
├── docs/plan/          # piano ufficiale (v1.3; la v1.2 superata è stata rimossa l'11/11/2026)
└── docs/history.md     # cronaca dei giri (fasi, round UX, robustezza) spostata dal RECAP
```

## 7. Workflow operativo (per agenti)

- **Planner** (orchestra, confeziona pacchetti di contesto) → **coding** (implementa + self-check: build/test/lint prima dell'handoff) → **reviewer** (review diff-based: correttezza, memory leak, errori, logica; non modifica). Rework mirato se la review trova problemi.
- Prima di scrivere codice: leggere le sezioni del piano citate dal pacchetto (di solito §3 regole, §4 dati, §6 stati, §7 protocollo).
- **Aggiornare questo RECAP** a ogni decisione o step completato; la cronaca va in `docs/history.md`.
- tokensave è inizializzato in questo repo: dopo modifiche al codice eseguire `tokensave sync` dalla root (`.tokensave/` è nel `.gitignore`).

## 8. FAQ rapida

- **Perché Svelte?** Scelta dell'utente (D22): bundle piccolo, reattività semplice per la mano privata, SVG naturale. Se delude, si rimpiazzano due viste e basta.
- **Perché il repo è privato?** D23: è la documentazione personale del progetto; il piano §12 nota che potrebbe diventare pubblico senza toccare IP altrui, ma non è previsto.
- **Dove sono le regole precise?** Piano §3 (vincolante) e §9 (algoritmi). Dati di mappa/ruleset: §4.
