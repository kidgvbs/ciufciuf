# Fonti dati Basilicata — CiufCiuf!

Mappa di prova ricavata da un'immagine (`IMG_0165.JPG`, 512×382, stile Ticket to Ride, quasi certamente generata da IA). Non è un tabellone pubblicato: rotte, colori e lunghezze sono letti dall'immagine; i biglietti non compaiono nell'immagine e sono generati.

## Decisioni dell'utente (17/11/2026)

| Punto ambiguo nell'immagine | Scelta |
|---|---|
| Due «Salerno» (grigio a ovest, pallino rosso «SALERNO» a est) | Il pallino rosso è **Battipaglia** |
| Pallino rosso senza nome a NE di Acerenza | È **Irsina** (l'etichetta è poco sotto) |
| Snodo bianco senza nome a sud di Bari (5 tratte) | Chiamato **Altamura** |
| Monte Vulture, Monte Sirino, Monte Pollino come capolinea | Tenuti come stazioni giocabili |
| Snodo bianco poco sopra Lagonegro | Unito a **Lagonegro** |
| Città su cui una tratta passa senza fermarsi | La tratta si spezza nella città (Melfi–Venosa–Irsina, Matera–Ferrandina–Metaponto) |
| Tratte con vagoni di colori diversi | Colore della maggioranza (Battipaglia–Potenza doppia grigia da 4: 1 rosso + 3 grigi; Senise–Lauria nera da 4: 1 blu + 1 antracite + 2 neri) |
| Biglietti | Generati: valore = numero minimo di vagoni tra le due città |

## Regole di lettura

- **Rimossi** perché non collegano due città: il moncone nero da 1 a ovest di Tricarico e la diramazione nera da metà della Senise–Lauria verso Monte Pollino. Il moncone grigio a nord di Potenza è l'attacco della Potenza–Irsina e non conta come vagone.
- **Rimossa** perché ridondante dopo l'unione con Lagonegro: la antracite da 2 dallo snodo a Lauria (Lagonegro–Lauria resta l'arancione da 1).
- **Colori:** i vagoni neri pieni (luminanza dei pixel 2–25) sono `Black`; quelli antracite o grigio scuro (38–95, con i vagoni disegnati come quelli grigi) sono `Gray`, come i grigi chiari. Bari–Altamura doppia: bianca + grigio scuro → `White` + `Gray`.
- **Lunghezze:** numero di vagoni disegnati. Incerta Monte Vulture–Avigliano: una barra unica, lunga circa due vagoni → 2.

## Coordinate

Lat/lon reali da Wikipedia (it/en) e Wikidata (Acerenza Q52511, Altamura Q19306, Senise Q52676); per i monti la vetta. Il layout schematico non le usa: parte dai pallini dell'immagine (`MAP_BOUNDS.basilicata` in `client/tools/gen-schematic-layout.mjs`). Lecce, per esempio, sta a 4 vagoni da Altamura ma a 140 km. Unico spostamento: Acerenza (che ha una sola rotta) va a est di Tricarico, perché nell'immagine Acerenza–Tricarico incrocia Potenza–Irsina.

## Biglietti

25 biglietti `regular`, set `standard`, scelti in modo deterministico su coppie la cui distanza minima non è coperta da una sola rotta: 6 da 5–6, 7 da 7–8, 6 da 9–10, 4 da 11–12, 2 da 13–14 (valori ottenuti: 6 da 6, 7 da 8, 6 da 10, 1 da 11, 3 da 12, 2 da 14); ogni città compare in 1–3 biglietti, ognuna almeno una volta.

## Ruleset

Scelte di bilanciamento, non presenti nell'immagine: 2–4 giocatori, 25 treni a testa (100 dei 123 vagoni con 4 giocatori), mazzo standard (12 per colore + 14 locomotive), 3 biglietti iniziali da tenerne 2, pesca 3 tieni 1, scarti in fondo al mazzo, Longest Path +10, doppie aperte da 4 giocatori, punti rotta 1/2/4/7/10/15/18/21 per lunghezze 1–8.

## Statistiche

25 città, 38 rotte (2 coppie doppie + 34 singole), 123 vagoni: Gray 34, Black 27, Blue 17, Yellow 15, Orange 11, Red 10, White 6, Green 3, nessuna Purple. Lunghezze 1–8. `ttr-validate`: numero ciclomatico stimato 4 (massimo 14).
