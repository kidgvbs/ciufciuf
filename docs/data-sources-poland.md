# Fonti dati Poland — CiufCiuf!

Ruleset `poland`, mappa `data/maps/poland.json`.

## Regolamento

**Ticket to Ride Map Collection 6½: Poland**, EN: https://cdn.svc.asmodee.net/production-asmodeeca/uploads/2023/07/tt_mc6-5_rules_EN.pdf. Lo stesso testo è nel regolamento PL/EN di Rebel (https://files.rebel.pl/files/instrukcje/Rulebook_T2R_Poland.pdf).

- 35 treni, 2–4 giocatori; 4 biglietti iniziali, tieni ≥2, scarti in fondo; pesca 3, tieni ≥1.
- Doppie: con 2–3 giocatori un solo binario. Le rotte verso i Paesi hanno tutti i binari aperti con qualunque numero di giocatori, uno per giocatore.
- Carte Paese:
  - reclamata una rotta verso un Paese, se la rete che la contiene raggiunge almeno due Paesi, si prende la carta in cima alla pila di ogni Paese della rete;
  - ci sono due esempi nel regolamento, il primo è riprodotto in un test (`crates/ttr-core/tests/germany_poland.rs`).
- Fine partita: nessun Globetrotter e nessun percorso più lungo.
- Spareggio: biglietti completati, poi punti dei biglietti completati.
- Punti per lunghezza 1–5: 1/2/4/7/10, letti dalla tabella stampata sul tabellone. Non esistono rotte da 6.

## Mappa e biglietti

| Fonte | Contenuto |
|---|---|
| Wiki di fan, `Poland:_Cities_and_Routes` (via https://ticket-to-ride.fandom.com/api.php) | 28 città, 7 Paesi, 68 rotte con colori dei binari |
| Wiki di fan, `List_of_Poland_Destination_Tickets` (via `api.php`) | 35 biglietti |
| Foto del tabellone vuoto a 2560 px: https://gryfgra.pl/wp-content/uploads/2019/10/img_7698.jpg | verifica di città, rotte e lunghezze |
| Foto del tabellone con le pile di carte Paese: https://pionkizkieszonki.blogspot.com/2019/08/wsiasc-do-pociagu-polska.html | valori delle carte Paese |
| Foto di carte biglietto (Rebel, recensioni) | 17 biglietti su 35, tutti uguali alla wiki |

- **Una correzione rispetto alla wiki:** Zakopane–Rzeszów è da **4**, non 5. Ho contato i vagoni sulla foto: Gdańsk–Płock resta l'unica rotta da 5, come conferma anche una recensione.
- **Altre letture sulla foto:**
  - Gorzów–Niemcy è da 1, con 3 binari;
  - Olsztyn–Rosja è da 2, con 3 binari;
  - Ucraina, 3 binari: blu, giallo, arancio;
  - Kalisz–Płock 3, Płock–Łódź 2.
- **Verifica:** tutti i 35 biglietti valgono quanto il percorso minimo sulla mappa.
- **Rotte verso i Paesi** (binari × lunghezza):
  - Niemcy 3×1, Rosja 3×2, Litwa 1×1, Białoruś 3×2;
  - Ukraina 3×2, Słowacja 3×1, Czechy 4×3.
- **Carte Paese** (dall'alto in basso), 20 in tutto:
  - Niemcy 7/4/3, Rosja 7/4/3, Czechy 10/7/4/2;
  - Białoruś 6/4/2, Ukraina 6/4/2, Słowacja 6/4/2, Litwa 3.
- **Ancora non verificati:** 18 valori di biglietti hanno la sola wiki come fonte.
- **Coordinate:** lat/lon reali delle città, e un punto rappresentativo per i Paesi.

## Layout

Il layout parte dalle posizioni sulla foto del tabellone (`MAP_BOUNDS.poland.image` in `client/tools/gen-schematic-layout.mjs`): sul tabellone i Paesi sono bordi, non punti geografici.
