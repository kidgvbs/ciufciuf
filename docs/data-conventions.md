# Convenzioni dati — CiufCiuf!

## Identificatori (id)

| Tipo | Schema | Esempio |
|---|---|---|
| **CityId** | lowercase snake case | `new_york`, `salt_lake_city`, `sault_st_marie` |
| **RouteId** | `{cityA}_{cityB}_{n}`, città in ordine lessicografico, `_1` singola / `_1` `_2` doppia | `denver_phoenix_1`, `seattle_vancouver_2` |
| **TicketId** | `{cityA}_{cityB}`, città in ordine lessicografico | `los_angeles_new_york` |

**Regola deterministica**: mai iterare `HashMap` in percorsi che influenzano stato o serializzazione → `BTreeMap` o ordinamenti espliciti (piano §3.3, R3).

## Colori

- **Carte treno**: `Red`, `Orange`, `Yellow`, `Green`, `Blue`, `Purple`, `Black`, `White`, `Locomotive`
- **Giocatori**: `Red`, `Blue`, `Green`, `Yellow`, `Black`
- **Rotte grigie** (colore qualunque ma uniforme): `"Gray"` nei dati mappa

## Rotte doppie — rappresentazione geometrica

Due segmenti tra le stesse città si sovrapporrebbero. Il renderer applica un **offset perpendicolare ±d** lungo la normale:
- `n = (−Δy, Δx) / |Δ|` dove `Δ = (lon_B − lon_A, lat_B − lat_A)`
- Per la rotta `_1`: offset `+d`; per la `_2`: offset `−d`
- `d` è calcolato in base allo zoom (es. ~0.15° a zoom default)

**Bézier opzionale**: i dati mappa possono includere un punto di controllo (`controlPoint`) dove l'offset lineare non basta (es. rotte molto curve).

## Nomi città

Nomi reali in inglese (es. "New York", non "Nuova York"). I file dati hanno sia `id` (machine-friendly) che `name` (display).

## Deck biglietti

- `"regular"` — mazzo standard
- `"long"` — mazzo biglietti lunghi (Europe)
- Il ruleset dichiara in `ticketDecks` quali mazzi esistono; ogni biglietto ha un campo `deck`