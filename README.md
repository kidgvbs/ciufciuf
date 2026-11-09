# CiufCiuf!

Gioco in stile *Ticket to Ride* per la **LAN di casa**: la TV mostra la mappa condivisa, ogni giocatore (2–5) usa il proprio telefono come controller privato, collegandosi via **QR code**. Zero internet, zero installazione, solo amici.

> [!NOTE]
> Progetto personale, uso privato. Nessun asset, artwork o testo di Days of Wonder/Asmodee: le meccaniche sono replicabili, la grafica è originale (mappa SVG disegnata da coordinate geografiche reali).

## Stato

Giocabile dall'inizio alla fine sulla mappa **USA** (`usa_v2`), da 2 a 5 giocatori. Motore, server e client sono completi per questo primo traguardo. La mappa Europe, il bot e le statistiche per profilo non ci sono ancora.

Per orientarsi nel dettaglio:

- **[`RECAP.md`](RECAP.md)**: punto d'ingresso, con stato, decisioni e prossimi fix (scritto anche per agenti AI)
- **[`docs/plan/ciufciuf-v1.3.md`](docs/plan/ciufciuf-v1.3.md)**: piano completo con regole vincolanti, architettura, protocollo e roadmap

## Come si gioca

Servono Rust (edizione 2024) e Node.js. Dalla radice del repo:

```bash
(cd client && npm install && npm run build)
cargo build --release
./target/release/ttr-server --port 8080 --data-dir ./data --saves-dir ./saves --client-dist ./client/dist
```

Poi:

1. Apri `http://<ip-del-mac>:8080` sul browser della TV e crea la partita.
2. Ogni giocatore scansiona il QR col telefono, che deve stare sulla stessa rete Wi-Fi. Se macOS lo chiede, consenti le connessioni in entrata a `ttr-server`.
3. Quando sono entrati tutti, avvia la partita dalla TV.

Dopo ogni modifica va ricompilato ciò che è cambiato: `npm run build` per il client, `cargo build --release` per il server. Il server serve i file di `client/dist`, quindi un client non ricompilato resta alla versione vecchia.

### Opzioni del server

| Opzione | Default | Note |
|---|---|---|
| `--port` | `8080` | |
| `--data-dir` | `data` | Mappe e ruleset |
| `--saves-dir` | `saves` | Una partita è un file JSON |
| `--client-dist` | `../client/dist` | Il default vale solo se si parte da una sottocartella: dalla radice va passato `./client/dist` |
| `--resume <file>` | | Ripristina una partita salvata |

Il server non ha `--help`: le opzioni sconosciute vengono ignorate e il server parte lo stesso.

### Riprendere una partita salvata

```bash
./target/release/ttr-server --data-dir ./data --saves-dir ./saves --client-dist ./client/dist --resume saves/<id>.json
```

Il log stampa l'id della partita e un `hostToken` nuovo. I giocatori rientrano con gli stessi nomi, e finché non ci sono tutti l'avvio viene rifiutato. La TV non conosce il nuovo token, quindi per ora l'avvio si fa a mano:

```bash
curl -X POST http://localhost:8080/api/games/<id>/start -H 'content-type: application/json' -d '{"hostToken":"<token>"}'
```

A ogni ripresa il save originale viene copiato in `saves/<id>.resume-<timestamp>.bak`. I backup non vengono mai ruotati né sovrascritti.

## Da sapere

- **Rientro in partita:** un telefono disconnesso rientra da solo, ma da un telefono diverso il rientro non funziona ancora.
- **Telefono in standby:** alla riattivazione il client si riconnette e riceve lo stato aggiornato. Non c'è HTTPS, quindi niente Wake Lock.
- **Save vecchi:** un save che aveva già esaurito mazzo e scarti può divergere al replay senza dare errore. Tutti gli altri restano identici.
- **Partite infinite:** una partita non finisce solo se chi può ancora reclamare rotte smette di farlo.
- **Test:** `full_game_over_ws` e `save_and_replay` ogni tanto si appendono, perché il driver fa `recv` senza timeout. Se un test resta fermo, va interrotto e rilanciato.

## Regole

Le regole vincolanti sono nel piano, §3. Il motore le applica così, e tre casi limite sono decisioni di progetto:

- Chi non è di turno non ha mosse disponibili.
- L'ultimo giro parte quando un giocatore resta con 2 treni o meno, e lo giocano tutti, compreso chi l'ha innescato.
- **D26:** con 3 o più locomotive scoperte si scartano le 5 carte e se ne scoprono altre. Se tra mazzo e scarti restano al massimo 2 carte non-locomotiva, si rimescola una volta sola e ci si ferma, anche con 3 locomotive scoperte.
- **D27:** se finiscono sia il mazzo sia gli scarti, entra in gioco un mazzo completo nuovo. Le carte in mano restano ai giocatori.
- **D28:** se nessuno può più reclamare alcuna rotta, parte l'ultimo giro.

## Sviluppo

### Struttura

```
crates/ttr-core/      motore puro e deterministico, indipendente dalla mappa (più ttr-cli per il debug)
crates/ttr-server/    Axum/Tokio: lobby REST, WebSocket, save e resume, file statici
crates/ttr-proto/     tipi del protocollo ed export TypeScript con ts-rs
crates/ttr-validate/  validazione dei dati di mappa e ruleset
data/                 mappe (maps/) e ruleset (rulesets/) in JSON
client/               Svelte 5: vista TV (/) e controller del telefono (/controller)
docs/                 piano, convenzioni dati, specifiche di fase
```

### Verifiche

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
FUZZ_GAMES_FULL=6000 cargo test -p ttr-core --release --test integration_tests fuzzing_1000_games_explicit
./target/release/ttr-validate data/maps/usa.json data/rulesets/usa_v2.json

cd client
npm run typecheck && npm test && npm run check && npm run build
node smoke-test.mjs http://localhost:8080       # serve un server già avviato; lascia un save di prova
node tools/fullgame-test.mjs --spawn-server     # partita completa con un server suo, dopo la build release
```

### Convenzioni

- **Determinismo:** una partita è seed più log delle azioni. Il replay deve dare lo stesso stato e gli stessi eventi: niente iterazione su `HashMap` nei percorsi che toccano stato o serializzazione.
- **`RULES_VERSION`** (`ttr-core`) va incrementata quando una modifica cambia il replay di una partita esistente.
- **Il client non conosce le regole:** le mosse legali arrivano dal server.
- **Protocollo:** i tipi nascono in `ttr-proto`; `npm run copy-bindings` li copia nel client.
- **Layout della mappa:** `cd client && node tools/gen-schematic-layout.mjs` (circa 3 minuti, deterministico) rigenera `usa-layout.json`. Va rilanciato se cambiano geometria delle rotte o regole delle etichette.

## Prossimi fix

In ordine di priorità, con il dettaglio in [`RECAP.md`](RECAP.md):

1. Rientro da un telefono diverso
2. Ripresa di una partita avviabile dalla TV
3. Identità al resume, oggi basata solo sul nome
4. Timeout nei test che si appendono
5. Save e lobby: fsync, metadati del save, rotazione dei backup
6. Limiti contro gli abusi e prestazioni del longest path
7. Rifiniture grafiche

Da confermare: il criterio di accettazione con 2–5 telefoni veri via QR. Poi la mappa Europe.
