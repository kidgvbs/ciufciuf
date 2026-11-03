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