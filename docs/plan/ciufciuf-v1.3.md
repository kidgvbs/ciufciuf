# CiufCiuf! — Piano completo — Gioco stile Ticket to Ride su LAN

**Versione:** 1.3 — 3 novembre 2026 *(supera la v1.2: integra D22 Svelte + D23 repo GitHub privato/RECAP)*
**Stato (aggiornato 18/11/2026):** Fasi 0–4 completate; 15 ruleset su 7 mappe (USA, Europe e loro modalità; Germany, Poland, Basilicata, Nordic Countries, Switzerland — §3.4, §3.5); prossima la Fase 5. §14 chiuso. Le decisioni successive a D23 (D24–D45) sono registrate in `RECAP.md` §4; la cronaca dei giri in `docs/history.md`. Casa del progetto: `~/Rust/ciufciuf` (repo GitHub privato `kidgvbs/ciufciuf`, D23)
**Fonti:** ricerca web (Wikipedia, BGA, Marmalade/Steam, Days of Wonder), studio preliminare, GDD esterno revisionato dall'utente, decisioni prese in sessione, revisione esterna del piano v1.1 (verificata), risposte definitive dell'utente del 3/11/2026 (12 punti — integrate come D8/D19/D20/D21 e in §14) e conferme successive dello stesso giorno (**D22**: Svelte; **D23**: repo GitHub privato + `RECAP.md`).

> Questo documento **supera** lo studio preliminare. Regola dell'ultimo giro: chi innesca la fine **gioca** l'ultimo turno
> (regolamento ufficiale: *"each player, including that player, gets one final turn"*; confermato su Board Game Arena).

---

## Indice

1. [Visione e vincoli](#1-visione-e-vincoli)
2. [Registro delle decisioni](#2-registro-delle-decisioni)
3. [Regole di gioco — specifica vincolante](#3-regole-di-gioco--specifica-vincolante)
4. [Dati di mappa e ruleset descriptor](#4-dati-di-mappa-e-ruleset-descriptor)
5. [Architettura](#5-architettura)
6. [Macchina a stati](#6-macchina-a-stati)
7. [Contratto WebSocket](#7-contratto-websocket)
8. [Specifiche UI/UX](#8-specifiche-uiux)
9. [Algoritmi](#9-algoritmi)
10. [Roadmap a fasi](#10-roadmap-a-fasi)
11. [Registro rischi](#11-registro-rischi)
12. [Note legali](#12-note-legali)
13. [Riferimenti](#13-riferimenti)
14. [Punti aperti](#14-punti-aperti)

---

## 1. Visione e vincoli

**Visione in una frase:** un'app Ticket to Ride completa che gira su un host nella Wi-Fi di casa: la TV mostra la mappa condivisa, gli amici si collegano dai cellulari via QR code, e ogni telefono mostra la mano privata del proprio giocatore.

**Il modello UX** (ispirato ad Amazon Luna, "reinventato" per LAN): Luna usa i cellulari come *dumb controller* per uno streaming cloud da AWS — non replicabile né desiderabile (richiede internet, non è self-hostable). La nostra versione è **meglio del modello Luna** per TTR: i controller sono *smart* (browser), ogni telefono è la mano privata del giocatore — risolvendo il vero problema del gioco locale (privacy di carte e biglietti).

**Vincoli d'uso:**
- Gioco **esclusivamente locale**: LAN di casa, zero dipendenze da internet.
- Solo amici: 2–5 giocatori, nessun matchmaking, nessun account, nessun negozio.
- Progetto **personale**: nessuna pubblicazione prevista (v. §12).
- Fedeltà alle **regole per-mappa della versione 2023** dell'app ufficiale Marmalade (= regolamenti fisici ufficiali).

**Cosa NON è:** niente online pubblico, niente classifiche online, niente cross-play, niente IAP/cosmetici, niente AI "adattiva" neural-network (un bot euristico decente basta).

---

## 2. Registro delle decisioni

| # | Decisione | Scelta | Motivo |
|---|---|---|---|
| D1 | Scope | App completa per uso locale | Da richiesta utente |
| D2 | Modello multiplayer | Host LAN + controller smart da browser (telefono) | Pattern Jackbox/companion-app; privacy mano gratis; zero install |
| D3 | Stack backend | **Rust** (core + server Axum/Tokio) | Determinismo, enum esaustivi per la state machine, binario statico per host qualsiasi, ambiente utente già Rust |
| D4 | Stack client | **TypeScript** (web) | Obbligato dai telefoni come controller: browser = zero install, iOS+Android |
| D5 | Spring Boot | **Scartato** | Cerimonia inutile a questa scala, JVM sul deployment, niente condivisione tipi col client. (Gli algoritmi del GDD Java restano validi: sono stack-agnostici) |
| D6 | STOMP | Sostituito da WebSocket raw + JSON | Le *shape* dei payload del GDD restano; STOMP era un idioma Spring, non un requisito |
| D7 | Mappe primo traguardo | **USA + Europe** | USA copre il core; Europe valida tunnel/traghetti/stazioni e l'architettura a ruleset |
| D8 | Hardware host | **Mac esistente dell'utente**, con **containerizzazione** (Docker) come target di deployment; Raspberry Pi resta possibile in futuro (binario statico, ARM64 via `cross`) | Decisione utente 3/11: zero hardware nuovo; il container vale anche come ambiente riproducibile per il server |
| D9 | Regole riferimento | App 2023 (Marmalade) = regolamenti fisici ufficiali | Fonte di verità: i PDF Days of Wonder; BGA come riferimento implementativo |
| D10 | Fallback Pass & Play | Fuori dal primo traguardo, fase "aperta" (candidato dopo la Fase 3) | Chiarito con l'utente: un unico telefono che passa di mano in giro, mentre il modello principale resta "TV + un telefono per giocatore". Priorità bassa perché i giocatori previsti (2–5 amici in casa) hanno tutti un telefono |
| D11 | Ultimo giro | **Tutti giocano, incluso chi innesca** (flag `finalRoundIncludesTrigger`) | Regolamento ufficiale + BGA |
| D12 | Stazioni (Europe) | **Quarta azione principale ed esclusiva del turno** | Regolamento Europe: il turno è *una* tra pesca carte, reclamo rotta, pesca biglietti, costruzione stazione |
| D13 | Tunnel non pagato (Europe) | Carte giocate tornano in mano, 3 carte rivelate negli scarti, **turno terminato** | Regolamento Europe |
| D14 | Biglietti Europe | **Due mazzi**: 6 lunghi + 40 regolari; il descriptor supporta mazzi di biglietti nominati | Il setup Europe pesca da entrambi, la pesca in partita solo dai regolari |
| D15 | Connettività con stazioni | Union-Find sul grafo proprio + ricerca esaustiva sulle assegnazioni stazione→rotta avversaria | Una stazione presta *una sola* rotta, la stessa per tutti i biglietti (§9) |
| D16 | URL nel QR | **IP numerico LAN** (`http://192.168.x.y:porta`); `ttr.local` solo come testo di fallback | Risoluzione `.local` inaffidabile su molti telefoni/router |
| D17 | Telefoni in standby | Meccanismo primario: reconnect + `SNAPSHOT` al ritorno visibile; Wake Lock come miglioria best-effort | La Screen Wake Lock API richiede un *secure context* (HTTPS), assente su `http://` LAN (§5.4) |
| D18 | Persistenza partite | Un file JSON per partita in `saves/` (`seed` + log azioni); nessun DB | Decisione utente 3/11; event sourcing sul motore deterministico; i save sono anche test di replay |
| D19 | Nome del progetto | **CiufCiuf!** | Decisione utente 3/11: nome originale non-"Ticket to Ride" da subito (v. §12) |
| D20 | Statistiche profili | **Sì**: per ogni profilo creato, statistiche cumulative oltre la singola partita — ispirate alla versione web di Amazon Luna ("totale stelle cumulate" per profilo) — persistite in `profiles.json` (niente DB) | Decisione utente 3/11: funzione teneria; i profili nascono già in Fase 2/3 (lobby: nome + colore), le statistiche cumulative entrano in Fase 5 |
| D21 | Audio | **Posticipato oltre la Fase 5 di fatto**: prima far funzionare il gioco, poi i suoni | Decisione utente 3/11: priorità funzionalità; la Fase 5 lo prevede come voce opzionale, candidabile a "fase aperta" |
| D22 | Framework client | **Svelte 5** (su TypeScript, D4) | Decisione utente 3/11: "usiamo svelte". Bundle piccolo per i telefoni, reattività semplice per la mano privata, SVG inline naturale per la mappa, poco boilerplate. Il contratto WS (§7) e i tipi ts-rs sono framework-agnostic: l'unica superficie Svelte sono le due viste (§5.4) → eventualmente sostituibile senza toccare il backend |
| D23 | Casa del progetto | **`~/Rust/ciufciuf`** + **repo GitHub privato `kidgvbs/ciufciuf`** + **`RECAP.md` context-pack alla radice** | Decisione utente 3/11: documentazione centralizzata nel repo; qualunque agente ricostruisce il contesto leggendo `RECAP.md`; il piano vive in `docs/plan/` |

---

## 3. Regole di gioco — specifica vincolante

Fonte: PDF ufficiali Days of Wonder (EN e IT disponibili, v. §13). BGA come riferimento implementativo.

### 3.1 USA (mappa base)

**Componenti (edizione V2 2025, la più recente — scelta dell'utente):** 110 carte treno (8 colori × 12 + 14 locomotive/jolly), 45 treni per giocatore (5 colori), **33 biglietti destinazione** (i 30 classici + 3 nuovi, tutti presenti anche in USA 1910: **Atlanta–Washington 4**, **Omaha–New Orleans 8**, **San Francisco–Washington 21**), 5 carte scoperte.

**Setup:** ogni giocatore riceve 4 carte treno e **4 biglietti destinazione**; ne tiene **almeno 2** (regola V2 2025: prima erano 3 distribuiti); gli scarti vanno **in fondo al mazzo, rimescolati tra loro** (PDF V2: *"shuffled together and put at the bottom of the ticket deck"*). NOTA: nel v1.1 qui c'era "in ordine (non rimescolati)" riferito all'edizione classica — il PDF V2 2025 dice esplicitamente *shuffled* per gli scarti di setup; gli scarti **in partita** restano *in fondo in qualunque ordine* (PDF: *"in any order you want"*).

**Turno — esattamente UNA di queste 3 azioni:**

1. **Pescare 2 carte treno** (mazzo coperto o scoperte, qualsiasi combinazione):
   - Una **locomotiva scoperta conta come pescata intera**: il turno di pesca termina. Non si può prendere una locomotiva scoperta come *seconda* carta.
   - Una locomotiva pescata dal mazzo coperto è una pescata normale.
   - Se a un qualsiasi momento **≥3 locomotive** sono scoperte: scartare tutte le 5, rivelarne 5 nuove.
   - Mazzo esaurito → rimescolare gli scarti. Se anche gli scarti sono esauriti (**D27**): si crea un mazzo completo nuovo con la composizione del ruleset, lo si mescola e si continua a pescare; le carte in mano restano dove sono, quindi il totale in gioco può superare 110.
2. **Reclamare una rotta**: pagare carte pari alla lunghezza, del colore della rotta (rotte grigie: qualsiasi colore, ma tutte uguali); le locomotive sostituiscono qualsiasi colore. Punti immediati (tabella sotto). Piazzare i treni.
3. **Pescare 3 biglietti destinazione**, tenerne **almeno 1** (fino a 3). Scarti in fondo al mazzo in ordine. I biglietti tenuti non si scartano più.

**Rotte doppie** (parallele tra le stesse due città):
- Stesso giocatore: **mai entrambe**.
- 2–3 giocatori: reclamarne una **blocca** l'altra per tutti.
- 4–5 giocatori: due giocatori diversi possono reclamarle entrambe.

**Fine partita:** quando a fine turno un giocatore ha **≤2 treni**, oppure quando **nessun giocatore può più reclamare alcuna rotta**, a prescindere dalle carte (**D28**: rotte già prese, sorelle bloccate o più lunghe dei treni rimasti), si annuncia l'ultimo giro: **ogni giocatore, incluso chi ha innescato, fa un ultimo turno** (D11). Il server memorizza `lastRoundTriggerPlayer = Pi`; si prosegue in ordine di turno e, dopo il turno finale di `Pi`, si passa a `GAME_OVER`. Poi rivelazione biglietti e punteggio finale.

**Punteggio:**

| Lunghezza | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|
| Punti | 1 | 2 | 4 | 7 | 10 | 15 |

- Rotte: punti immediati al claim.
- Biglietti: +valore se completati (le due città connesse nel proprio sottografo), −valore se falliti.
- **Longest Continuous Path**: +10 al percorso continuo più lungo (parità: +10 a tutti i pari). Cammino *semplice* sul multigrafo: nessuna rotta riutilizzata, città riattraversabili.

### 3.2 Europe (differenze)

- 110 carte treno (come USA), 45 treni, **3 stazioni per giocatore**.
- **Biglietti: 46 in due mazzi** (D14):
  - **6 lunghi** (sfondo blu) e **40 regolari**.
  - **Setup**: ogni giocatore riceve **1 lungo + 3 regolari** e ne tiene **almeno 2** (il lungo si può scartare). I lunghi non distribuiti e quelli scartati al setup tornano nella scatola, coperti: escono dalla partita. Anche i **regolari scartati al setup escono dalla partita** (non tornano nel mazzo): modellato dal descriptor con `setupDiscards: "removed"` (v. §4.1). Confermato dall'utente: in Europe "base" i biglietti totali sono 46.
  - **In partita**: "Pesca biglietti" pesca **3 regolari**, tenerne **almeno 1**; scarti in fondo al mazzo regolare.
- **Turno — esattamente UNA di 4 azioni**: le tre di §3.1 più **costruire una stazione** (D12).
- **Tunnel** (rotte marcate):
  1. Il giocatore gioca le carte richieste dalla lunghezza.
  2. Si rivelano le prime 3 carte del mazzo (meno se mazzo + scarti non bastano; nessuna carta → nessun costo extra).
  3. Ogni carta rivelata del **colore delle carte giocate** (per i tunnel grigi: il colore scelto) o locomotiva aumenta il costo di 1. Se il giocatore ha pagato **solo con locomotive**, contano solo le locomotive rivelate.
  4. Il giocatore paga le carte extra (colore o locomotive) e reclama la rotta, **oppure** rinuncia: le carte giocate tornano in mano e **il turno termina** (D13).
  5. Le 3 carte rivelate vanno sempre negli scarti.
- **Traghetti** (segmenti con icona locomotiva): quel numero di locomotive è **parte obbligatoria** del pagamento.
- **Stazioni**:
  - Costruzione = azione del turno. Costo: **1 carta** per la prima, **2 carte dello stesso colore** per la seconda, **3 carte dello stesso colore** per la terza (locomotive ammesse come jolly).
  - Si costruisce su una città **senza altre stazioni** (max una stazione per città), anche se nessuna rotta vi arriva ancora.
  - A fine partita ogni stazione consente di usare **una sola** rotta avversaria entrante/uscente da quella città, la **stessa per tutti i biglietti** del giocatore. Le rotte prese in prestito **non** contano per il percorso più lungo.
  - Ogni stazione **non costruita** vale **+4** a fine partita (max +12).
- **Punteggio rotte (Europe)**: tabella **per-mappa** (nel descriptor, v. §4.1 `routePoints`) — lunghezze 1/2/3/4/6/8 → **1/2/4/7/15/21** (in Europe **non esistono rotte da 5**; la tabella USA è diversa).
- **Bonus: European Express** +10 al percorso continuo più lungo (stesse regole del Longest Path USA, parità inclusa). In Europe **non** esiste il Globetrotter.
- **Aggiornamento Fase 4 (11/11/2026)**: riferimento = regolamento Europe **2026** (DO7202N). Spareggio: totale → biglietti completati → European Express, senza "meno stazioni usate" del 2015 (**D29**). Tunnel: la rivelazione non crea mai un mazzo nuovo (**D30**); costo extra 0 → rotta presa subito, costo > 0 → sempre `TUNNEL_RESOLUTION` (**D31**). Assegnazione stazioni: massimo di biglietti completati − falliti, parità all'ordine canonico (**D32**). D28 resta invariata con le stazioni (**D33**). Dettagli: `docs/packages/fase4-europe.md`.

### 3.3 Checklist edge case del motore

- [ ] Locomotiva scoperta: pescata unica; mai come seconda carta
- [ ] Reset pool a **≥3** locomotive scoperte (vale anche per 4-5)
- [ ] Rimescolamento scarti; esaurimento totale → mazzo nuovo dal ruleset (D27), nessun turno perso
- [ ] Biglietti scartati → fondo del mazzo **in ordine**
- [ ] Rotte doppie: blocco a 2–3 giocatori; mai entrambe allo stesso giocatore
- [ ] Ultimo giro: **tutti**, incluso chi innesca (flag ruleset); fine dopo il turno di chi ha innescato
- [ ] Longest path = cammino semplice sul multigrafo (no riuso rotte, città riattraversabili)
- [ ] RNG con seed: partite riproducibili e replay verificabili
- [ ] Tunnel (Europe): rivelazione di 3 carte (o meno), colore delle carte giocate, caso solo-locomotive, rinuncia → carte in mano + fine turno, rivelate sempre negli scarti
- [ ] Traghetti (Europe): locomotive obbligatorie nel pagamento
- [ ] Stazioni (Europe): azione del turno, costo 1/2/3 stesso colore, una per città, un prestito per stazione valido per tutti i biglietti, nessun contributo all'European Express, +4 per stazione non costruita
- [ ] Biglietti Europe: setup 1 lungo + 3 regolari / min 2; scarti di setup (lunghi e regolari) fuori dalla partita; pesca in partita solo dai regolari
- [ ] Punteggio rotte dalla tabella **per-mappa del descriptor** (Europe: niente rotte da 5; USA sì)
- [ ] **Ordinamento deterministico in Rust**: niente iterazione di `HashMap` in percorsi che influenzano stato/serializzazione → `BTreeMap` o ordinamenti espliciti (il seed di `RandomState` cambia a ogni processo)

### 3.4 Varianti = parametri, non codice (regole per-mappa 2023)

Ogni variante è un ruleset:

| Ruleset | Biglietti iniziali | Pesca biglietti | Bonus |
|---|---|---|---|
| **USA base (`usa_v2`)** | **4 pescati / min 2** (V2 2025) | 3 pescati / min 1 | Longest Path +10 |
| **USA 1910 (`usa_1910`)** | idem (D37) | idem | **Globetrotter +15** (sostituisce L.P.) |
| **USA Mega (`usa_mega`)** | **5 pescati / min 3** | **4 pescati / min 1** | Entrambi (10 + 15) |
| **USA Big Cities (`usa_big_cities`)** | 4 pescati / min 2 | 4 pescati / min 1 | Nessuno |
| **Europe** | **1 lungo + 3 regolari / min 2** | 3 regolari / min 1 | **European Express +10**; stazioni non costruite +4 cad.; tunnel/traghetti |
| **Europe Expanded / 1912 / Big Cities / Mega** | varianti dei pacchetti biglietti 2024 (D34; dettagli nei file `data/rulesets/europe_*.json`) | | come Europe; Mega con `keepMax` 1 sui lunghi |
| **Germany (`germany`, `germany_2017`)** | **2 lunghi + 3 corti / min 2, di cui ≥1 corto** (regola della casa, §3.5) | 4 dalla pila unita / min 1 | Globetrotter +15; 2017 con passeggeri |
| **Poland (`poland`)** | 4 pescati / min 2 | 3 pescati / min 1 | Nessun bonus; carte Paese (D38) |

I valori esatti di ogni ruleset sono nei file `data/rulesets/*.json` (fonte di verità); questa tabella è un riassunto.

Nota: tipo e valore dei bonus variano per ruleset (Longest Path 10, European Express 10, Globetrotter 15, Mega con due bonus) → i bonus *devono* essere parametrici. Anche la composizione dei mazzi di biglietti (singolo o lungo/regolare) è un parametro. Le soglie delle rotte doppie (`doubleRouteMinPlayers: 4`) e l'ultimo giro (`finalRoundIncludesTrigger: true`) sono parametri del ruleset.

Altre mappe dell'app 2023 (India, Legendary Asia, San Francisco, Japan, Africa) arrivano **dopo** USA+Europe: ogni mappa = dati + modulo regole, zero fork del motore. **Germany, Poland e Basilicata** (mappa di prova) sono entrate il 17/11/2026 come primo caso, **Nordic Countries e Switzerland** il 18/11/2026 (§3.5).

### 3.5 Altre mappe e regole della casa (dal 17/11/2026)

- **Binari paralleli 2–4**: un gruppo parallelo ha 2–4 binari (`double: true`, id `_1`…`_n`). Un giocatore non ne prende mai due; con meno di `doubleRouteMinPlayers` giocatori, preso un binario gli altri sono chiusi a tutti (esposti in `PublicState.blocked_route_ids` e nascosti sulla TV). `alwaysOpen: true` (Poland, rotte verso i Paesi) tiene aperti tutti i binari.
- **Paesi confinanti** (Germany, Poland): vicoli ciechi; due rotte verso lo stesso Paese non sono collegate tra loro. Poland: carte Paese (D38).
- **Passeggeri** (`germany_2017`) e **spareggi configurabili** (`tieBreakers` nel ruleset).
- **Germany, regola della casa (D40, diversa dal regolamento ufficiale)**: mano iniziale distribuita **2 lunghi + 3 corti**, si tengono almeno 2 biglietti di cui almeno 1 corto (`keepMin` per voce di `initialTickets.dealt`). Finito il draft, i mazzi di `ticketDraw.mergeDecks` confluiscono in `ticketDraw.deck` in **una sola pila** mescolata con l'RNG del motore (deterministica); `DRAW_TICKETS` pesca 4, tieni ≥1, gli scartati (anche lunghi) vanno in fondo alla pila. `PublicState.ticket_deck_size` conta i biglietti pescabili.
- **Regole speciali come dato (D42)**: i ruleset che differiscono dal gioco base hanno `specialRules` (testi), mostrati dal client a inizio partita senza che il client conosca le regole.
- **Edge case aggiuntivi**: unione dei mazzi una sola volta e riprodotta identica al replay/resume; pesca limitata ai biglietti rimasti; `reshuffleBottom` incompatibile con `mergeDecks` (rifiutato dalla validazione).
- **Nordic Countries (D43, D45)**: 2–3 giocatori, 40 treni; locomotive solo su tunnel e traghetti (`locomotives.usableOn`), pescabili come carte normali (`freeDraw`), niente reset con 3 scoperte (`faceUpReset: false`); sui traghetti 3 carte qualsiasi valgono una locomotiva (`ferries.anyCardsPerLocomotive`); Murmansk–Lieksa (9) con 4 carte qualsiasi per spazio (`anyCardsPerSpace`); biglietti scartati rimossi anche a metà partita (`ticketDraw.discards: "removed"`); Globetrotter +10, nessun percorso più lungo, spareggio `completedTickets` poi `longestPath`. Gruppi paralleli con binari di lunghezza diversa ammessi.
- **Switzerland (D43, D44)**: 2–3 giocatori, 40 treni; locomotive solo sui tunnel, pescabili come carte normali (il reset con 3 scoperte resta); 4 Paesi confinanti; 12 biglietti con più destinazioni (`destinations`): vale la destinazione collegata di valore più alto, nessuna collegata = −valore più basso; scarti rimossi al setup e a metà partita; percorso più lungo +10.
- **Pagamento delle rotte**: valido solo se tutte le carte dichiarate si assegnano esattamente agli spazi (`ttr-core/src/payment.rs`); senza sostituzioni né restrizioni resta il controllo storico (carte = lunghezza).

Convenzioni dei dati: `docs/data-conventions.md`; fonti per mappa: `docs/data-sources-*.md`.

---

## 4. Dati di mappa e ruleset descriptor

### 4.1 Schema dati (concettuale)

```jsonc
// data/maps/europe.json
{
  "cities":  [{ "id": "london", "name": "Londra", "lat": 51.51, "lon": -0.13 }],
  "routes":  [{ "id": "dieppe_london_1", "from": "dieppe", "to": "london",
                "len": 2, "color": "gray", "double": true,
                "tunnel": false, "ferries": 1 }],          // ferries = loco obbligatorie
  "tickets": [{ "id": "edi_ath", "from": "edinburgh", "to": "athina",
                "points": 21, "deck": "long" }]            // deck: "long" | "regular"
}

// data/rulesets/europe.json
{
  "map": "europe", "players": { "min": 2, "max": 5 }, "trainsPerPlayer": 45,
  "deck": { "perColor": 12, "locomotives": 14 },
  "ticketDecks": ["long", "regular"],
  "initialTickets": { "dealt": [{ "deck": "long", "count": 1 },
                                { "deck": "regular", "count": 3 }],
                      "keepMin": 2, "undealtLong": "removed" },
  "ticketDraw":      { "deck": "regular", "count": 3, "keepMin": 1 },
  "setupDiscards": "removed",               // scarti di setup (lunghi E regolari) escono dalla partita
  "routePoints": { "1": 1, "2": 2, "3": 4, "4": 7, "6": 15, "8": 21 },
  "bonuses": [{ "type": "longestPath", "label": "European Express", "points": 10 }],
  "stations": { "count": 3, "costs": [1, 2, 3], "unbuiltPoints": 4, "onePerCity": true },
  "tunnels":  { "revealed": 3, "declineEndsTurn": true },
  "doubleRouteMinPlayers": 4,
  "finalRoundIncludesTrigger": true
}
```

Per USA i biglietti hanno un solo mazzo (`"deck": "regular"`), `initialTickets.dealt` contiene una sola voce, `setupDiscards` vale `"reshuffleBottom"` (gli scarti di setup USA vanno in fondo al mazzo in ordine, come da regolamento) e `routePoints` è `{1:1, 2:2, 3:4, 4:7, 5:10, 6:15}`. **Il validatore (§4.2) deve verificare che ogni lunghezza di rotta presente nella mappa abbia una voce in `routePoints`** — copre anche il caso Europe (nessuna rotta da 5).

### 4.2 Fonti, geometria e validazione

- **Fonti**: PDF dei regolamenti (elenco biglietti con punti), BoardGameGeek (rotte verificate community), repo open source esistenti da incrociare (`geckods/ticket-to-ride` ha la mappa USA in `gameconstants.go`).
- **Rendering senza copiare artwork**: le città sono reali → coordinate lat/long + proiezione. Mappa pulita stile "semplificata" (la preferita dalla community), zero IP grafico.
- **Rotte doppie**: due segmenti tra le stesse coordinate si sovrapporrebbero. Il renderer applica a ciascuno un offset perpendicolare ±d lungo la normale normalizzata `n = (−Δy, Δx)/|Δ|`; i dati mappa possono sovrascriverlo con una curva (Bézier quadratica con punto di controllo esplicito) dove l'offset non basta. Regola definita in Fase 0.
- **Validazione obbligatoria** (tool `ttr-validate`, Fase 0): ogni città dei biglietti esiste; ogni rotta collega città esistenti; ogni biglietto raggiungibile (connessione); conteggi carte e biglietti per mazzo coerenti col ruleset; numero segmenti coerente col tabellone; ogni coppia di città ha al massimo due rotte, e le doppie sono marcate su entrambe.

---

## 5. Architettura

### 5.1 Workspace

```
ciufciuf/                        # repo GitHub privato kidgvbs/ciufciuf — CiufCiuf! (D19, D23)
├── RECAP.md            # context-pack: punto d'ingresso per qualunque agente (D23)
├── crates/
│   ├── ttr-core/       # motore: puro, deterministico, map-agnostic
│   ├── ttr-server/     # Axum: WebSocket, lobby, QR/mDNS, persistenza, statici
│   ├── ttr-validate/   # validazione dati mappa (CLI)
│   └── ttr-proto/      # tipi del contratto WS (serde) + export ts-rs per il client
├── data/
│   ├── maps/           # cities, routes, tickets per mappa
│   └── rulesets/       # descriptor per ruleset
├── client/             # TypeScript + Svelte 5 (D22): table view + controller view (stessa codebase)
└── docs/               # piano (docs/plan/ciufciuf-v1.3.md) + regole estratte per mappa
```

### 5.2 Core engine (`ttr-core`)

- **API**: `Applica(Azione) → [Eventi]`. Nessuna conoscenza di UI/rete.
- Azioni: `PescaCarta`, `PrendiBiglietti`, `ScegliBiglietti`, `ReclamaRotta`, `CostruisciStazione` (Europe), `DecidiTunnel` (Europe).
- RNG seeded → replay = seed + log azioni. Test golden dal regolamento + fuzzing self-play.
- **Hooks per ruleset**: costo claim (tunnel/traghetti), regole pesca, mazzi di biglietti, bonus end-game, meccaniche speciali (stazioni) — implementati come moduli `Ruleset` che il core interpella.

### 5.3 Server (`ttr-server`)

- Axum + Tokio in ascolto su `0.0.0.0`: WebSocket, statici del client.
- **QR con IP numerico** (D16): all'avvio il server enumera le interfacce (crate `local-ip-address`). Se l'host ha **più interfacce LAN attive** (es. Mac con Ethernet + Wi-Fi), la lobby mostra **tutti gli IP candidati**, ciascuno col proprio QR selezionabile (default: l'interfaccia con route verso la rete del telefono, se individuabile). `http://ttr.local:<porta>` (mDNS) compare solo come testo di fallback sulla table view.
- Lobby: creazione partita, scelta mappa/ruleset, join, assegnazione colori.
- **Sessioni e reconnect**: ogni stato rientrabile via snapshot + eventi (i telefoni si addormentano).
- Persistenza (D18): una partita = un file JSON (seed + log azioni) in `saves/`; ripresa a serata diversa.
- **Containerizzazione (D8)**: `Dockerfile` multi-stage (build Rust in container → immagine distroless/debian-slim) + `docker-compose.yml` per avvio unico sulla LAN. Il codice resta pensato per il binario statico (opzione fallback diretta sul Mac), così il container è un wrapper, non un vincolo. Nota: in container la scansione delle interfacce vede solo quella del container → pubblicare la porta (`ports:`) e usare l'IP host per il QR, oppure `network_mode: host` non essendo disponibile su Docker Desktop macOS → **preferire il fallback "binario statico sul Mac" per il gioco serale e usare il container per CI/riproducibilità**.

### 5.4 Client (`client/`, TypeScript + Svelte 5 — D22)

- **Table view** (TV/browser HDMI): solo stato pubblico.
- **Controller view** (telefono, portrait): mano privata, biglietti propri, azioni, mosse legali.
- Tipi importati da `ttr-proto` (ts-rs): drift Rust↔TS impossibile per costruzione.
- **Svelte 5 (D22)**: le due viste sono componenti Svelte; lo stato pubblico/privato dal WS finisce in store reattivi (uno per canale, §7); la mappa è SVG inline. Contratto WS e tipi ts-rs restano framework-agnostic per costruzione: l'unica superficie Svelte sono le due viste → eventualmente sostituibile senza toccare il backend.
- **Assets grafici**: **nessun artwork ufficiale** Days of Wonder/Asmodee — né estratto dall'app né scaricato (copyright, v. §12). La mappa è SVG disegnata da zero da coordinate geografiche reali (§4.2), palette e icone originali o da librerie libere (es. Lucide). v1.1→v1.2: confermato con l'utente il 3/11.
- **Standby del telefono** (D17):
  - Listener `visibilitychange`: al ritorno visibile il client riapre il WebSocket se chiuso e invia `REQUEST_SNAPSHOT`.
  - Wake Lock: `navigator.wakeLock.request('screen')` dopo un gesto dell'utente, riacquisito a ogni ritorno visibile (il sistema lo rilascia quando la pagina va in background). L'API esiste solo in *secure context*: su `http://<ip-lan>` `navigator.wakeLock` è `undefined`, quindi serve HTTPS (certificato self-signed → avviso sul telefono) oppure un fallback "video muto in loop" (tecnica NoSleep.js) che funziona anche su HTTP. Scelta in §14.

### 5.5 AI (`ttr-ai`, Fase 5)

Bot euristico BFS/Dijkstra + greedy che consuma la stessa API di azioni del core. Livello "riempi il tavolo / gioca da soli". Niente RL.

---

## 6. Macchina a stati

Stati del server (dal GDD esterno, completati e corretti):

| # | Stato | Chi agisce | Note e reconnect |
|---|---|---|---|
| 1 | `WAITING_FOR_PLAYERS` | lobby | Join via QR; quorum 2–5; riconnessione = rientro in lobby |
| 2 | `INITIAL_DRAFT` | **tutti in parallelo** (barriera) | Non è un turno: ogni giocatore sceglie i biglietti contemporaneamente (Europe: 1 lungo + 3 regolari); si parte quando tutti hanno confermato. Reconnect: chi rientra riprende la propria scelta |
| 3 | `PLAYER_TURN` | giocatore di turno | Azioni §3.1 (+ `BUILD_STATION` in Europe, che chiude il turno); le mosse legali sono nel canale privato |
| 4 | `SECOND_CARD_DRAW` | giocatore di turno | Solo se la prima pescata NON era locomotiva scoperta; secondo messaggio di pesca |
| 5 | `TICKET_CHOICE` | giocatore di turno | Post-pesca biglietti mid-game: scelta di quali tenere (min 1) |
| 6 | `LAST_ROUND` | tutti (incluso chi innesca) | Non è uno stato per-giocatore: è il **modo del giro** (banner + contatore turni rimasti). Termina dopo il turno di `lastRoundTriggerPlayer` |
| 7 | `TUNNEL_RESOLUTION` | giocatore di turno | Europe: `CLAIM_ROUTE` su tunnel → server rivela fino a 3 carte e pubblica il costo extra → `TUNNEL_DECISION`: paga (rotta reclamata) o rinuncia (carte in mano, turno finito). In entrambi i casi le rivelate vanno negli scarti e il turno passa |
| 8 | `STATION_SCORING` | server, fine partita Europe | Per ogni giocatore il server sceglie l'assegnazione stazione→rotta avversaria che massimizza il punteggio biglietti (§9) e somma +4 per stazione non costruita. Nessun input dei giocatori |
| 9 | `GAME_OVER` | nessuno | **Schermata con ripartizione punti** per giocatore: rotte, biglietti +, biglietti −, bonus, stazioni (con le rotte prese in prestito) — la feature che l'app Marmalade ha tagliato e la community ha preteso |

---

## 7. Contratto WebSocket

**Decisioni**: WebSocket raw + JSON (no STOMP, D6). Tre canali: broadcast pubblico, unicast privato per giocatore, errori. Sequence number per client → idempotenza al reconnect (i telefoni ritrasmettono). Tipi generati con ts-rs da `ttr-proto`. Campo `protocolVersion` in ogni SNAPSHOT.

### 7.1 Inbound (client → server): `game/{gameId}/action`

```jsonc
// Pesca carta — NON atomica: due messaggi per completare il turno di pesca
{ "playerId": "…", "actionType": "DRAW_TRAIN_CARD",
  "payload": { "source": "faceUp", "index": 2 } }        // oppure { "source": "deck" }

// Scelta biglietti (stesso tipo per INITIAL_DRAFT e TICKET_CHOICE)
{ "playerId": "…", "actionType": "CHOOSE_TICKETS",
  "payload": { "keep": ["den_pho", "sea_nyc"] } }         // min per ruleset; scarti → fondo del mazzo di provenienza in ordine

// Reclamo rotta — cardsDiscarded è un SUGGERIMENTO: il server valida e risolve
// il pagamento (locomotive automatiche, come BGA). Su un tunnel apre TUNNEL_RESOLUTION.
{ "playerId": "…", "actionType": "CLAIM_ROUTE",
  "payload": { "routeId": "den_pho_1", "cardsDiscarded": ["RED", "RED", "LOCOMOTIVE"] } }

// Europe
{ "actionType": "TUNNEL_DECISION", "payload": { "pay": true, "cardsDiscarded": ["…"] } }  // pay:false → turno finito
{ "actionType": "BUILD_STATION",  "payload": { "cityId": "paris", "cardsDiscarded": ["…"] } } // azione del turno

// Riallineamento dopo standby/reconnect
{ "playerId": "…", "actionType": "REQUEST_SNAPSHOT", "payload": {} }
```

### 7.2 Outbound — broadcast pubblico: `game/{gameId}/state`

Stato completo senza nulla di privato (congiunto per i client e per la table view):

```jsonc
{ "gameId": "…", "protocolVersion": 1,
  "state": "PLAYER_TURN", "currentTurnPlayerId": "…",
  "lastRound": false, "lastRoundTriggerPlayerId": null,
  "visibleDeck": ["RED", "BLUE", "LOCOMOTIVE", "GREEN", "YELLOW"],
  "claimedRoutes": [{ "routeId": "den_pho_1", "claimedByPlayerId": "…" }],
  "stations": [{ "cityId": "paris", "playerId": "…" }],            // Europe
  "tunnelReveal": null,   // Europe, in TUNNEL_RESOLUTION: { "routeId", "cards": [...], "extraCost": 1 }
  "playersSummary": [{ "playerId": "…", "name": "Gino", "color": "CYAN",
                       "trainsRemaining": 42, "stationsRemaining": 3, "score": 7,
                       "cardsHandCount": 5, "ticketsCount": 3 }] }
```

### 7.3 Outbound — unicast privato: `user/{playerId}/private`

```jsonc
{ "seq": 128,
  "hand": ["RED", "RED", "BLUE", "LOCOMOTIVE"],
  "tickets": [{ "id": "den_pho", "points": 11, "completed": false }],
  "legalMoves": { "routes": ["den_pho_1", "sea_hel_2"],
                  "drawFaceUp": [0, 2, 4], "canDrawDeck": true, "canDrawTickets": true,
                  "stationCities": ["paris", "wien"] } }                 // Europe
```

Nota: `tickets[].completed` live alimenta il feedback "biglietto completato" (§9; in Europe include l'assegnazione ottima delle stazioni già costruite). `legalMoves` dal server = client scemo, regole in un posto solo.

### 7.4 Outbound — errori e snapshot

```jsonc
{ "type": "ERROR", "seq": 129, "reason": "INSUFFICIENT_CARDS",
  "actionRef": "CLAIM_ROUTE den_pho_1" }
{ "type": "SNAPSHOT", "protocolVersion": 1, /* stato pubblico + privato del client */ }
```

`SNAPSHOT` inviato a ogni (re)connect e in risposta a `REQUEST_SNAPSHOT`.

---

## 8. Specifiche UI/UX

(Base: sezione 5 del GDD esterno, con correzioni concordate.)

### 8.1 Due viewport

| Vista | Device | Forma |
|---|---|---|
| **Table view** | TV/browser via HDMI | Landscape 16:9, min 1920×1080; pan e zoom sulla mappa; QR con IP numerico + URL `ttr.local` testuale in lobby |
| **Controller view** | Telefono | Portrait; scala propria |

Le dimensioni del GDD sono **proporzioni di riferimento** (unità relative, non px assoluti): carte ~100×150, biglietti ~120×80, singolo vagone ~40×15. Colori giocatore: rosa, ciano, lime, marrone, lavanda, indaco (dal 18/11/2026; prima rosso, blu, verde, giallo, nero, troppo vicini ai colori delle tratte).

### 8.2 Feedback e animazioni (adottate dal GDD)

- **Hover/click su rotta**: glow/stroke sulle rotte **legali per il giocatore** (usa `legalMoves` dal server — niente ricalcolo client).
- **Pesca**: ease-out dal deck/pool alla mano (controller view) e aggiornamento sincrono dei conteggi (table view).
- **Claim**: carte usate si rimpiccioliscono verso gli scarti; trenini staggered fade-in sulla rotta; sbuffo di vapore (suoni in Fase 5).
- **Tunnel** (Europe): le 3 carte rivelate compaiono sulla table view con evidenziate quelle che aumentano il costo; il controller mostra "Paga N carte / Rinuncia".
- **Biglietto completato**: lampeggio verde/spunta animata (dati live dal server, §9).
- **ULTIMO GIRO**: banner su table e controller quando `lastRound = true`.
- **GAME_OVER**: schermata ripartizione punti per giocatore + highlight del percorso più lungo + rotte prese in prestito dalle stazioni.
- Accessibilità colori (pattern/forme oltre al colore): P3, non bloccante.

---

## 9. Algoritmi

(Base: sezione 6 del GDD esterno; stack-agnostici, qui in Rust.)

1. **Union-Find per giocatore** (dal GDD ✓): aggiornamento incrementale a ogni claim sulle **rotte proprie** (O(α)); verifica biglietti O(1). Alimenta il feedback live `tickets[].completed` per USA e per i giocatori Europe senza stazioni.
2. **Stazioni (Europe)**: per ogni stazione costruita i candidati sono le rotte avversarie reclamate incidenti alla sua città, più "nessuna". Si enumerano tutte le assegnazioni (prodotto dei candidati, al più qualche centinaio con 3 stazioni), per ognuna si copia l'Union-Find delle rotte proprie e si uniscono le rotte prestate, e si tiene l'assegnazione che massimizza la somma dei biglietti (completati − falliti). Una rotta prestata vale per tutti i biglietti; usato a fine partita (`STATION_SCORING`) e per il feedback live.
3. **Longest path / European Express: DFS con backtracking** (dal GDD ✓): cammino semplice sul multigrafo delle **sole rotte proprie** (≤45 archi, grado basso) — calcolato a fine partita per il bonus e per l'highlight. Nessuna memoizzazione necessaria.
4. **Mosse legali lato server**: il private channel include ciò che il giocatore può fare ora; il client valida zero regole.
5. **Determinismo** (Rust-specifico): RNG seeded + `BTreeMap`/ordinamenti espliciti, anche nell'ordine di enumerazione delle assegnazioni stazione (v. §3.3, §11).
6. **Nota per il futuro (India)**: il bonus Mandala richiede due percorsi *distinti* verificati tra A e B — Union-Find non basta, servirà ricerca di percorso. Voce di debito tecnico, non di Fase 4.

---

## 10. Roadmap a fasi

| Fase | Contenuto | Stima | Criterio di accettazione |
|---|---|---|---|
| **0** | Workspace + `ttr-proto` + dati USA + `ttr-validate` + regola geometrica rotte doppie | 3–5 gg | Dati USA passano tutti i check; golden test di base verdi |
| **1** | `ttr-core`: regole USA complete, macchina a stati (§6), RNG seeded, CLI debug | 5–10 gg | 1000 partite fuzzing senza stati illegali; replay deterministico; test golden dal regolamento |
| **2** | `ttr-server`: WS + protocollo §7 + lobby + QR (multi-IP) + profili (nome+colore) + reconnect + persistenza JSON | 4–7 gg | 2 browser + 2 client simulati completano una partita via LAN; un client si disconnette a metà e rientra senza perdere nulla |
| **3** | Client **Svelte 5** (D22): table view (mappa SVG USA) + controller view + gestione standby (`visibilitychange` + wake lock/fallback) | 10–20 gg | **Partita completa 2–5 giocatori da telefoni veri via QR**; un telefono lasciato in standby rientra allineato — il primo traguardo "giocabile con amici" |
| **4** | Europe: dati (doppio mazzo biglietti), tunnel/traghetti, stazioni come azione, TUNNEL_RESOLUTION, STATION_SCORING, European Express | 4–8 gg | Partita Europe completa su LAN, con schermata punti a fine partita — **completata 11/11/2026**; poi modalità USA/Europe (12/11) e Germany/Poland/Basilicata (17–18/11) |
| **5** | AI euristica + polish: **statistiche cumulative per profilo (D20: `profiles.json`, stile "stelle cumulate" Luna)**; tutorial; suoni solo se avanza voglia (D21) | 5–10 gg | Bot decente riempie i posti; serata interrotta e ripresa; statistiche per profilo persistenti |
| aperto | Pass & Play fallback (D10), container Docker come deployment quotidiano (D8), altre mappe 2023 (oltre a Germany/Poland già fatte), replay UI, audio (D21) | a piacere | Ogni mappa = dati + ruleset module, zero fork |

**Traguardi**: giocabile con amici a fine Fase 3 (~20–35 giornate); "per intero" (USA+Europe+AI) a fine Fase 5.

**Stato al 18/11/2026**: Fasi 0–4 completate e verificate (dettagli in `docs/history.md`). Restano da confermare il criterio di accettazione della Fase 3 con 2–5 telefoni veri via QR e i prossimi fix in `RECAP.md` §2; poi Fase 5.

Ordine deliberato: **motore prima della UI, dati prima del motore**. La Fase 3 è il rischio maggiore (rendering mappa = la voce più sottovalutata del piano); la Fase 4 è dove si smette di faticare e si gioca.

---

## 11. Registro rischi

| # | Rischio | Impatto | Mitigazione |
|---|---|---|---|
| R1 | Rendering mappa (Fase 3) sottovalutato | Alto | SVG da coordinate reali; niente 3D; vista "semplificata" come target; offset/curve per rotte doppie definiti in Fase 0 |
| R2 | Drift contratto↔implementazione | Alto | `ttr-proto` + ts-rs; test round-trip dei payload |
| R3 | **Non-determinismo Rust** (iterazione HashMap con seed per-process) | Alto (replay/test) | Regola di progetto: `BTreeMap`/ordinamento esplicito ovunque conta; test replay golden |
| R4 | Dati Europe imprecisi (tunnel/traghetti, mazzo lunghi/regolari) | Medio | PDF Europe + `ttr-validate` + verifica community |
| R5 | Reconnect dei telefoni (sleep, Wi-Fi instabile) | Medio | Event sourcing: snapshot + seq numbers; `REQUEST_SNAPSHOT` su `visibilitychange`; test di disconnessione in Fase 2 |
| R6 | Wake Lock non disponibile su `http://` LAN | Medio | Il reconnect resta il meccanismo primario; HTTPS self-signed o fallback video (§14) |
| R7 | Wi-Fi con isolamento client (rete ospiti, AP isolation) o multicast bloccato | Medio | QR con IP numerico; messaggio in lobby che chiede di usare la rete principale; mDNS solo fallback |
| R8 | Scope creep mappe | Medio | Una mappa per volta; ruleset descriptor. **Materializzato** il 17/11 (Germany, Poland, Basilicata prima della Fase 5, per scelta dell'utente): contenuto da dati + `ttr-validate` + partite casuali su tutti i ruleset; i dati Germany non verificati su foto sono in `RECAP.md` §2 |

---

## 12. Note legali

- **Meccaniche**: non copyrightabili → replicarle è lecito.
- **Nome, logo, artwork, testi**: IP Asmodee/Days of Wonder. Uso privato tra amici = rischio pratico nullo.
- **Prassi pulita** (già adottata): grafica propria da coordinate geografiche reali, nessun asset ufficiale estratto o scaricato. **Nome di progetto non-"Ticket to Ride" adottato da subito: CiufCiuf! (D19)** — rinominare dopo sarebbe peggio. Il repo è **privato** (D23); potrà diventare pubblico senza toccare IP Asmodee (meccaniche non copyrightabili; titolo del documento già neutro).

---

## 13. Riferimenti

- **Regolamenti PDF** (EN/IT/DE/ES…): daysofwonder.com → pagina del gioco → "The Rules". USA EN: `7201N_TICKET2RIDEV2_RULES_EN_20250425_WEB.pdf`; IT: `7261N_…_IT_….pdf` su `cdn.svc.asmodee.net/production-daysofwonder`. Europe: regolamento Europe dalla stessa pagina.
- **Board Game Arena** (implementazione licenziata, riferimento implementativo): `en.boardgamearena.com/doc/Gamehelptickettoride` — regola ultimo giro (D11) e parametri varianti (§3.4).
- **App 2023 (Marmalade)**: Steam app 2477010 (FAQ ufficiale), Marmalade Game Studio (news espansioni).
- **Wikipedia**: "Ticket to Ride (board game)" e "Ticket to Ride (video game)".
- **Open source di riferimento**: `geckods/ticket-to-ride` (Go, engine+bot+GA), `Kiryushin-Andrey/TicketToRide` (Kotlin web), `JMooreo/TicketToRideAI` (RL), `ss2cp/AI-TicketToRide` (BFS+annealing).
- **Web API**: Screen Wake Lock API (MDN; richiede secure context), NoSleep.js (fallback video).
- **Studio preliminare**: superato da questo piano.

---

## 14. Punti aperti

In ordine di importanza:

1. ~~Set biglietti USA~~ **CHIUSO (v1.2)**: l'utente ha scelto le regole più recenti; verificate sul PDF ufficiale V2 2025 (§13): **33 biglietti** (30 classici + Atlanta–Washington 4, Omaha–New Orleans 8, San Francisco–Washington 21 — identici a USA 1910) e **setup 4 biglietti / min 2**. Ruleset di riferimento: **`usa_v2`** (aggiornato in §3.1 e §4.1); `usa_classic` resta nel descriptor solo per compatibilità/eventuali confronti.
2. ~~Composizione biglietti di 1910 / Mega / Big Cities~~ **CHIUSO (12/11/2026, D34–D37)**: ruleset `usa_1910`, `usa_big_cities`, `usa_mega` e le modalità Europe come dati; fonti in `docs/data-sources-usa.md` e `docs/data-sources-europe.md`.
3. ~~Standby dei telefoni~~ **CHIUSO (decisione utente 3/11)**: **HTTPS con certificato self-signed** come target (Wake Lock API nativa, che richiede secure context). Server con TLS (Axum supporta `rustls` direttamente) + QR che punta a `https://<ip-lan>:<porta>`; sul telefono la prima visita mostra l'avviso di sicurezza del certificato → UI di lobby con istruzioni esplicite "accetta il certificato" (esperienza classica degli strumenti self-hosted). Il meccanismo primario resta comunque il **reconnect + snapshot** (D17): Wake Lock è best-effort per evitare che lo schermo si spenga, non è necessario alla correttezza. Implementazione in Fase 3.
4. ~~Framework client~~ **CHIUSO (D22, decisione utente 3/11: "usiamo svelte")**: **Svelte 5** — bundle piccolo per i telefoni, reattività semplice per la mano privata, SVG inline naturale per la mappa, meno boilerplate di React per "una mappa + pannelli". Il traffico WS a turni è basso → nessun vincolo di prestazioni. Salvaguardia: contratto WS (§7) e tipi ts-rs framework-agnostic; l'unica superficie Svelte sono le due viste (§5.4) → eventualmente sostituibile senza toccare il backend.
5. ~~Hardware host~~ **Chiuso (D8)**: Mac esistente + containerizzazione; binario statico come fallback per il gioco serale (Docker Desktop macOS non ha `network_mode: host`).
6. ~~Nome del progetto~~ **Chiuso (D19)**: **CiufCiuf!**.
7. ~~Audio~~ **Chiuso (D21)**: posticipato; prima il gioco che funziona.
8. ~~Persistenza~~ **Chiusa (D18/D20)**: partite → JSON in `saves/`; **statistiche cumulative per profilo** → `profiles.json` in Fase 5.

---

*Fine del piano v1.3. §14 è chiuso: restano solo i ruleset futuri (1910/Mega/Big Cities), rinviati per design. Casa del progetto: `~/Rust/ciufciuf`, repo GitHub privato `kidgvbs/ciufciuf` con `RECAP.md` context-pack alla radice (D23). Prossimo passo: pacchetto Fase 0+1 per la pipeline coding→reviewer (workspace `ciufciuf`, `ttr-proto`, dati USA `usa_v2` validati, motore USA con macchina a stati e test).*
