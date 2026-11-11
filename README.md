# CiufCiuf!

Gioco in stile *Ticket to Ride* per la **LAN di casa**: la TV mostra la mappa condivisa, ogni giocatore (2–5) usa il proprio telefono come controller privato, collegandosi via **QR code**. Zero internet, zero installazione, solo amici.

> [!NOTE]
> Progetto personale, uso privato. Nessun asset, artwork o testo di Days of Wonder/Asmodee: le meccaniche sono replicabili, la grafica è originale (mappa SVG disegnata da coordinate geografiche reali).

## Stato

Giocabile dall'inizio alla fine sulla mappa **USA** (`usa_v2`), da 2 a 5 giocatori, con **profili giocatore** (solo il nome, creati una volta) e una **classifica generale** delle partite concluse. Motore, server e client sono completi per questo primo traguardo. La mappa Europe e il bot non ci sono ancora.

Per orientarsi nel dettaglio:

- **[`RECAP.md`](RECAP.md)**: punto d'ingresso, con stato, decisioni e prossimi fix (scritto anche per agenti AI)
- **[`docs/plan/ciufciuf-v1.3.md`](docs/plan/ciufciuf-v1.3.md)**: piano completo con regole vincolanti, architettura, protocollo e roadmap

## Come si gioca

Servono Rust (edizione 2024) e [bun](https://bun.sh). Dalla radice del repo:

```bash
(cd client && bun install --frozen-lockfile && bun run build)
cargo build --release
./target/release/ttr-server --port 8080 --data-dir ./data --saves-dir ./saves --client-dist ./client/dist
```

Poi:

1. Apri `http://<ip-del-mac>:8080` sul browser della TV: la home mostra la classifica generale. Crea la partita.
2. Ogni giocatore scansiona il QR col telefono, che deve stare sulla stessa rete Wi-Fi. Se macOS lo chiede, consenti le connessioni in entrata a `ttr-server`.
3. Sul telefono, alla voce **Chi sei?**, tocca il tuo nome; la prima volta usa **+ Nuovo profilo**. Il colore si sceglie a ogni partita.
4. Quando sono entrati tutti, avvia la partita dalla TV.

Nella mano sul telefono il numero di carte per colore sta in alto a sinistra di ogni carta; toccando una carta la si porta in primo piano.

Dopo ogni modifica va ricompilato ciò che è cambiato: `bun run build` per il client, `cargo build --release` per il server. Il server serve i file di `client/dist`, quindi un client non ricompilato resta alla versione vecchia.

### Profili, classifica e fine partita

I dati restano in `--saves-dir` (ignorata da git):

| File | Contenuto |
|---|---|
| `<id>.json` | Save di una partita **in corso**: seed, log delle azioni e, per ogni giocatore, solo l'hash SHA-256 del token |
| `profiles.json` | Nomi dei profili, salvati al primo ingresso in partita (maiuscole, forma Unicode e caratteri confondibili ignorati, massimo 200; se due profili hanno la stessa chiave resta il primo). Se il file è illeggibile viene spostato in `profiles.json.corrupt-<unix>` e non è mai sovrascritto |
| `results.jsonl` | Una riga per partita conclusa: nome, totale e vincitore di ogni giocatore. Una partita compare una sola volta |

A fine partita il motore calcola il vincitore (totale più alto; a parità più biglietti completati, poi bonus del percorso più lungo), il risultato viene aggiunto a `results.jsonl` e solo dopo il save della partita viene **eliminato**. Se l'archiviazione fallisce il save resta su disco con stato `completed` e viene archiviato al riavvio successivo: all'avvio il server archivia e elimina i save conclusi rimasti e cancella i `*.tmp` più vecchi di un'ora. La schermata finale resta per 30 minuti, poi la partita sparisce dall'elenco. La classifica generale (vittorie, poi punti totali, aggregata per nome normalizzato) è in `GET /api/standings`; i profili in `GET /api/profiles`. Save, profili e risultati sono scritti in modo durevole (file temporaneo, `fsync`, `rename`, `fsync` della cartella) da un task per partita, fuori dall'attore.

Limiti e durate:

- **Nomi:** giocatori e partite sono normalizzati in NFKC (massimo 24 caratteri) e le sequenze di spazi diventano uno spazio solo. Sono rifiutati con `INVALID_NAME` i caratteri di controllo, invisibili o "vuoti" (zero-width, bidi, Hangul filler, braille blank, variation selector), i segni combinanti iniziali o più di 2 di fila e le parole che mescolano alfabeti (quindi anche "Ωmega", "서울Seoul" e il persiano con ZWNJ). Gli emoji sono ammessi: `U+FE0F` solo dopo un emoji e `U+200D` solo tra due emoji ("Anna ❤️", sequenze di famiglia), altrove sono rifiutati. Due nomi con lo stesso aspetto hanno la stessa chiave (skeleton confusabile UTS #39, minuscolo) e il secondo riceve `NAME_TAKEN`: oltre a "А" cirillica e "A" latina, sono lo stesso nome anche "Tim"/"Tirn", "Leo"/"Le0" e "Player1"/"Playerl", e in classifica finiscono in una riga sola.
- **Seed:** il seed di ogni partita è di 64 bit da un generatore crittografico e finisce solo nel save, mai nelle API o nei messaggi WebSocket. `POST /api/games` con un `seed` risponde `400 SEED_NOT_ALLOWED`, salvo che il server sia avviato con `--allow-client-seed` (test e partite riproducibili: con il seed si ricostruiscono le mani di tutti).
- **Lobby inattiva:** una lobby non avviata senza join, start o connessioni di giocatori per 30 minuti sparisce; senza giocatori bastano 5 minuti, anche con spettatori collegati.
- **Partita abbandonata:** una partita avviata senza connessioni per 6 ore sparisce dall'elenco, ma il suo save resta su disco e si può riprendere con `--resume`.
- **Partite attive:** al massimo 20, e le partite concluse non contano; oltre, la creazione risponde `429 TOO_MANY_GAMES`. Al massimo 8 spettatori WebSocket per partita. L'host chiude una partita con `DELETE /api/games/<id>` e `{"hostToken"}` (204, oppure 401): un save su disco resta dov'è.
- **WebSocket:** messaggi oltre 16 KiB chiudono la connessione; un client che non legge viene scollegato e, riconnettendosi, riceve lo stato aggiornato. Il server manda un ping ogni 15 secondi e scollega chi tace per 45; la scrittura su un socket che non assorbe dati scade dopo 10 secondi. Ogni giocatore ha al massimo 4 connessioni: la quinta chiude la più vecchia.

### Opzioni del server

| Opzione | Default | Note |
|---|---|---|
| `--port` | `8080` | Porta TCP (`u16`) |
| `--bind` | `0.0.0.0` | Indirizzo IP su cui ascoltare |
| `--data-dir` | `data` | Mappe e ruleset |
| `--saves-dir` | `saves` | Save delle partite in corso, profili e risultati |
| `--client-dist` | `../client/dist` | Il default vale solo se si parte da una sottocartella: dalla radice va passato `./client/dist` |
| `--resume <file>` | | Ripristina una partita salvata |
| `--allow-client-seed` | spento | Ammette il seed scelto dal client in `POST /api/games` |

`--help` elenca le opzioni; un valore non valido, un valore mancante o un'opzione sconosciuta fanno terminare il server con un errore. Su SIGINT/SIGTERM il server smette di accettare creazioni, join e WebSocket (503), ferma le partite, attende le scritture in coda e chiude entro 15 secondi; un secondo segnale esce subito (codice 130) e i save in coda potrebbero non essere su disco.

### Riprendere una partita salvata

```bash
./target/release/ttr-server --data-dir ./data --saves-dir ./saves --client-dist ./client/dist --resume saves/<id>.json
```

Il log stampa l'id della partita e un `hostToken` nuovo. Il resume rifiuta i save di una partita già conclusa e quelli creati con regole precedenti (`RULES_VERSION` 1 o assente: "save creato con regole precedenti: non riproducibile").

I giocatori rientrano con il nome **e il token originale**, che il telefono conserva: `POST /api/games/<id>/join` con `{"name","token"}` restituisce lo stesso `playerId` e lo stesso token, mentre il solo nome dà `401 RESUME_TOKEN_MISMATCH`. Finché non ci sono tutti l'avvio viene rifiutato.

Se un telefono ha perso il token, o il save è di formato 1 (senza hash dei token), l'host assegna un token nuovo con `reassign`; il vecchio smette di funzionare. L'`hostToken` della ripresa sta solo nel log: la TV lo chiede (campo "Token host") per i pulsanti "Riassegna", oppure si possono usare `curl`:

```bash
curl -X POST http://localhost:8080/api/games/<id>/reassign -H 'content-type: application/json' -d '{"hostToken":"<token>","name":"Alice"}'
curl -X POST http://localhost:8080/api/games/<id>/start -H 'content-type: application/json' -d '{"hostToken":"<token>"}'
```

A ogni ripresa il save originale viene copiato in `saves/<id>.resume-<timestamp>.bak`. I backup non vengono mai ruotati né sovrascritti, e si possono cancellare a mano quando non servono più.

## Da sapere

- **Rientro in partita:** un telefono disconnesso rientra da solo. Dopo un resume rientra solo chi ha il token originale; da un telefono diverso serve la riassegnazione dell'host.
- **Profili:** un profilo è solo un nome, senza password: chiunque può toccare qualsiasi nome. Le partite concluse prima dell'introduzione della classifica non vi compaiono.
- **Uscita:** "Esci" durante la partita chiede conferma, perché fa perdere il posto.
- **Telefono in standby:** alla riattivazione il client si riconnette e riceve lo stato aggiornato. Non c'è HTTPS, quindi niente Wake Lock.
- **Save vecchi:** `RULES_VERSION` è 2 (rispetto alla 1 cambiano D27, D28 e gli scarti delle carte realmente rimosse). I save creati con regole precedenti, o senza `rulesVersion`, non sono riproducibili e il resume li rifiuta.
- **Partite infinite:** una partita non finisce solo se chi può ancora reclamare rotte smette di farlo.

## Regole

Le regole vincolanti sono nel piano, §3. Il motore le applica così, e tre casi limite sono decisioni di progetto:

- Chi non è di turno non ha mosse disponibili.
- L'ultimo giro parte quando un giocatore resta con 2 treni o meno, e lo giocano tutti, compreso chi l'ha innescato.
- **D26:** con 3 o più locomotive scoperte si scartano le 5 carte e se ne scoprono altre. Se tra mazzo e scarti restano al massimo 2 carte non-locomotiva, si rimescola una volta sola e ci si ferma, anche con 3 locomotive scoperte.
- **D27:** se finiscono sia il mazzo sia gli scarti, entra in gioco un mazzo completo nuovo. Le carte in mano restano ai giocatori.
- **D28:** se nessuno può più reclamare alcuna rotta, parte l'ultimo giro.
- **Pagamento:** il client indica le carte; se ne manca una del colore dichiarato il motore usa una locomotiva (sempre un pagamento legale).

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
bun run test && bun run check && bun run build
bun smoke-test.mjs http://localhost:8080        # serve un server già avviato con --allow-client-seed; lascia un save di prova
bun tools/fullgame-test.mjs --spawn-server     # partita completa con un server suo, dopo la build release
```

### Convenzioni

- **Determinismo:** una partita è seed più log delle azioni. Il replay deve dare lo stesso stato e gli stessi eventi: niente iterazione su `HashMap` nei percorsi che toccano stato o serializzazione.
- **`RULES_VERSION`** (`ttr-core`) va incrementata quando una modifica cambia il replay di una partita esistente.
- **Il client non conosce le regole:** le mosse legali arrivano dal server.
- **Protocollo:** i tipi nascono in `ttr-proto`; `bun run copy-bindings` li copia nel client.
- **Package manager del client:** bun (`client/bun.lock`). `bun install` blocca il postinstall di `esbuild` (`bun pm untrusted` lo elenca): è innocuo, perché esbuild usa il binario del suo pacchetto per piattaforma, quindi `trustedDependencies` resta vuoto.
- **Registry del client:** è fissato in `client/bunfig.toml` (`registry.npmjs.org`) e `bun.lock` contiene solo URL di quel registry (la CI lo verifica): non cambiarli anche se in locale si usa un mirror aziendale.
- **Sviluppo del client:** `bun run dev` usa il proxy di Vite per `/api` e `/ws` verso `localhost:8080`; il parametro `?server=` non esiste più.
- **`hostToken` della TV:** vive in `sessionStorage`, quindi chiudere la scheda della TV prima dell'avvio fa perdere il controllo della partita. La TV offre "Crea una nuova partita" e la lobby vecchia scade da sola. Se un join lento annullato riesce in ritardo, il telefono salva comunque l'identità senza cambiare schermata, e riprovando con lo stesso nome riprende quel giocatore. Solo se la risposta non arriva mai il nuovo tentativo riceve `NAME_TAKEN` e l'host deve riassegnarlo.
- **Aggiornamento delle dipendenze:** Renovate (`renovate.json`) apre PR raggruppate. `rand`, `rand_chacha` e `rand_core` sono esclusi dal bot, perché determinano lo shuffle e quindi il replay di ogni save: si aggiornano solo a mano, insieme a `RULES_VERSION`.
- **Major del client:** `vite`, `vitest`, `svelte` e `typescript` richiedono modifiche alla configurazione, quindi le major si approvano dalla Dependency Dashboard di Renovate.
- **Dati di mappa e ruleset:** `ttr-validate` e l'avvio del server rifiutano dati incoerenti (id duplicati, città inesistenti, rotte doppie non appaiate, biglietti con partenza uguale all'arrivo, ecc.) e le mappe troppo dense per il calcolo esatto del percorso più lungo (numero ciclomatico stimato oltre 14; la USA stima 13). Il percorso più lungo è una ricerca esatta (branch and bound) con un budget di sicurezza: se lo esaurisce il server logga un `warn!` e il risultato di quel giocatore può non essere esatto. Le rotte con partenza uguale all'arrivo sono rifiutate.
- **Errori:** `ErrorPayload.detail` porta il messaggio italiano del motore; il client lo mostra (fallback: `lib/errors.ts`).
- **Layout della mappa:** `cd client && bun tools/gen-schematic-layout.mjs` (circa 3 minuti, deterministico) rigenera `usa-layout.json`. Va rilanciato se cambiano geometria delle rotte o regole delle etichette.

## Prossimi fix

In ordine di priorità, con il dettaglio in [`RECAP.md`](RECAP.md):

1. Ripresa di una partita avviabile dalla TV (elenco dei save in corso + pulsante "Riprendi", `hostToken` della ripresa visibile alla TV)
2. TV: riquadro con le ultime mosse
3. Rotazione dei backup `.bak` e layout schematico della mappa per ruleset (oggi `usa-layout.json` e l'elenco dei ruleset sono statici nel client; la mappa e i biglietti arrivano da `GET /api/rulesets/{id}/map`)

Da confermare: il criterio di accettazione con 2–5 telefoni veri via QR.
