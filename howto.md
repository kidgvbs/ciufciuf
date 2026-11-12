# HOWTO — promemoria personale

Note per me su come usare server, API e client di CiufCiuf!. Non è documentazione del progetto: per quella ci sono `README.md`, `RECAP.md` e `docs/`. Se qualcosa qui non torna con il codice, vale il codice.

Tutti i comandi partono dalla radice del repo. Per bun: `export PATH=$HOME/.bun/bin:$PATH`.

---

## 1. Avviare tutto

### Prima volta e dopo ogni modifica

```bash
(cd client && bun install --frozen-lockfile && bun run build)
cargo build --release
./target/release/ttr-server --port 8080 --data-dir ./data --saves-dir ./saves --client-dist ./client/dist
```

- Il server serve `client/dist`: se tocco il client devo rifare `bun run build`, se tocco Rust `cargo build --release`.
- Se macOS chiede di consentire le connessioni in entrata a `ttr-server`, consento.
- Il registry del client è fissato in `client/bunfig.toml` (`registry.npmjs.org`). Non cambiarlo, anche se in locale uso il mirror aziendale: la CI controlla che `bun.lock` abbia solo URL pubblici.

### Opzioni del server

| Opzione | Default | Cosa fa |
|---|---|---|
| `--port` | `8080` | Porta TCP |
| `--bind` | `0.0.0.0` | Indirizzo su cui ascoltare (`127.0.0.1` per solo locale) |
| `--data-dir` | `data` | Mappe (`maps/`) e ruleset (`rulesets/`) |
| `--saves-dir` | `saves` | Save in corso, `profiles.json`, `results.jsonl` |
| `--client-dist` | `client/dist` | File statici del client |
| `--resume <file>` | | Riprende una partita da un save |
| `--allow-client-seed` | spento | Permette `seed` in `POST /api/games` (test, partite riproducibili) |
| `--deleted-saves-retention-days <N>` | 30 | Giorni dopo i quali all'avvio cancella i `*.deleted-*.bak`; `0` = mai |

Valori sbagliati o mancanti = errore e uscita ≠ 0. `--help` elenca tutto.
Log: `RUST_LOG=info` (o `debug`) per vedere di più.

### Fermare il server

`Ctrl-C` (SIGINT) o SIGTERM: smette di accettare creazioni, join e WebSocket (503), ferma le partite, aspetta le scritture in coda e chiude entro 15 s. Un secondo `Ctrl-C` esce subito (codice 130) e i save in coda possono non essere su disco: da evitare.

---

## 2. Giocare una partita

1. Sulla TV apro `http://<ip-del-mac>:8080` (la home mostra la classifica) e creo la partita.
2. Ogni telefono (stessa Wi-Fi) scansiona il QR. Da 2 a 5 giocatori.
3. Sul telefono: **Chi sei?** → tocco il mio nome; la prima volta **+ Nuovo profilo**. Il colore si sceglie a ogni partita.
4. Quando ci sono tutti, avvio dalla TV.

Cose da ricordare:

- Un profilo è solo un nome, senza password: chiunque può toccare qualsiasi nome. È una scelta di design.
- "Esci" in partita chiede conferma, perché fa perdere il posto.
- Un telefono disconnesso (standby, Wi-Fi) rientra da solo e riceve lo stato aggiornato.
- Se chiudo la scheda della TV **prima dell'avvio**, perdo l'`hostToken` (sta in `sessionStorage`): la TV offre "Crea una nuova partita" e la lobby vecchia scade da sola (30 min, 5 se è vuota).
- Se annullo un join lento e la richiesta riesce in ritardo, il telefono salva comunque l'identità (senza cambiare schermata): riprovando con lo stesso nome riprendo quel giocatore. Solo se la risposta non arriva mai il nuovo tentativo riceve `NAME_TAKEN` e l'host deve riassegnarlo (vedi §5).
- Una partita finisce quando scatta l'ultimo giro (un giocatore a ≤ 2 treni, oppure nessuno può più reclamare rotte, D28). Se chi può ancora reclamare non lo fa, non finisce.

### Nomi (giocatori e partite)

- Max 24 caratteri per i giocatori, 40 per le partite; NFKC, spazi collassati.
- Rifiutati (`400 INVALID_NAME`): caratteri di controllo/invisibili/"vuoti", combinanti iniziali o più di 2 di fila, parole con alfabeti misti (anche "Ωmega", "서울Seoul").
- Emoji ok: `U+FE0F` solo dopo un emoji, `U+200D` solo tra due emoji.
- Due nomi "uguali a vista" sono lo stesso nome (`409 NAME_TAKEN`): "Tim"/"Tirn", "Leo"/"Le0", "Player1"/"Playerl", cirillico/latino. In classifica finiscono in una riga sola.

---

## 3. Dati su disco (`--saves-dir`)

| File | Contenuto |
|---|---|
| `<id>.json` | Save di una partita **in corso**: seed, log delle azioni, per ogni giocatore solo l'hash SHA-256 del token |
| `<id>.resume-<timestamp>.bak` | Copia del save a ogni resume, senza duplicare se l'ultima è identica. Ne restano al massimo 5 per partita (i più vecchi vengono cancellati) |
| `<id>.deleted-<timestamp>.bak` | Save eliminato dalla TV. Per recuperarlo lo rinomino in `<id>.json`. All'avvio il server cancella quelli più vecchi di `--deleted-saves-retention-days` (30 giorni) |
| `profiles.json` | Profili (max 200). Se illeggibile viene spostato in `profiles.json.corrupt-<unix>`, mai sovrascritto |
| `results.jsonl` | Una riga per partita conclusa (nome, totale, vincitore). Una partita compare una volta sola |

- A fine partita: il risultato va in `results.jsonl`, poi il save viene eliminato. Se l'archiviazione fallisce, il save resta con `status: completed` e viene archiviato **all'avvio successivo** (i `*.tmp` più vecchi di un'ora vengono rimossi).
- Vincitore: totale più alto; a parità più biglietti completati, poi bonus del percorso più lungo.
- Una partita senza connessioni per 6 ore sparisce dall'elenco ma il save resta e si può riprendere.

---

## 4. Riprendere una partita salvata

**Dalla TV:** nella home, sezione "Partite salvate", premo **Riprendi** sulla partita. La TV entra nella lobby con l'`hostToken` (in `sessionStorage`), quindi "Riassegna" e "Start" funzionano senza copiare nulla. Con **Elimina** (e conferma) tolgo un save dall'elenco: il file diventa `saves/<id>.deleted-<timestamp>.bak`, recuperabile a mano rinominandolo. Elenco: `GET /api/saves`; ripresa: `POST /api/saves/{id}/resume`; eliminazione: `DELETE /api/saves/{id}` (§5).

**Da riga di comando:**

```bash
./target/release/ttr-server --data-dir ./data --saves-dir ./saves --client-dist ./client/dist \
  --resume saves/<id>.json
```

- Il log stampa l'id e un **`hostToken` nuovo**: sta solo lì. Lo copio subito.
- Una partita già in lobby non si riprende due volte (`409 GAME_ALREADY_ACTIVE`).
- Rifiutati: save di una partita già conclusa («partita già conclusa») e save con `RULES_VERSION` vecchia o assente («save creato con regole precedenti: non riproducibile»). Oggi `RULES_VERSION` è 2.
- Finché non sono rientrati tutti, lo start è rifiutato (`409 ROSTER_INCOMPLETE`).
- Rientro: ogni giocatore fa join con **nome + token originale** (il telefono ce l'ha in `localStorage`) e riottiene lo stesso `playerId`/token. Solo nome → `401 RESUME_TOKEN_MISMATCH`.
- Telefono senza token, o save di formato 1 (senza hash): serve `reassign` dall'host (sotto). Il vecchio token smette di funzionare. Sulla TV ci sono i pulsanti "Riassegna", che mostrano un QR con l'identità; il campo "Token host" compare solo se la TV non ha l'`hostToken` (resume da riga di comando).

### Rientro in una partita avviata (anche non ripresa)

- Dopo lo start nessun nuovo giocatore entra: un nome che non è nella partita dà `409 GAME_STARTED`.
- Chi è già nella partita rientra col join (`POST /api/games/{id}/join {name, token}`): nome nel roster e token attuale → `200` con lo stesso `playerId` e token, nessuna modifica al roster. Nome presente ma token assente o sbagliato → `401 RESUME_TOKEN_MISMATCH`.
- Sul telefono "Esci" in partita tiene l'identità: la partita avviata compare nell'elenco come "↩ rientra" (le altre avviate restano 🔒) e si rientra scegliendola di nuovo. Dopo "Esci" il telefono non si riconnette da solo al ricaricamento della pagina. "Non sono io", "Esci" in lobby e "Nuova partita" dimenticano l'identità.
- Telefono senza identità (cache cancellata, altro telefono): dalla TV, nel segnapunti, "Riassegna" accanto a ogni giocatore offline (solo se la TV ha l'`hostToken`) mostra il QR di rientro. `reassign` su una partita avviata non ripresa vale solo per i giocatori del roster (nome sconosciuto → `409 NAME_NOT_IN_SAVE`); chiude le connessioni del giocatore e il vecchio token smette di funzionare.

---

## 5. API REST

Base: `http://<host>:8080`. Corpo e risposte JSON (`Content-Type: application/json`).
Errore standard: `{"error": "<CODICE>", "detail": "<messaggio in italiano>"}`.

| Metodo e percorso | Corpo | Risposta |
|---|---|---|
| `GET /api/lan` | | `{port, candidates:[{ip,url,preferred}], fallback}`: gli URL LAN da mostrare nel QR |
| `GET /qr?data=<url>` | | SVG del QR (il client oggi lo genera da solo, nel browser) |
| `POST /api/games` | `{rulesetId, name?, seed?}` | `201 {gameId, hostToken, rulesetId, maxPlayers}` |
| `GET /api/games` | | `{games:[{gameId,name,rulesetId,playersJoined,maxPlayers,started}]}` |
| `GET /api/games/{id}` | | `{gameId,name,rulesetId,players:[{playerId,name,color}],started,maxPlayers,missingPlayers}` |
| `POST /api/games/{id}/join` | `{name, color?, token?}` | `{playerId, token, wsUrl}` |
| `POST /api/games/{id}/start` | `{hostToken}` | `204` |
| `POST /api/games/{id}/reassign` | `{hostToken, name}` | `{playerId, token}` (token nuovo); in lobby solo per partite riprese, a partita avviata per ogni giocatore del roster |
| `DELETE /api/games/{id}` | `{hostToken}` | `204` (un save su disco resta) |
| `GET /api/rulesets` | | `{rulesets:[{id,name,mapId,cities,routes,tickets,minPlayers,maxPlayers,bonusLabel,tunnels,ferries,stations}]}` per id: il catalogo del selettore mappe (`name` dal campo `name` di `data/rulesets/<id>.json`) |
| `GET /api/rulesets/{id}/map` | | Documento della mappa (`cities`, `routes`, `tickets`, con `lat`/`lon`) più `rulesetId` |
| `GET /api/rulesets/{id}/layout` | | Layout schematico della mappa (`width`, `height`, `cities`, `labels`, `routes` con 2–3 punti) più `rulesetId` e `mapId`; mappa senza layout → `404 LAYOUT_NOT_FOUND` |
| `GET /api/saves` | | `{saves:[{gameId,name,rulesetId,players:[nome,…],createdAt,updatedAt}]}`: save `in_progress` riprendibili (regole attuali, ruleset noto) e non già attivi, dal più recente; i file illeggibili sono saltati con un warn |
| `POST /api/saves/{id}/resume` | | `201 {gameId, hostToken, rulesetId, maxPlayers}` (come la creazione) |
| `DELETE /api/saves/{id}` | | `204`: il save diventa `<id>.deleted-<unix>.bak`; partita attiva → `409 GAME_ALREADY_ACTIVE` |
| `GET /api/standings` | | `{games, standings:[{name,games,wins,total_points,best}]}` |
| `GET /api/profiles` | | `{profiles:[nome,…]}` (illeggibile → lista vuota + warn nel log) |
| `GET /ws/{gameId}?playerId=&token=` | | Upgrade WebSocket (§6) |

Note:

- `rulesetId`: `usa_v2` o `europe`.
- `color` è uno tra `RED`, `BLUE`, `GREEN`, `YELLOW`, `BLACK` (maiuscolo; un valore diverso → `422` con testo semplice di axum, non JSON); se manca il server assegna il primo libero; se è esplicito e già preso → `409 COLOR_TAKEN`.
- `seed` senza `--allow-client-seed` → `400 SEED_NOT_ALLOWED`. Il seed di default è a 64 bit da un CSPRNG, finisce solo nel save e mai nelle API: con il seed si ricostruiscono le mani di tutti.
- `DELETE` senza body → `401 UNAUTHORIZED`; body malformato → `400`.
- Un percorso `/api/*` inesistente (anche con lo slash finale) → `404 {"error":"NOT_FOUND","detail":"Endpoint inesistente."}`, mai la pagina della SPA.
- Su `join` in modalità resume `token` è obbligatorio.

### Codici d'errore

| Codice | HTTP | Quando |
|---|---|---|
| `GAME_NOT_FOUND` | 404 | id inesistente o partita già chiusa |
| `RULESET_NOT_FOUND` | 404 | ruleset inesistente |
| `LAYOUT_NOT_FOUND` | 404 | la mappa del ruleset non ha un file in `data/layouts/` |
| `NOT_FOUND` | 404 | endpoint `/api/*` inesistente |
| `SAVE_NOT_FOUND` | 404 | resume o eliminazione di un id non canonico o senza file `saves/<id>.json` |
| `GAME_ALREADY_ACTIVE` | 409 | resume o eliminazione di una partita già in lobby o in corso |
| `SAVE_NOT_RESUMABLE` | 409 | save concluso, illeggibile, con regole precedenti o con `gameId` diverso dal nome del file (`detail` spiega) |
| `GAME_FULL`, `GAME_STARTED`, `ROSTER_INCOMPLETE`, `COLOR_TAKEN`, `NAME_TAKEN`, `NAME_NOT_IN_SAVE` | 409 | conflitti di lobby |
| `UNAUTHORIZED`, `RESUME_TOKEN_MISMATCH` | 401 | `hostToken` o token sbagliato |
| `INVALID_NAME`, `BAD_REQUEST`, `MIN_PLAYERS`, `SEED_NOT_ALLOWED` | 400 | input non valido, meno di 2 giocatori allo start |
| `TOO_MANY_GAMES` | 429 | più di 20 partite attive (le concluse non contano) |
| `SHUTTING_DOWN` | 503 | server in chiusura |
| `INTERNAL_ERROR` | 500 | dettaglio nel log |

### Esempi con curl

```bash
B=http://localhost:8080
G=$(curl -s -X POST $B/api/games -H 'content-type: application/json' -d '{"rulesetId":"usa_v2","name":"Prova"}')
ID=$(echo $G | jq -r .gameId); HOST=$(echo $G | jq -r .hostToken)
curl -s -X POST $B/api/games/$ID/join -H 'content-type: application/json' -d '{"name":"Anna","color":"RED"}'
curl -s -X POST $B/api/games/$ID/join -H 'content-type: application/json' -d '{"name":"Bob"}'
curl -s -X POST $B/api/games/$ID/start -H 'content-type: application/json' -d "{\"hostToken\":\"$HOST\"}"
curl -s $B/api/games | jq
curl -s -X DELETE $B/api/games/$ID -H 'content-type: application/json' -d "{\"hostToken\":\"$HOST\"}"
```

### Limiti e durate

| Cosa | Valore |
|---|---|
| Lobby inattiva | 30 min senza join/start/reassign riusciti o connessione autenticata di un giocatore |
| Lobby senza giocatori | 5 min, anche con spettatori collegati |
| Partita avviata senza connessioni | 6 h (poi sparisce, il save resta) |
| Partite attive | 20 |
| Spettatori per partita | 8 (oltre: `ERROR GAME_FULL` con detail `TOO_MANY_SPECTATORS` e chiusura) |
| Connessioni per giocatore | 4 (la quinta chiude la più vecchia) |
| Messaggio WS | 16 KiB max (oltre: chiusura) |
| Ping / silenzio massimo | ogni 15 s / scollega dopo 45 s |
| Invio su socket che non assorbe | 10 s poi chiusura |

Solo richieste riuscite e autenticate rinnovano il TTL della lobby: join falliti, token errati e spettatori anonimi non contano.

---

## 6. WebSocket

- Giocatore: `ws://<host>:8080/ws/<gameId>?playerId=<id>&token=<token>`.
- Spettatore (la TV): stesso URL **senza** `playerId` e `token`; vede solo lo stato pubblico.
- L'identità è quella della connessione: il `player_id` nel payload non autorizza nulla.

Messaggi dal server (`type`): `PUBLIC_STATE` (broadcast), `PRIVATE_STATE` (solo al giocatore: mano, biglietti, `legalMoves`), `SNAPSHOT` (completo a connect e su richiesta), `ERROR` (`GAME_NOT_FOUND`/`UNAUTHORIZED` prima dello snapshot chiudono la connessione).

Azioni dal client (`actionType`, ogni payload ha `player_id` e `client_seq?` opzionale e crescente per la dedup):

```json
{"actionType":"DRAW_TRAIN_CARD","player_id":"…","source":"deck"}
{"actionType":"DRAW_TRAIN_CARD","player_id":"…","source":{"faceUp":{"index":2}}}
{"actionType":"DRAW_TICKETS","player_id":"…"}
{"actionType":"CHOOSE_TICKETS","player_id":"…","keep":["id_biglietto","…"]}
{"actionType":"CLAIM_ROUTE","player_id":"…","route_id":"a_b_1","cards_discarded":["RED","RED","LOCOMOTIVE"]}
{"actionType":"REQUEST_SNAPSHOT","player_id":"…"}
{"actionType":"BUILD_STATION","player_id":"…","city_id":"paris","cards_discarded":["RED","LOCOMOTIVE"]}
{"actionType":"TUNNEL_DECISION","player_id":"…","pay":true,"cards_discarded":["BLACK"]}
```

- Il client non conosce le regole: le mosse legali arrivano nel `PRIVATE_STATE`. Il pagamento di `CLAIM_ROUTE` è un suggerimento; il motore usa una locomotiva se manca una carta del colore.
- Un `client_seq` ≤ all'ultimo già visto è un duplicato: viene scartato e il server rimanda l'ultimo `PRIVATE_STATE`. Il client lo omette per `REQUEST_SNAPSHOT`.
- Stati (`GameState`): `WAITING_FOR_PLAYERS`, `INITIAL_DRAFT`, `PLAYER_TURN`, `SECOND_CARD_DRAW`, `TICKET_CHOICE`, `LAST_ROUND`, `TUNNEL_RESOLUTION` (Europe), `GAME_OVER`.
- Europe: `BUILD_STATION` è l'azione del turno, con `legalMoves.station_cost` carte di un colore (locomotive jolly) su una città di `legalMoves.station_cities`. Un `CLAIM_ROUTE` su un tunnel rivela fino a 3 carte (`PUBLIC_STATE.tunnel_reveal`): con costo extra 0 la rotta è presa subito, altrimenti si entra in `TUNNEL_RESOLUTION` e il giocatore risponde con `TUNNEL_DECISION` (`pay: false` = carte giocate di nuovo in mano, turno finito; `legalMoves.tunnel.can_pay` dice se la mano basta). I traghetti richiedono `ferries` locomotive nel pagamento.
- I tipi veri sono in `crates/ttr-proto` e, generati, in `crates/ttr-proto/bindings` e `client/src/lib/proto`.

Reconnect del client (`client/src/lib/ws.ts`): backoff da 0,5 a 30 s con jitter, che si azzera solo dopo 10 s di connessione stabile; al ritorno in primo piano (o evento `online`) chiede uno snapshot e, se non arriva nulla in 3 s, ricrea il socket (anche se è fermo in `CONNECTING` oltre 5 s). Gli `ERROR` fatali mostrano il messaggio e fermano i tentativi.

---

## 7. Client (`client/`)

Svelte 5 + Vite + TypeScript. Due viste: `/` (TV) e `/controller` (telefono).

```bash
cd client
bun install --frozen-lockfile
bun run dev            # Vite in dev; proxy /api e /ws verso localhost:8080 (il server va avviato a parte)
bun run build          # produce dist/, servito dal server
bun run check          # svelte-check (tipi anche dei .svelte)
bun run test           # vitest
bun run copy-bindings  # copia i binding ts-rs da crates/ttr-proto/bindings
```

- `?server=` non esiste più: in dev si passa dal proxy di Vite.
- Identità (telefono): `localStorage`, una chiave per partita (`ciufciuf-identity:<gameId>`), scade dopo 24 h; sulla schermata di rientro c'è "Non sono io". L'`hostToken` della TV sta in `sessionStorage`.
- Mappa e biglietti arrivano da `GET /api/rulesets/{id}/map`, il disegno schematico da `GET /api/rulesets/{id}/layout` (`data/layouts/<mapId>.json`): le due fetch partono insieme, sotto lo stesso timeout di 10 s. Anche l'elenco dei ruleset (card del selettore, nome del bonus percorso più lungo) arriva dal server: `GET /api/rulesets`.
- Il QR si genera nel browser (`qrcode-generator`): nessun token finisce in una query string.
- La CSP è in `client/index.html`.

### Script end-to-end (con bun)

```bash
bun client/smoke-test.mjs http://localhost:8080          # server già avviato con --allow-client-seed
bun client/tools/fullgame-test.mjs --spawn-server        # partita completa con 2 bot, dopo la build release
cd client && bun tools/gen-schematic-layout.mjs          # rigenera data/layouts/usa.json (~3 min); solo se cambia la geometria
```

---

## 8. Motore, dati e strumenti

```
crates/ttr-core/      motore puro e deterministico (più il binario ttr-cli per il debug)
crates/ttr-server/    Axum/Tokio: REST, WebSocket, save/resume, statici
crates/ttr-proto/     tipi del protocollo + export TypeScript (ts-rs)
crates/ttr-validate/  validazione di mappe e ruleset
data/                 maps/, rulesets/ e layouts/ (JSON)
```

- Validare i dati: `./target/release/ttr-validate data/maps/usa.json data/rulesets/usa_v2.json`. Stampa ✓ solo per i controlli passati; exit 1 se qualcosa non va.
- Il server esegue le stesse validazioni all'avvio e **non parte** con dati incoerenti. Controlli principali: id unici (città, rotte, biglietti, giocatori, colori), città esistenti, `len` ≥ 1 con voce in `routePoints`, biglietti e rotte con `from != to`, `trainsPerPlayer` ≥ 1, rotte doppie `_1`/`_2` appaiate, deck dei biglietti coerenti, numero ciclomatico stimato ≤ 14 (la USA stima 13).
- Convenzioni dei dati: `docs/data-conventions.md`. Piano e regole vincolanti: `docs/plan/ciufciuf-v1.3.md`. Specifica del server: `docs/packages/fase2-ttr-server.md`.
- `ttr-cli` (in `ttr-core`): debug del motore da terminale; un argomento mancante dà exit 2.

### Determinismo: cosa non toccare

- Una partita è seed + log delle azioni; il replay deve dare lo stesso stato.
- `rand = "=0.8.8"` e `rand_chacha = "=0.3.1"` determinano lo shuffle e quindi ogni save: **mai** aggiornarli senza alzare `RULES_VERSION` (`crates/ttr-core/src/lib.rs`). Renovate li ignora apposta.
- Riordinare o aggiungere voci in `ticket_decks` di un ruleset esistente cambia il replay.
- Niente iterazione su `HashMap` nei percorsi che toccano stato o serializzazione.
- Se cambio i tipi di `ttr-proto`: `cargo test -p ttr-proto && bash client/scripts/copy-bindings.sh` e committo entrambe le cartelle di binding (la CI controlla la deriva, anche per file nuovi o rimossi).

---

## 9. Verifiche prima di committare

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --release -p ttr-core -- --ignored longest_path_      # tempi del percorso più lungo
(cd client && bun run check && bun run test && bun run build)
```

Fuzz lungo del motore: `FUZZ_GAMES_FULL=6000 cargo test -p ttr-core --release --test integration_tests fuzzing_1000_games_explicit`.

La CI (`.github/workflows/ci.yml`) esegue fmt, clippy, test, drift dei binding, `cargo deny`, e per il client `bun install --frozen-lockfile`, check, test e build. Renovate (`renovate.json`) apre PR raggruppate: le major del client si approvano dalla Dependency Dashboard; la prima PR di pin delle action va mergiata per prima.

---

## 10. Quando qualcosa non va

| Sintomo | Cosa controllo |
|---|---|
| La TV mostra la UI vecchia | Ho rifatto `bun run build`? Il server serve `client/dist` |
| 404 sul client / pagina vuota | Il server parte dalla radice del repo? Altrimenti `--client-dist <percorso>/client/dist` |
| I telefoni non si collegano | Stessa Wi-Fi, firewall di macOS, `--bind` non su 127.0.0.1, URL dal QR (`GET /api/lan`) |
| Il server non parte | Dati incoerenti (guardo il messaggio, poi `ttr-validate`), porta occupata |
| `429 TOO_MANY_GAMES` | 20 partite attive: chiudo le vecchie con `DELETE` o aspetto i TTL |
| `NAME_TAKEN` inatteso | Nome "uguale a vista" a un altro, o giocatore rimasto registrato dopo un join annullato → `reassign` |
| `401 RESUME_TOKEN_MISMATCH` | Manca il token originale: `reassign` dall'host (anche a partita avviata, "Riassegna" nel segnapunti della TV) |
| Resume: «partita già conclusa» | Il save era `completed`: è stato archiviato all'avvio, guardo `results.jsonl` |
| Resume: «save creato con regole precedenti» | Save di una versione vecchia delle regole, non riproducibile |
| TV senza `hostToken` | Chiusa la scheda prima dello start: "Crea una nuova partita" |
| Profili spariti | Cerco `profiles.json.corrupt-*` in `--saves-dir`; controllo di aver lanciato il server dalla cartella giusta (`--saves-dir` relativo alla cwd) |
| Warn «percorso più lungo non esatto» nel log | Budget di ricerca esaurito (mappa molto densa): il bonus potrebbe essere sbagliato; non succede sulla USA |
| Test del server instabili | Mai `sleep` fissi: uso gli helper `wait_for*` in `crates/ttr-server/tests/common/mod.rs` |
