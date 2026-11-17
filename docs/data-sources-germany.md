# Fonti dati Germany — CiufCiuf!

Una mappa (`data/maps/germany.json`) e due modalità:

| Ruleset | Modalità | Biglietti | Passeggeri |
|---|---|---|---|
| `germany` | Deutschland | 62 (`sets: deutschland`) | no |
| `germany_2017` | Passeggeri | 89 (`sets: germany`) | 60 |

## Regolamento

- **Ticket to Ride: Germany 2017** (Days of Wonder), EN: https://cdn.svc.asmodee.net/staging-daysofwonder/uploads/2024/07/tg_rules_2017_en.pdf
  - 45 treni, 2–5 giocatori, mazzo 12×8 + 14 locomotive;
  - pesca di 4 biglietti nella combinazione corti/lunghi scelta dal giocatore, tieni ≥2 all'inizio e ≥1 dopo, scarti in fondo al proprio mazzo;
  - doppie e triple: con 2–3 giocatori un solo binario;
  - Paesi vicoli ciechi;
  - Globetrotter 15;
  - passeggeri con premi 20/10 per colore;
  - spareggio: biglietti completati, poi passeggeri.
- **Punti per lunghezza** (1–7): 1/2/4/7/10/15/18, dalla tabella del regolamento (immagine su https://ultraboardgames.com/ticket-to-ride/gfx/germany8.jpg) e dal regolamento Märklin, che usa la stessa mappa.

## Mappa

- **Rotte e biglietti di Deutschland:** tallguy-h/LibreLine `docs/maps/germany.md` (https://raw.githubusercontent.com/tallguy-h/LibreLine/main/docs/maps/germany.md): 40 posti (5 Paesi), 110 rotte con colori, 62 biglietti (33 corti, 29 lunghi).
- **Verifica delle rotte:** una ventina, confrontate con l'immagine ufficiale del tabellone (pagina prodotto Days of Wonder), senza discrepanze.
- **Verifica dei biglietti:** tutti i 62 valori coincidono con il percorso minimo sulla mappa, contando i Paesi come vicoli ciechi.
- **Coordinate:** lat/lon reali delle città. Per i Paesi un punto di confine rappresentativo, usato solo per il disegno.

## Biglietti 2017

- **File BGG «List of all tickets from ZuZ and 1902»** (`Tickets.xlsx`, https://boardgamegeek.com/filepage/126718): i 91 biglietti di Zug um Zug Deutschland (62) e dell'espansione 1902 (29).
- **Lista dei biglietti della scatola 2017** trascritta da un proprietario: thread BGG https://boardgamegeek.com/thread/1843369.
- **Incrocio delle due fonti:** 2017 = i 91 meno **Köln–Stuttgart 6** e **Dänemark–Regensburg 20**. Fa 55 corti e 34 lunghi, come dichiara il regolamento, e coincide con la lista del thread.
- **Particolarità:**
  - Mannheim–Stuttgart vale 2, anche se il regolamento dice che i corti vanno da 3 a 11;
  - Kiel–Stuttgart vale 17 con un percorso minimo di 16.

  In entrambi i casi si usano i valori stampati.

## Passeggeri

- **Quanti per città:** dallo schema ufficiale di preparazione (https://ultraboardgames.com/ticket-to-ride/gfx/germany2.jpg). Berlino 5; Hamburg, Köln, Mainz e München 4; Leipzig e Stuttgart 3; tutti gli altri posti, Paesi compresi, 1. Totale 60.
- **Colori:** 6 colori da 10 (rosso, nero, verde, giallo, blu, bianco). Viene dalla descrizione su BGG e dal comunicato della versione digitale Marmalade; non ho trovato una foto dei componenti fisici. Una wiki di fan dice 5×12, ma contraddice le altre fonti.
- **Zero passeggeri di un colore:** non danno punti, nemmeno il secondo posto (testo del regolamento, «for the Passengers they collected»; thread BGG 2422508). Non esiste una risposta ufficiale.

## Layout

Bbox geografica con un'ancora debole verso la geografia reale. Per i triangoli degeneri, cioè rotte lunghe quanto la somma delle due corte, e per l'hub di Berlino servono bersagli per coppia (`linkScale`) e uno spostamento di Schwerin: vedi `MAP_TUNING.germany` in `client/tools/gen-schematic-layout.mjs`.
