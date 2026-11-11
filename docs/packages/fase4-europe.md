# Fase 4 — Europe: pacchetto di contesto

Pacchetto del planner per la pipeline coding → reviewer. Fonte vincolante: piano §3.2 (regole Europe),
§4 (dati), §6 (stati 7 e 8), §7 (contratto), §9.2 (stazioni) e le decisioni D29–D33 qui sotto.

## Regolamento di riferimento

Regolamento Europe più recente: **DO7202N, edizione 2026** (cdn.svc.asmodee.net, `DO7202N_TICKET2RIDE_EUROPE_RULES_EN_20260512`).
Rispetto al piano §3.2 (scritto sull'edizione 2015) l'unica differenza di regola è lo spareggio (D29).

| # | Decisione | Motivo |
|---|---|---|
| D29 | Europe segue l'edizione **2026**. Spareggio: totale → biglietti completati → European Express; **niente** "meno stazioni usate" (era nel 2015). Coincide con lo spareggio USA già nel motore: nessun parametro nuovo | Regola utente "edizione più recente" (come USA V2 2025) |
| D30 | La rivelazione di un tunnel **non** crea un mazzo nuovo (D27 vale solo per la pesca): si rivela dal mazzo, a mazzo vuoto si rimescolano gli scarti, con mazzo e scarti vuoti si rivelano meno carte (anche zero → nessun costo extra) | Regolamento: "only those cards that are available are revealed" |
| D31 | Tunnel con costo extra 0 → rotta reclamata subito, nessuna `TUNNEL_RESOLUTION`. Costo extra > 0 → sempre `TUNNEL_RESOLUTION`, anche se la mano non basta (`can_pay: false`, resta solo la rinuncia) | Una sola strada per la decisione; il client non deve indovinare |
| D32 | Stazioni a fine partita: per ogni giocatore si sceglie l'assegnazione stazione → rotta avversaria che **massimizza** biglietti completati − falliti; a parità vince la prima in ordine canonico (stazioni in ordine di costruzione, per ciascuna prima "nessuna rotta", poi le rotte per id crescente). La stessa funzione alimenta `tickets[].completed` in partita | Piano §9.2; ordine canonico = determinismo e prestiti mostrati solo se servono |
| D33 | D28 invariata: l'ultimo giro scatta quando nessuno può reclamare rotte, anche se si possono ancora costruire stazioni | Le stazioni non consumano treni: senza rotte la partita non avanza |

## Dati (già in repo, verificati dal planner)

- `data/maps/europe.json`: 47 città (id ASCII, nomi del tabellone 2026: «Kyïv», «Zágráb», «Athína», «København», …, `lat`/`lon` reali), **101 rotte** (11 coppie doppie, 18 tunnel, 13 traghetti; 8 rotte per ognuno degli 8 colori + 37 grigie; lunghezze 1/2/3/4/6/8), **46 biglietti** (6 `long` + 40 `regular`).
  Campi nuovi sulle rotte: `"tunnel": bool`, `"ferries": u32` (locomotive obbligatorie). Fonti incrociate: tallguy-h/LibreLine `docs/maps/europe.md` (dati dell'app), brts-u/TicketToRide, 11samype/TicketToRideEurope, leonsi7/ticket-to-ride-europe; discrepanze risolte a maggioranza (Sochi–Erzurum tunnel rosso, Rostov–Sevastopol non tunnel, Stockholm–Wien 11, Palermo–Smyrna traghetto 6/2).
- `data/rulesets/europe.json`: come piano §4.1 più
  `"stations": { "count": 3, "costs": [1, 2, 3], "unbuiltPoints": 4 }` e `"tunnels": { "revealed": 3 }`.
- `ttr-validate` (codice attuale) passa: numero ciclomatico stimato **12** (massimo 14).

## Contratto WS (già in `ttr-proto`, binding rigenerati e copiati nel client)

- `InboundAction::BuildStation(BuildStationPayload { player_id, city_id, cards_discarded, client_seq? })` — `"BUILD_STATION"`.
- `InboundAction::TunnelDecision(TunnelDecisionPayload { player_id, pay, cards_discarded, client_seq? })` — `"TUNNEL_DECISION"`.
- `LegalMoves` + `station_cities: Vec<CityId>`, `station_cost: u32`, `tunnel: Option<TunnelDecisionMoves { route_id, extra_cost, color, can_pay }>`.
- `TunnelReveal { route_id, cards, color, extra_cost }` in `PublicState.tunnel_reveal` durante `TUNNEL_RESOLUTION`.
- `PublicState.stations: Vec<StationPlacement { city_id, player_id }>`, `PlayerSummary.stations_remaining`.
- `FinalScoreInfo` + `station_points: i32`, `borrowed_routes: Vec<BorrowedRoute { cityId, routeId }>` (camelCase come il resto di `FinalScoreInfo`).
- `GameState::TunnelResolution` esiste già; `GameState::StationScoring` resta inutilizzato (il punteggio stazioni è calcolato dentro il game over, nessun input).

## Motore (`ttr-core`)

**Tipi.** `Route` + `#[serde(default)] tunnel: bool`, `#[serde(default)] ferries: u32` (i JSON USA restano validi).
`Ruleset` + `stations: Option<StationsConfig { count, costs: Vec<u32>, unbuilt_points (unbuiltPoints) }>`,
`tunnels: Option<TunnelsConfig { revealed: u32 }>`, entrambi `#[serde(default)]`.
`PlayerState` + stazioni costruite in ordine di costruzione. Indice globale città → giocatore della stazione.
`GamePhase::TunnelResolution`.

**Validazione** (`check_map`/`check_setup`, nuove varianti di `EngineInitError`): `ferries <= len`; una rotta non è insieme tunnel e traghetto; rotte tunnel ⇒ `tunnels` presente con `revealed >= 1`; `stations` ⇒ `costs.len() == count` e ogni costo ≥ 1.

**Azioni** (`Action`, serializzate nei save: solo varianti nuove, quelle esistenti invariate):
- `BuildStation { player_index, city_id, cards_discarded }` — fasi `PlayerTurn`/`LastRound`, giocatore di turno; ruleset con stazioni e stazioni residue; città esistente e senza stazioni di nessuno; esattamente `costs[costruite]` carte, le non-locomotive tutte dello stesso colore. Stessa convenzione del claim: una carta dichiarata assente in mano è sostituita da una locomotiva, la validazione dei colori va fatta sulle carte **realmente tolte**. Carte agli scarti, `StationBuilt`, `advance_turn`. Le stazioni non consumano treni.
- `ClaimRoute` invariato per le rotte normali, più:
  - traghetti: le carte realmente tolte contengono almeno `ferries` locomotive, altrimenti `InsufficientCards`;
  - tunnel: dopo le stesse validazioni del claim, le carte giocate escono dalla mano e restano "sul tavolo" nel pendente; colore del tunnel = colore delle non-locomotive tolte, `Locomotive` se sono tutte locomotive; si rivelano fino a `tunnels.revealed` carte (D30, `DeckReshuffled` se rimescola); `extra` = rivelate uguali al colore o locomotive (se il colore è `Locomotive`: solo locomotive). `extra == 0` → claim completato subito (D31); altrimenti `phase = TunnelResolution` con pendente `{ route_id, played, revealed, color, extra }`. Evento `TunnelRevealed { player_index, route_id, cards, color, extra_cost }` in entrambi i casi, prima di `RouteClaimed`.
- `TunnelDecision { player_index, pay, cards_discarded }` — solo in `TunnelResolution`, giocatore di turno.
  `pay: true`: `cards_discarded.len() == extra`, ogni carta del colore del tunnel o locomotiva (solo locomotive se il colore è `Locomotive`), tolte con la convenzione del claim; se non bastano → errore e stato invariato (si può ancora rinunciare). Poi claim completato come una rotta normale.
  `pay: false`: carte giocate di nuovo in mano, `TunnelDeclined`, turno finito.
  Ordine di scarto deterministico: giocate, extra, rivelate (rinuncia: solo rivelate). Le rivelate restano fuori dagli scarti finché il tunnel è pendente.
- Il pendente fa parte dello stato: entra in `serialize_state` (replay) insieme alle stazioni.

**Mosse legali.** `routes`: per i traghetti `locomotive >= ferries` e il resto pagabile; i tunnel compaiono se il costo base è pagabile. `station_cities`: in `PlayerTurn`/`LastRound`, se restano stazioni e la mano copre il costo (un colore + locomotive, o sole locomotive), tutte le città senza stazione in ordine di id; `station_cost` = costo della prossima (0 se finite). `tunnel`: solo in `TunnelResolution` per il giocatore di turno, con `can_pay` calcolato sulla mano corrente. Ogni mossa elencata deve essere accettata da `apply` (invariante dei fuzz).

**Punteggio.** Biglietti con stazioni secondo D32, per i candidati: rotte reclamate da **avversari** incidenti alla città della stazione. Il completamento live dei biglietti (`TicketStatus.completed`) usa la stessa funzione e va ricalcolato per **tutti** i giocatori con stazioni dopo ogni claim (una rotta avversaria nuova può completare un biglietto altrui) e per il giocatore dopo una stazione. `FinalScore` + `station_points` (stazioni non costruite × `unbuiltPoints`) e `borrowed_routes: Vec<(CityId, RouteId)>`; `total` le include. Il percorso più lungo resta sulle sole rotte proprie. Spareggio invariato (D29).

**USA invariata.** Con `tunnels`/`stations` assenti e rotte senza tunnel/traghetti il comportamento e la sequenza RNG restano identici: tutti i test esistenti (golden, replay, proptest) verdi senza ritocchi ai valori attesi. `RULES_VERSION` resta 2.

## Server (`ttr-server`)

`inbound_to_core_action` e `extract_client_seq` per le due azioni nuove; `bridge`: `GamePhase::TunnelResolution → GameState::TunnelResolution`, `tunnel_reveal` dal pendente, `stations` (ordine città), `stations_remaining` dal motore, i tre campi nuovi di `LegalMoves`, `station_points`/`borrowed_routes` in `FinalScoreInfo`; il `max_trains = 45` di `build_players_summary` diventa `trainsPerPlayer` del ruleset; nuove `EngineError` mappate a `ErrorReason` esistenti con `detail` leggibile.

## Client (`client/`)

Abilitare `europe` in `lib/rulesets.ts`. Mappa: tunnel e traghetti riconoscibili (tratteggio/contorno per i tunnel, icona locomotiva sui primi `ferries` slot), stazioni come segnaposto nel colore del giocatore. Controller: pagamento claim con locomotive obbligatorie dei traghetti; costruzione stazione (città tra `station_cities`, carte per `station_cost`); dialog tunnel (carte rivelate, costo extra, Paga/Rinuncia, solo Rinuncia con `can_pay: false`); stazioni residue. TV: carte rivelate del tunnel con evidenza di quelle che aumentano il costo; ripartizione finale con stazioni e rotte prese in prestito. Regole solo da `legalMoves` (regola d'oro 5).

## Layout

`data/layouts/europe.json` dal generatore `client/tools/gen-schematic-layout.mjs` parametrizzato per mappa; l'output USA resta identico byte per byte.

## Accettazione (piano §10, Fase 4)

Partita Europe completa su LAN fino alla schermata punti, con tunnel, traghetti e stazioni; USA invariata.
