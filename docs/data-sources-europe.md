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

## Modalità Europa 1912 (12/11/2026)

Regolamenti: Europa 1912, ristampa 2015 (Estesa = 46 base + 19 regolari nuovi) ed Europe 15° anniversario 2021 (1912 = solo carte 1912; Mega: 2 lunghi di cui al massimo 1 + 5 altri, tieni 3 in totale, scarti regolari in fondo; Grandi Città: 45 biglietti, 5 tieni 2, pesca 4 tieni 1, scarti in fondo). Depositi e magazzini non implementati (assenti nell'app 2024). Estesa e 1912 sono entrambe presenti (D35).

Biglietti (`sets` in `data/maps/europe.json`): 55 nuovi (6 lunghi, 19 regolari, 30 grandi città) e i 15 base con il logo Grandi Città. Fonte primaria: BGG file 49406 «Ticket To Ride 1912 list of destination tickets» (`Ticket_To_Ride_1912_card_reference.xls`, Andrew Yao 2009), che elenca tutti i 101 biglietti con valore, lungo/1912/Grandi Città. Concorda con i 46 biglietti base esistenti e con tutte le 47 voci ricavate indipendentemente da foto delle carte su BGG, dalle tabelle Praxeo (A. Beuve, 2010) e dai thread BGG 62863/452730 (media Grandi Città 11,07 → somma dei 30 nuovi 369). Grandi città: London, Berlin, Moskva, Paris, Wien, Madrid, Roma, Athína, Angora.
