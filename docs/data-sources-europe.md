# Fonti dati Europe — CiufCiuf!

## Regolamento

- **DO7202N, edizione 2026** (Days of Wonder):
  https://cdn.svc.asmodee.net/production-daysofwonder/uploads/2026/09/DO7202N_TICKET2RIDE_EUROPE_RULES_EN_20260512_compressed.pdf
  Riferimento per tunnel, traghetti, stazioni, punteggio e spareggio (D29).
- Edizione 2015 (`te_rules_2015_en.pdf`) solo per confronto: differisce nello spareggio (aveva "meno stazioni usate").

## Mappa e biglietti

| Fonte | Contenuto | Uso |
|---|---|---|
| tallguy-h/LibreLine `docs/maps/europe.md` | 101 rotte (colore, tunnel, traghetti), 46 biglietti con i 6 lunghi marcati | Base dei dati |
| brts-u/TicketToRide `static/europe/` | rotte (manca Paris–Zürich) e biglietti | Verifica |
| 11samype/TicketToRideEurope `trainroutes.txt`, `destinations.txt` | rotte e biglietti | Verifica |
| leonsi7/ticket-to-ride-europe `routes.csv`, `destinations.csv` | coppie di città senza colori né doppie | Verifica |

Coordinate: lat/lon reali delle città (le coordinate di Sofia in leonsi7 sono errate).
Nomi: quelli del tabellone 2026 (Kyïv, Zágráb, Athína, København, Zürich, München, Cádiz, Warszawa, Danzig).

## Discrepanze risolte (a maggioranza)

- Sochi–Erzurum: tunnel rosso 3 (samype lo dà come rotta normale).
- Rostov–Sevastopol: grigia 4 senza tunnel (samype la dà come tunnel).
- Stockholm–Wien: 11 punti (leonsi7 dà 10).
- Palermo–Smyrna: traghetto 6 con 2 locomotive (leonsi7 la colloca tra Palermo e Athína); confermato dall'esempio del regolamento 2015.

## Statistiche

47 città, 101 rotte (11 coppie doppie + 79 singole), 18 tunnel, 13 traghetti, 8 rotte per ognuno degli 8 colori e 37 grigie;
lunghezze 1/2/3/4/6/8 (nessuna da 5); 46 biglietti (6 `long` + 40 `regular`).
`ttr-validate`: numero ciclomatico stimato 12 (massimo 14).
