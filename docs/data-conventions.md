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
- L'ordine di `ticketDecks` è parte del replay: i mazzi vengono mescolati in quell'ordine, quindi riordinarlo o aggiungere un mazzo a un ruleset esistente invalida i suoi save (vedi `RULES_VERSION` in `ttr-core`).
- Ogni mazzo compare una sola volta in `ticketDecks`; `initialTickets.dealt[].deck` e `ticketDraw.deck` devono essere elencati in `ticketDecks`, e un mazzo elencato in `dealt` deve contenere almeno `giocatori × count` biglietti.
- `initialTickets.keepMin` non supera il totale dei biglietti distribuiti e `ticketDraw.keepMin` non supera `ticketDraw.count`; `ticketDraw.count` è almeno 1.
- Con `setupDiscards: "reshuffleBottom"` tornano in fondo alla pila di pesca solo gli scarti del setup appartenenti a `ticketDraw.deck`; gli scarti degli altri mazzi escono dalla partita.
- `initialTickets.dealt[].keepMax` (opzionale) limita i biglietti di quel mazzo tenibili nel draft iniziale (Europe Mega: al massimo 1 lungo); `keepMax <= count` e `keepMin` deve restare raggiungibile.

## Modalità e set di biglietti

- Ogni ruleset è una modalità di una mappa: `name` (nome della mappa), `mode` (nome della modalità, non vuoto), `modeOrder` (ordine nel selettore; 0 = Standard).
- Ogni biglietto ha `sets` (non vuoto, senza duplicati): le modalità in cui è in gioco. Il ruleset sceglie il proprio con `ticketSet`; i biglietti in gioco sono quelli il cui `sets` lo contiene, nell'ordine del file, e il set non può essere vuoto.
- Un biglietto con valori diversi tra modalità è un biglietto distinto (i 4 rivisti del 1910 hanno id `<a>_<b>_1910`).
- I biglietti nuovi si aggiungono in coda: l'ordine dei biglietti in gioco è parte del replay dei ruleset esistenti.
- `bigCities` (opzionale) elenca le città da evidenziare sulla mappa nelle modalità Grandi Città; solo visualizzazione, città esistenti e senza duplicati.

## Bonus

- Tipi ammessi in `bonuses`: `"longest_path"` (percorso più lungo, parità a tutti) e `"globetrotter"` (più biglietti completati, parità a tutti, nessuno se il massimo è 0). Un tipo sconosciuto o ripetuto rende il ruleset non valido; la lista può essere vuota (USA Grandi Città). Senza `longest_path` il percorso più lungo non viene calcolato.

## Validazione

`Ruleset::validate(&map)` in `ttr-core` controlla i vincoli di questo documento ed è eseguita da `Engine::new`, da `ttr-validate` e all'avvio del server. I colori delle rotte accettati sono solo quelli canonici elencati sopra (`Gray` e i nomi dei colori carta in PascalCase).

Vincoli su mappa e ruleset, ognuno con una variante dedicata di `EngineInitError`:

- id unici per città, rotte e biglietti;
- le città di rotte e biglietti esistono; una rotta e un biglietto hanno `from != to`;
- ogni rotta ha `len >= 1` e la sua lunghezza compare in `routePoints`;
- `trainsPerPlayer >= 1`;
- il ruleset ha un `name` non vuoto (nome mostrato nel selettore mappe, servito da `GET /api/rulesets`);
- una rotta con `double: true` ha id con suffisso `_1`/`_2` e una gemella con `double: true`, stessi estremi e stessa lunghezza;
- il mazzo treni ha `perColor × 8 >= 3 × locomotives`;
- `Engine::new` richiede inoltre id e colori dei giocatori distinti.

## Longest path e densità della mappa

Il percorso più lungo (un trail: nessuna rotta riusata, città ripetibili) è calcolato con un branch and bound sulle rotte che il trail lascia fuori: i componenti ad albero usano il diametro pesato, gli altri una ricerca esatta entro `MAX_SEARCH_NODES` nodi (in `scoring.rs`). Se il budget si esaurisce il risultato è solo un limite inferiore: `longest_path_report` lo segnala con `exact: false` e `Engine::longest_path_inexact_players` elenca i giocatori coinvolti a fine partita.

`Ruleset::validate` stima il numero ciclomatico massimo (rotte − città + 1) di una componente che un giocatore può costruire con `trainsPerPlayer` treni e rifiuta la mappa se la stima supera `MAX_CYCLOMATIC_NUMBER` (14, in `scoring.rs`). La stima è un limite superiore calcolato per ogni componente connessa della mappa (`density.rs`): per ogni moltiplicatore `a/b` il limite di treni diventa un costo `a/b` per ogni unità di lunghezza, e il sottografo più denso a quel prezzo è una chiusura di peso massimo risolta con un min-cut; vale il minimo sui moltiplicatori provati. Non assume che la mappa sia planare. La mappa USA stima 13 (il massimo realmente costruibile è circa 11).
