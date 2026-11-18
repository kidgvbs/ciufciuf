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
- **Conteggi:** 34 città, 4 Paesi, 88 rotte in 75 coppie (13 gruppi paralleli, tra cui Kreuzlingen–Deutschland), 42 binari tunnel, 17 sbocchi verso i Paesi (3 Österreich, 4 France, 5 Italia, 5 Deutschland: come dice il regolamento), 46 biglietti (34 + 4 + 8).
- **Nomi:** quelli del tabellone: DEUTSCHLAND, FRANCE, ITALIA, ÖSTERREICH; `st_gallen` è «St. Gallen», `la_chaux_de_fonds` «La Chaux-de-Fonds». Id dei Paesi: `deutschland`, `france`, `italia`, `osterreich`.
- **Paesi come nodo unico:** sul tabellone ogni Paese ha più bandierine; nei dati è una sola città `country: true`, quindi le rotte verso lo stesso Paese non sono collegate fra loro (vicoli ciechi). Vale anche per le due bandierine di Kreuzlingen: il tabellone le tratta come due varchi distinti verso la Germania (bianca e arancione, lunghezza 1), nei dati sono un gruppo parallelo `deutschland_kreuzlingen_1/2` con `alwaysOpen: true`, così restano aperti anche in 2 giocatori. Il motore vieta a uno stesso giocatore di prenderli entrambi: la regola ufficiale non lo vieta, ma il secondo varco non aggiunge collegamenti. **Non verificato** sulle regole ufficiali: il regolamento non parla del caso.
- **Coordinate:** lat/lon reali delle città (arrotondate ai centri abitati). Per i Paesi un punto rappresentativo, solo per il disegno.

## Biglietti

- **Id dei biglietti con più destinazioni:** `{from}_{to1}_{to2}…` con le destinazioni in ordine lessicografico; la seconda copia ha il suffisso `_2` (`deutschland_france_italia_osterreich_2`).
- **Città→Paesi:** Bern (France 5, Deutschland 6, Italia 8, Österreich 11), Chur (Österreich 3, Italia 5, Deutschland 6, France 12), Lugano (Italia 2, Deutschland 12, Österreich 13, France 14), Zürich (Deutschland 3, France 7, Österreich 7, Italia 11).
- **Paese→Paesi (×2):** Deutschland (France 5, Österreich 5, Italia 13), France (Deutschland 5, Italia 11, Österreich 14), Italia (Österreich 6, France 11, Deutschland 13), Österreich (Deutschland 5, Italia 6, France 14).
- **Verifica:** tutti i 34 biglietti città–città valgono il percorso minimo sulla mappa (contando i Paesi come vicoli ciechi), e lo stesso vale per i valori città→Paese tranne Chur–France 12 (il minimo calcolato è 13). Nei Paese→Paese Deutschland–Italia 13, Italia–Deutschland 13, Italia–France 11 e France–Italia 11 superano il minimo calcolato (rispettivamente 11, 11, 10, 10): i valori sono quelli stampati sulle carte, uguali in entrambe le fonti, quindi li tengo così. **Non verificati** su una foto delle carte.

## Layout provvisorio

`data/layouts/switzerland.json` copre tutte le 38 città e le 88 rotte, ma **non passa tutti i gate** del generatore: è provvisorio.

- **Perché è difficile:** i Paesi sono un nodo solo con più bandierine sul tabellone (Deutschland tocca Basel e Kreuzlingen a 1 carta da ciascuno, ma le due città sono a ~8 carte l'una dall'altra), e la mappa ha molti triangoli stretti. Il disegno del tabellone da solo non rispetta le lunghezze (primo tentativo: 7 rotte fuori dal rapporto 0,8–1,6 e 23 incroci).
- **Seme attuale:** posizioni ottimizzate a parte (ricottura simulata sui rapporti di lunghezza 0,8–1,6, nessun incrocio in linea retta, orientamento nord-in-alto del tabellone), in `MAP_BOUNDS.switzerland` di `client/tools/gen-schematic-layout.mjs`, con ancora 0,45; `MAP_TUNING.switzerland`: `unit: 60`, `lenOneDense: true`. Il layout salvato è l'esito di `bun tools/gen-schematic-layout.mjs --map switzerland` con queste voci (canvas 881×746).
- **Gate falliti** (exit 1):
  - 37 incroci/sovrapposizioni di rotte residui (25 conflitti tratta-tratta, 27 conflitti in totale; molti vengono dalle tratte piegate dei nodi di confine);
  - 2 label-CORE (`la_chaux_de_fonds` su `geneve-lausanne` e `geneve-yverdon`) e 1 tratta «nascosta completo» (`chur|sargans` coperta all'85% da `davos|osterreich`);
  - 1 rotta con rapporto di lunghezza critico fuori gate: `geneve|yverdon` 0,78× (282,6 px su 360).
- **Passati:** 0 conflitti label-label, 0 label oltre 32 px, 0 doppie con tracce che si incrociano.
- **Solo warning:** 46 coppie con separazione d'avvicinamento sotto 10 px, 24 fan-out sotto i 34°, 2 label senza candidato pulito.
- **Da fare:** serve un lavoro manuale sul generatore (`linkScale` per coppia, spostamenti `f2Shifts`, o un posizionamento dei Paesi con più bandierine) o un ritocco a mano del layout.

## Numero ciclomatico

Il bonus del percorso più lungo attiva il controllo del numero ciclomatico (massimo 14). La stima del validatore contava ogni binario di un gruppo parallelo come rotta separata (15 su questa mappa); con un solo binario per gruppo, come può realmente costruire un giocatore, è 11.
