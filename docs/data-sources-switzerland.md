# Fonti dati Switzerland — CiufCiuf!

Ruleset `switzerland`, mappa `data/maps/switzerland.json` (Map Collection 2).

## Regolamento

**Ticket to Ride Map Collection 2: Switzerland** (Days of Wonder), EN/FR/DE/ES…: https://cdn.svc.asmodee.net/staging-daysofwonder/uploads/2024/07/720114-T2RMC2-Rules_switzerland-ML-2017-4.pdf

- 40 treni, 2–3 giocatori, mazzo da 110 carte (12×8 + 14 locomotive).
- 3 rotte verso l'Austria, 4 verso la Francia, 5 verso l'Italia, 5 verso la Germania.
- 46 biglietti: 34 città–città, 12 con più destinazioni (4 città→Paesi, 8 Paese→altri Paesi, 4 modelli in 2 copie: thread BGG «Duplicate Country Tickets», https://boardgamegeek.com/thread/225973). Vale il punteggio più alto tra le destinazioni collegate; se nessuna è collegata si perde il valore più basso.
- 5 biglietti iniziali (tieni ≥2), pesca di 3 (tieni ≥1); gli scartati, a inizio partita e dopo, escono dal gioco, quindi il mazzo può finire. La versione danese del regolamento dice invece «in fondo alla pila» ma contraddice le altre (EN, FR, DE, ES, FI, SV, NO): vale la maggioranza.
- Locomotive: pescabili come carte normali (anche due in un turno, anche scoperte); con 3 locomotive tra le 5 scoperte il mercato si scarta e si rimpiazza; si giocano solo sui tunnel.
- Tunnel come Europe. Niente traghetti, niente stazioni.
- Doppie: con 2 giocatori un solo binario, con 3 entrambi.
- Percorso più lungo: +10.
- **Punti per lunghezza:** 1/2/3/4/5/6 → 1/2/4/7/10/15, la tabella standard; il regolamento non la riporta e la mappa non ha rotte da 7 o più. Non verificata su una foto del tabellone.

## Mappa

| Fonte | Contenuto |
|---|---|
| Wiki di fan, `Switzerland:_Cities_and_Routes` (via https://ticket-to-ride.fandom.com/api.php) | 34 città, 17 bandierine di confine, 76 rotte / 88 binari con colori, tunnel e coordinate in percentuale sul tabellone |
| Wiki di fan, `List_of_Switzerland_Destination_Tickets` (via `api.php`) | 46 biglietti |
| Progetto didattico EPFL «tCHu» (`David-Kyrat/CS108_Project-tCHu`, `src/ch/epfl/tchu/game/ChMap.java`, https://github.com/David-Kyrat/CS108_Project-tCHu) | 34 città, 17 nodi Paese, 88 rotte con colore, lunghezza e livello (tunnel), 34 biglietti città–città e i 12 con più destinazioni |
| Foto del tabellone (https://static.wikia.nocookie.net/ticket-to-ride/images/0/05/Swiss.jpg, 1000×666) | nomi sul tabellone, posizione dei pallini e delle bandierine, controllo visivo di alcune rotte |

- **Riconciliazione:** ho confrontato wiki e tCHu per script. Le 88 rotte coincidono per estremi, lunghezza, colore e tunnel (nessuna differenza). I 34 biglietti città–città coincidono per coppia e punti. Gli 8 biglietti con più destinazioni (4 modelli) coincidono a mano: stesse destinazioni e stessi valori.
- **Conteggi:** 34 città, 4 Paesi, 88 rotte in 75 coppie (12 gruppi paralleli; Kreuzlingen–Deutschland è una coppia con due varchi indipendenti, non un gruppo), 42 binari tunnel, 17 sbocchi verso i Paesi (3 Österreich, 4 France, 5 Italia, 5 Deutschland: come dice il regolamento), 46 biglietti (34 + 4 + 8).
- **Nomi:** quelli del tabellone: DEUTSCHLAND, FRANCE, ITALIA, ÖSTERREICH; `st_gallen` è «St. Gallen», `la_chaux_de_fonds` «La Chaux-de-Fonds». Id dei Paesi: `deutschland`, `france`, `italia`, `osterreich`.
- **Paesi come nodo unico:** sul tabellone ogni Paese ha più bandierine; nei dati è una sola città `country: true`, quindi le rotte verso lo stesso Paese non sono collegate fra loro (vicoli ciechi); il motore dà a ogni rotta verso un Paese un nodo privato e un biglietto con estremo Paese è completato da una qualunque. Vale anche per le due bandierine di Kreuzlingen: due varchi di confine indipendenti verso la Germania (lunghezza 1, a est bianca, a ovest arancione, secondo la tabella dei confini della wiki di fan `Switzerland:_Cities_and_Routes`), nei dati `deutschland_kreuzlingen_1` (est) e `_2` (ovest) con `double: false`, senza `alwaysOpen`. Come sul tabellone, uno stesso giocatore può prenderli entrambi e prenderne uno non blocca l'altro, anche in 2 giocatori. Ogni Paese ha un `flag` (ISO alpha-2: DE, FR, IT, AT) per disegnare la bandiera nazionale.
- **Coordinate:** lat/lon reali delle città (arrotondate ai centri abitati). Per i Paesi un punto rappresentativo, solo per il disegno.

## Biglietti

- **Id dei biglietti con più destinazioni:** `{from}_{to1}_{to2}…` con le destinazioni in ordine lessicografico; la seconda copia ha il suffisso `_2` (`deutschland_france_italia_osterreich_2`).
- **Città→Paesi:** Bern (France 5, Deutschland 6, Italia 8, Österreich 11), Chur (Österreich 3, Italia 5, Deutschland 6, France 12), Lugano (Italia 2, Deutschland 12, Österreich 13, France 14), Zürich (Deutschland 3, France 7, Österreich 7, Italia 11).
- **Paese→Paesi (×2):** Deutschland (France 5, Österreich 5, Italia 13), France (Deutschland 5, Italia 11, Österreich 14), Italia (Österreich 6, France 11, Deutschland 13), Österreich (Deutschland 5, Italia 6, France 14).
- **Verifica:** tutti i 34 biglietti città–città valgono il percorso minimo sulla mappa (contando i Paesi come vicoli ciechi), e lo stesso vale per i valori città→Paese tranne Chur–France 12 (il minimo calcolato è 13). Nei Paese→Paese Deutschland–Italia 13, Italia–Deutschland 13, Italia–France 11 e France–Italia 11 superano il minimo calcolato (rispettivamente 11, 11, 10, 10): i valori sono quelli stampati sulle carte, uguali in entrambe le fonti, quindi li tengo così. **Non verificati** su una foto delle carte.

## Layout

`data/layouts/switzerland.json` (canvas 1368×881) copre tutte le 38 città e le 88 rotte e **passa tutti i gate** del generatore: `cd client && bun tools/gen-schematic-layout.mjs --map switzerland` esce con 0 (circa 3,5 minuti; `--svg OUT.svg` scrive l'anteprima con le bandierine al loro posto).

- **Perché serviva un metodo apposta:** sul tabellone ogni Paese ha più bandierine (Deutschland 5, Österreich 3, Italia 5, France 4) e ogni bandierina ha una sola rotta; Deutschland tocca Basel e Kreuzlingen a ~8 carte l'una dall'altra. Disegnare il Paese come un punto solo (come fanno Germany e Poland) fa convergere tutte le rotte di confine in un nodo: 37 incroci, 2 label sopra le rotte, 1 rotta nascosta e 1 rapporto di lunghezza fuori gate. Le regole di gioco non cambiano (il motore dà già un nodo privato a ogni rotta di confine): cambia solo il disegno.
- **Bandierine:** `MAP_TUNING.switzerland.splitCountries: true` (opt-in per mappa, solo Switzerland). Prima della simulazione ogni città `country: true` viene sostituita da un nodo `<paese>#<id rotta>` di grado 1 per rotta di confine (17 in tutto, senza label) e le rotte vengono riscritte su quegli estremi. Il footprint della bandierina sul tabellone TV (rettangolo 3:2 da 22×15 px centrato sul nodo, +2 px di bordo/ombra, +1 di respiro) è trattato come una «label» fissa in tutti i controlli: le label delle città, le rotte non incidenti (che vengono piegate via), i pallini, la distanza minima da altre bandierine e il bounding del canvas. Le rotte finiscono esattamente sul nodo: in `routes[id]` l'estremo lato Paese (quello più lontano dalla città svizzera) è la posizione della bandierina. Gli id `<paese>#…` non compaiono in `cities`/`labels`.
- **Kreuzlingen:** le due bandierine sono due rotte indipendenti (`deutschland_kreuzlingen_1` a est, bianca; `_2` a ovest, arancione), come nei dati di mappa.
- **Seme:** `SWISS_SEED` in `client/tools/gen-schematic-layout.mjs`, posizioni per le 34 città e le 17 bandierine (60 px per carta, ancora 0,45; `unit: 60`, `lenOneDense: true`). Si parte dalle posizioni del tabellone (`SWISS_BOARD_PCT`, percentuali della wiki di fan per città e bandierine, foto 1000×666) scalate in modo che la mediana delle rotte misuri 60 px per carta, poi una ricottura simulata (PRNG a seed fisso, 800 mila mosse; il risultato è un dato, non si ricalcola a ogni run) minimizza: rapporti di lunghezza fuori da 0,92–1,35, incroci in linea retta, distanze troppo piccole rotta–rotta/città–rotta/città–città, angoli tra rotte incidenti sotto i 36°, scostamento dalla forma del tabellone (che conserva l'orientamento e l'ordine delle città, ad esempio Locarno a ovest di Bellinzona). Il primo tentativo con le sole posizioni grezze del tabellone lasciava ancora 3 incroci e 14 approcci sotto i 10 px; partire dal seme ottimizzato li azzera.
- **Nomi dei Paesi:** `cities[<paese>]` + `labels[<paese>]` sono dove si scrive il nome (anchor `middle`, `dx` 0), non più il punto di convergenza. Il punto è il più vicino (griglia 6 px) all'ancora che il tabellone dà al nome (`MAP_TUNING.switzerland.countryNameHints`, in % della foto: DEUTSCHLAND in alto a destra, FRANCE a ovest, ÖSTERREICH a est, ITALIA a sud, portate sul layout con un adattamento affine delle città) per cui il box del testo (+6 px) non tocca rotte, bandierine, label, pallini né altri nomi, la bandierina più vicina è dello stesso Paese e il nome sta oltre di essa rispetto alla sua città. Risultato: Deutschland (1144, 78), France (161, 191), Italia (1116, 819), Österreich (1189, 320).
- **Gate (esito):** 0 incroci/sovrapposizioni di rotte (2 prima della risoluzione), 0 conflitti tratta–tratta, 0 rotte «nascosto completo», 0 label-CORE, 0 label–label, 0 label–pallino, 0 conflitti del footprint delle bandierine, 0 nomi di Paese non liberi, 0 violazioni del rapporto di lunghezza (peggiore 1,57× su `schaffhausen|winterthur`), 0 label oltre 32 px (massimo 32,0), 0 label-ownership, 0 doppie con tracce che si incrociano, 0 tratte vicine a città non-estreme.
- **Warning:** approach separation sotto 10 px: 0 (l'hub polish li porta da 15 a 0; minimo 10,3 px). Restano solo avvisi intermedi della pipeline, anteriori al polish: 6 `ADJACENT-BEND` falliti (Davos, Genève, Schaffhausen, St. Gallen, Zürich ×2) e 2 `FAN-OUT` sotto i 34° (Davos `chur`/`sargans` 21,8°, Winterthur `kreuzlingen`/`st_gallen` 21,1°).
- **Altre mappe:** tutto il codice delle bandierine è dietro `splitCountries`; `germany` e `poland` si rigenerano byte-identici (verificato con `git diff --quiet` dopo `--map germany` e `--map poland`).
- **Se cambiano le rotte di confine:** aggiungere o togliere la voce `<paese>#<id rotta>` in `SWISS_SEED` e in `SWISS_BOARD_PCT`, poi rieseguire il generatore.

## Numero ciclomatico

Il bonus del percorso più lungo attiva il controllo del numero ciclomatico (massimo 14). La stima del validatore contava ogni binario di un gruppo parallelo come rotta separata (15 su questa mappa); con un solo binario per gruppo, come può realmente costruire un giocatore, è 11.
