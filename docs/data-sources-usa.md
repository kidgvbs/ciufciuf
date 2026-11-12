# Fonti dati USA — CiufCiuf!

## Fonti primarie

### 1. Rob217/TicketToRideAnalysis
- URL: https://github.com/Rob217/TicketToRideAnalysis
- File: city_locations.json, routes.csv, tickets.csv
- Uso: coordinate città, grafo rotte, 30 biglietti classici
- Note: coordinate convertite da SVG a lat/lon reali

### 2. geckods/ticket-to-ride (Go)
- URL: https://github.com/geckods/ticket-to-ride
- File: gameconstants.go
- Uso: verifica incrociata rotte e biglietti

### 3. PDF regolamento V2 2025 (Days of Wonder)
- URL: https://cdn.svc.asmodee.net/production-daysofwonder/uploads/2025/07/7201N_TICKET2RIDEV2_RULES_EN_20250425_WEB.pdf
- Uso: 3 nuovi biglietti V2, regole setup, composizione mazzo

### 4. BoardGameGeek / Fandom wiki
- Uso: verifica terza biglietti classici e punti

## Discrepanze risolte
- Pittsburgh–Saint Louis: Rob217 (single green 5) preferito a geckods (doppia)
- Nashville–Raleigh: Rob217 (single black 3) preferito a geckods (doppia)

## Errori noti della fonte `geckods/ticket-to-ride`

I seguenti errori sono documentati per provenienza; **non** sono stati importati nei
nostri dati, che usano invece i valori corretti:

- **Atlanta–Miami**: il nostro dato è Blue 5 (geckods riporta 6 — con 6 vagoni blu
  si sforerebbero i 27 disponibili).
- **Atlanta–Raleigh** e **Nashville–Pittsburgh**: geckods le omette del tutto;
  entrambe sono presenti e corrette nei nostri dati (single Black 3 e single Gray 4
  rispettivamente).

## Statistiche
- 36 città, 100 segmenti (22 doppie + 56 singole), 33 biglietti (30 classici + 3 V2)
## Modalità USA 1910 (12/11/2026)

Regolamento: USA 1910, ristampa 2018 (`7216-T2R1910-rulesbook_ML.pdf`, cdn.svc.asmodee.net). Modalità 1910 (biglietti 1910, Globetrotter +15 al posto del percorso più lungo, setup standard V2: D37), Grandi Città (biglietti con almeno una tra Chicago, Dallas, Houston, Los Angeles, Miami, New York, Seattle; 4 tieni 2, pesca 4 tieni 1, nessun bonus), Mega (tutti i 69; 5 tieni 3, pesca 4 tieni 1, entrambi i bonus).

Biglietti (`sets` in `data/maps/usa.json`): 35 «1910», 4 Mystery Train, 4 classici con valore ridotto nella ristampa (id `<a>_<b>_1910`: Los Angeles–Miami 19, Los Angeles–New York 20, New York–Seattle 20, Oklahoma City–Sault St. Marie 8; la modalità Standard tiene i valori originali). Ogni biglietto è confermato da tre fonti indipendenti che concordano su coppie e valori:
- Super Cheats, elenchi dell'edizione digitale Xbox/PC (`supercheats.com/ticket-to-ride/wiki/1910-map-master-routes-list`);
- `github.com/fernandomsilva/Ticket-to-Ride-Engine` (`gameContent/usa1910_destinations.txt`, `usa_megagame_destinations.txt`, `usa_bigcities_destinations.txt`);
- `github.com/crb0229/ttr-scorer` (`usa1910_tickets.json`, letto dalle carte).

Controlli: l'istogramma dei 69 valori pubblicato su BGG (totale 754) coincide; Grandi Città = 35 (15 classici + 19 «1910» + Chicago–Montreal Mystery Train), come nel regolamento. I 3 biglietti V2 (Atlanta–Washington, Omaha–New Orleans, San Francisco–Washington) fanno parte dei 35 «1910». Discrepanza risolta: Boston–Kansas City 11 (una sola fonte dice 7).
