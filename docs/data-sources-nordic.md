# Fonti dati Nordic Countries — CiufCiuf!

Ruleset `nordic`, mappa `data/maps/nordic.json` (39 città, 81 binari in 67 coppie di città, 46 biglietti, 207 treni totali di rotte).

## Regolamento

**Ticket to Ride Nordic Countries**, EN: https://cdn.svc.asmodee.net/production-asmodeeca/uploads/2023/07/7208-T2RNC-Rules-EN-2018.pdf (riassunto: https://edp.org/Games/TicketToRideNordicCountries.pdf; stesso testo su https://ultraboardgames.com/ticket-to-ride/nordic-countries.php).

- 2–3 giocatori, 40 treni, mazzo da 110 carte (12 per colore × 8 + 14 locomotive), mano iniziale di 4 carte, 5 carte scoperte.
- Biglietti: 46. Iniziali 5, tieni ≥2; pesca 3, tieni ≥1; **gli scartati escono dalla partita**, sia al setup sia a metà partita; con meno di 3 biglietti nel mazzo si pescano quelli che ci sono.
- Locomotive usabili **solo** su tunnel e traghetti. Pesca: nessun limite sulle locomotive («Unlike most Ticket To Ride games there is no restriction on the number of Locomotive cards you may draw per turn»): una locomotiva scoperta vale come carta normale, anche come seconda. La regola delle 3 locomotive scoperte non esiste nel regolamento.
- Traghetti: una locomotiva per ogni simbolo locomotiva; per gli altri spazi le carte del colore; locomotive aggiuntive al posto di carte colore; **3 carte qualsiasi valgono 1 locomotiva**.
- Tunnel: come Europe (3 carte scoperte dal mazzo).
- Murmansk–Lieksa (9, grigia): ogni carta di colore può essere sostituita da 4 carte qualsiasi, locomotive comprese («7 green cards and any 8 other cards»).
- Doppie: in 2 giocatori un solo binario, un giocatore non prende mai entrambi. Vale anche per le coppie con binari di lunghezza diversa (Mo i Rana–Trondheim, Stavanger–Kristiansand): [INFERENCE] dal testo «Some cities are connected by Double-Routes», non c'è un esempio stampato.
- Punti rotta: 1/2/4/7/10/15 per 1–6, 27 per 9 (tabella del regolamento; la lunghezza 9 è citata anche dalla wiki di fan: «9 space route worth 27 points»). Nessuna rotta da 7 o 8.
- Fine: ≤2 treni a fine turno → ultimo giro per tutti, chi lo innesca compreso. Globetrotter +10 (pari merito: tutti). Niente percorso più lungo come bonus.
- Spareggio: più biglietti completati, poi percorso più lungo (lunghezza, anche senza bonus).
- Niente stazioni.

## Mappa: rotte e città

| Fonte | Contenuto |
|---|---|
| Wiki di fan, `Nordic_Countries:_Cities_and_Routes` (via https://ticket-to-ride.fandom.com/api.php) | 39 città con coordinate sul tabellone in percentuale, 70 rotte = 81 binari (10 tunnel, 24 traghetti), lunghezze, colori, simboli locomotiva |
| tallguy-h/LibreLine, `docs/maps/nordic.md` (https://raw.githubusercontent.com/tallguy-h/LibreLine/main/docs/maps/nordic.md) | 39 città, 81 binari con lunghezza, colore, tunnel, traghetti con numero di locomotive; 46 biglietti |
| shadowCow/ttr-nordic-analysis, `analysis/game.go` (https://github.com/shadowCow/ttr-nordic-analysis) | 39 città, 69 rotte (coppia + lunghezza, senza colori né binari doppi), 46 biglietti |
| Foto del tabellone vuoto (BGG, https://boardgamegeek.com/image/419030, https://cf.geekdo-images.com/Nn0qyT1-jeedyFvdGAmvQw__large/img/E2jv9f5YhHdLlBz7s8CCPX8sgpw=/fit-in/1024x1024/filters:no_upscale():strip_icc()/pic419030.jpg) e griglia delle città (BGG, https://boardgamegeek.com/image/378839) | verifica a vista di colori, tunnel, traghetti e doppie |

- **Riconciliazione delle rotte:** wiki e LibreLine coincidono su tutti gli 81 binari (coppia, lunghezza, colore, tipo, locomotive) tranne uno. shadowCow coincide sulle 69 coppie con lunghezza e non ha colori.
- **Una discrepanza:** Åndalsnes–Bergen (5, traghetto, 2 locomotive): la wiki dice grigia, LibreLine nera. Sulla foto del tabellone vuoto i quadrati dell'arco sono neri (uguali a quelli di Kristiansand–Oslo), non grigio chiaro come Murmansk–Lieksa. Nei dati: **Black**.
- **Binari doppi:** 14 coppie con 2 binari (28 binari; nessuna tripla). Sono quelle della wiki con più colori («Blue / Red») e le coppie che compaiono due volte in tabella. Due coppie hanno binari di lunghezza diversa: Mo i Rana–Trondheim 6 (rosso, traghetto, 2 locomotive) + 5 (verde, tunnel) e Stavanger–Kristiansand 3 (arancio, traghetto, 1) + 2 (verde, tunnel); anche Stockholm–Helsinki ha due binari di traghetto da 4 (giallo con 1 locomotiva, grigio con 2). Tutte e tre sono `double: true`, con lunghezze diverse dove servono (vedi `docs/data-conventions.md`).
- **Id dei binari:** `{a}_{b}_{n}` con le città in ordine lessicografico; nelle coppie doppie `_1` è il binario nell'ordine della fonte (wiki/LibreLine).
- **Traghetti:** `ferries` = numero di simboli locomotiva (non di spazi). Sono 24 binari, nessuno è anche tunnel. Murmansk–Lieksa ha `anyCardsPerSpace: 4`.
- **Verifica dei totali:** 81 binari (28 doppi), lunghezze 1×17, 2×28, 3×20, 4×11, 5×3, 6×1, 9×1 = 207 treni; 10 tunnel, 24 traghetti, come dichiara la wiki.
- **Nomi:** forma del tabellone, con i segni diacritici (København, Göteborg, Tromsø, Åndalsnes…). Id in snake case senza segni (`kobenhavn`, `goteborg`, `tromso`, `mo_i_rana`).
- **Coordinate:** lat/lon reali delle città (gradi con 2 decimali), confrontate con le coordinate di Wikipedia EN (scarto massimo 0,13° per Narvik, gli altri sotto 0,1°). Il layout non le usa.
- **Non verificati su foto:** i simboli locomotiva e i colori di tutti i traghetti sono confermati da due fonti testuali concordi (wiki, LibreLine); sulla foto (bassa risoluzione) ho controllato a vista solo i colori e le doppie leggibili (zona Oslo, Stockholm, Kiruna, Umeå, Karlskrona, i tunnel norvegesi). Il conteggio dei simboli locomotiva è quindi letto dalle fonti, non contato sulla foto.

## Biglietti

- **Fonti:** wiki di fan `List_of_Nordic_Countries_Destination_Tickets` (46), LibreLine `docs/maps/nordic.md` (46), shadowCow `game.go` (46). Le tre liste coincidono per coppia e valore. [INFERENCE] LibreLine e shadowCow possono derivare dalla stessa trascrizione, e la foto delle carte non è stata controllata una per una.
- **Verifica:** tutti e 46 i valori coincidono con il percorso minimo sulla mappa (treni), come dichiarato dal regolamento («Point Value» = distanza); nessuna eccezione. Il più alto è København–Murmansk 24.
- **Conteggio:** 46 biglietti, `sets: ["standard"]`, `deck: "regular"`, un solo mazzo. Ordinati per id.

## Layout

Il layout parte dalle posizioni sul tabellone in percentuale della wiki (`MAP_BOUNDS.nordic.image` in `client/tools/gen-schematic-layout.mjs`, scalate a 671×1000): la geografia reale non rispetta le lunghezze (Murmansk–Lieksa da 9 carte, traghetti).
