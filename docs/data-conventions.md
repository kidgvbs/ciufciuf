# Convenzioni dati — CiufCiuf!

## Identificatori (id)

| Tipo | Schema | Esempio |
|---|---|---|
| **CityId** | lowercase snake case | `new_york`, `salt_lake_city`, `sault_st_marie` |
| **RouteId** | `{cityA}_{cityB}_{n}`, città in ordine lessicografico, `_1` singola / `_1`…`_n` binari paralleli (2–4) | `denver_phoenix_1`, `seattle_vancouver_2`, `czechy_wroclaw_4` |
| **TicketId** | `{cityA}_{cityB}`, città in ordine lessicografico; multi-destinazione `{from}_{to1}_{to2}…` (destinazioni in ordine lessicografico) | `los_angeles_new_york`, `frankreich_deutschland_italien` |

**Regola deterministica**: mai iterare `HashMap` in percorsi che influenzano stato o serializzazione → `BTreeMap` o ordinamenti espliciti (piano §3.3, R3).

## Colori

- **Carte treno**: `Red`, `Orange`, `Yellow`, `Green`, `Blue`, `Purple`, `Black`, `White`, `Locomotive`
- **Giocatori**: `Pink`, `Cyan`, `Lime`, `Brown`, `Lavender`, `Indigo` (dal 18/11/2026, D41; nel protocollo maiuscoli: `PINK`, `CYAN`, …)
- **Rotte grigie** (colore qualunque ma uniforme): `"Gray"` nei dati mappa

## Rotte parallele e Paesi

- Un gruppo parallelo ha 2–4 binari (`double: true`, stessi estremi, id `_1`…`_n`); le lunghezze dei binari possono differire (Nordic: Mo i Rana–Trondheim 6 traghetto + 5 tunnel), ogni binario ha i propri punti e costo. Un giocatore non ne prende mai due; con meno di `doubleRouteMinPlayers` giocatori, preso un binario gli altri sono bloccati.
- `alwaysOpen: true` (Poland, rotte verso i Paesi): tutti i binari del gruppo sono aperti con qualunque numero di giocatori, uno per giocatore.
- `country: true` su una città: Paese confinante, vicolo cieco. Due rotte che entrano nello stesso Paese **non** sono collegate tra loro (Freiburg–Frankreich e Saarbrücken–Frankreich non uniscono Freiburg e Saarbrücken). Un biglietto con estremo Paese è completato da una qualunque rotta del giocatore che vi entra, collegata all'altro estremo.
- `flag` (solo sulle città `country: true`, codice ISO 3166-1 alpha-2 maiuscolo, es. `"CH"`): bandiera nazionale disegnata sulla mappa; è solo presentazione, il motore la ignora. `ttr-validate` dà errore se è su una città non Paese o non è `^[A-Z]{2}$`, avviso se un Paese ne è privo.
- Più rotte sulla stessa coppia con un estremo Paese possono essere tutte `double: false`: ognuna è un varco di confine a sé (il motore le dà un nodo privato) e lo stesso giocatore può prenderle tutte (Switzerland: Kreuzlingen–Deutschland, `_1` bianca a est e `_2` arancione a ovest). Un mix di `double` e non `double` sulla stessa coppia, o più di 4 rotte, è un errore di `ttr-validate`.

Disegno: i binari di un gruppo stanno a `k · passo` dalla mezzeria, con `k = −(n−1)/2 … (n−1)/2` e passo = spessore del tratto + gap (`doubleGap` del layout); con 2 binari è il vecchio ±(metà tratto + metà gap). I Paesi sono targhette quadrate invece del pallino.

## Nomi città

Nomi reali in inglese (es. "New York", non "Nuova York"). I file dati hanno sia `id` (machine-friendly) che `name` (display).

## Deck biglietti

- `"regular"` — mazzo standard
- `"long"` — mazzo biglietti lunghi (Europe)
- Il ruleset dichiara in `ticketDecks` quali mazzi esistono; ogni biglietto ha un campo `deck`
- L'ordine di `ticketDecks` è parte del replay: i mazzi vengono mescolati in quell'ordine, quindi riordinarlo o aggiungere un mazzo a un ruleset esistente invalida i suoi save (vedi `RULES_VERSION` in `ttr-core`).
- Ogni mazzo compare una sola volta in `ticketDecks`; `initialTickets.dealt[].deck` e `ticketDraw.deck` devono essere elencati in `ticketDecks`, e un mazzo elencato in `dealt` deve contenere almeno `giocatori × count` biglietti.
- `initialTickets.keepMin` non supera il totale dei biglietti distribuiti e `ticketDraw.keepMin` non supera `ticketDraw.count`; `ticketDraw.count` è almeno 1.
- Con `setupDiscards: "reshuffleBottom"` tornano in fondo alla pila di pesca solo gli scarti del setup appartenenti a `ticketDraw.deck`; gli scarti degli altri mazzi escono dalla partita. Con `"reshuffleBottomOwnDeck"` gli scarti di ogni mazzo sono mescolati e tornano in fondo al proprio mazzo.
- `initialTickets.dealt[].keepMax` (opzionale) limita i biglietti di quel mazzo tenibili nel draft iniziale (Europe Mega: al massimo 1 lungo); `keepMax <= count` e `keepMin` deve restare raggiungibile.
- `initialTickets.dealt[].keepMin` (opzionale, default 0) è il minimo di biglietti di quel mazzo da tenere nel draft iniziale (Germany: almeno 1 corto); il server lo espone ai client in `ticket_choice.limits[].min` accanto a `max`, e `keepMin` globale resta il minimo totale. `keepMin <= count`, `keepMin <= keepMax` e la somma dei `keepMin` dei mazzi non supera `initialTickets.keepMin`.
- **Mazzo di pesca unito** (Germany, regola della casa): `ticketDraw: { "deck": "regular", "mergeDecks": ["long"], "count": 4, "keepMin": 1 }`. Finito il draft iniziale (scarti del setup già gestiti da `setupDiscards`, con `reshuffleBottomOwnDeck`) i biglietti rimasti di `deck` e dei mazzi di `mergeDecks` formano **una sola pila**, mescolata con l'RNG del motore (prima `deck`, poi `mergeDecks` nell'ordine indicato); `DRAW_TICKETS` pesca da quella pila e gli scartati a metà partita, di qualunque mazzo originale, tornano in fondo a essa. `PublicState.ticket_deck_size` conta i biglietti pescabili. La vecchia pesca con scelta del mazzo (`chooseFrom`, quote `counts`) non esiste più.
- **Scarti a metà partita** (`ticketDraw.discards`, default `"bottom"`): `bottom` rimette gli scartati in fondo al proprio mazzo (o alla pila unita) nell'ordine di offerta, come sempre; `"removed"` (Nordic, Switzerland) li toglie dalla partita, quindi il mazzo può finire (`PublicState.ticket_deck_size` scende di quanti biglietti escono dal mazzo). Con meno di `count` biglietti nel mazzo si pescano quelli rimasti (purché almeno `keepMin`, e comunque uno); a mazzo vuoto `DRAW_TICKETS` è rifiutato e `can_draw_tickets` è `false`. È ammesso anche con `mergeDecks`. Gli scarti del setup restano governati da `setupDiscards`.
- **Biglietti multi-destinazione** (Switzerland): un biglietto ha `from` più UNA tra due forme: `to` + `points` (classico) oppure `destinations: [{ "to": …, "points": … }, …]` (almeno 2 voci, `to` distinti e diversi da `from`). Le città possono essere Paesi (`country: true`), anche la partenza. Il biglietto è **completato** se almeno una destinazione è collegata alla partenza; vale il punteggio **più alto** tra le collegate, e se nessuna lo è si perde il valore **più basso**; conta come un solo biglietto completato (spareggi, globetrotter). Con più destinazioni collegate a pari punti vale la prima dell'elenco. Il TicketId è `{from}_{to1}_{to2}…` con le destinazioni in ordine lessicografico (suffisso `_2` se due biglietti producono lo stesso id). Il server espone a client ogni biglietto con `TicketInfo.destinations` (città e punti di ciascuna; `points` è il valore più alto) e, a fine partita, `FinalScoreInfo.tickets` con la destinazione realizzata e i punti guadagnati o persi di ogni biglietto. Nei file mappa il biglietto classico resta `{from, to, points, …}`; i test e il client leggono entrambe le forme.

## Modalità e set di biglietti

- Ogni ruleset è una modalità di una mappa: `name` (nome della mappa), `mode` (nome della modalità, non vuoto), `modeOrder` (ordine nel selettore; 0 = Standard).
- Ogni biglietto ha `sets` (non vuoto, senza duplicati): le modalità in cui è in gioco. Il ruleset sceglie il proprio con `ticketSet`; i biglietti in gioco sono quelli il cui `sets` lo contiene, nell'ordine del file, e il set non può essere vuoto.
- Un biglietto con valori diversi tra modalità è un biglietto distinto (i 4 rivisti del 1910 hanno id `<a>_<b>_1910`).
- I biglietti nuovi si aggiungono in coda: l'ordine dei biglietti in gioco è parte del replay dei ruleset esistenti.
- `bigCities` (opzionale) elenca le città da evidenziare sulla mappa nelle modalità Grandi Città; solo visualizzazione, città esistenti e senza duplicati.
- `specialRules` (opzionale, default vuoto): elenco di testi in italiano con le regole che differiscono dal gioco base (Germany nelle due modalità, Poland), mostrati ai giocatori a inizio partita. Voci non vuote (altrimenti il ruleset non è valido, `EmptySpecialRule`); sono solo testo, quindi non entrano nel replay né nei salvataggi e cambiarli non richiede `RULES_VERSION`. Il server li espone in `GET /api/rulesets` come `specialRules` (`[]` se il ruleset non ne ha); il client non conosce le regole, mostra solo quel che arriva. Vanno tenuti allineati a mano al comportamento del motore.

## Bonus

- Tipi ammessi in `bonuses`: `"longest_path"` (percorso più lungo, parità a tutti) e `"globetrotter"` (più biglietti completati, parità a tutti, nessuno se il massimo è 0). Un tipo sconosciuto o ripetuto rende il ruleset non valido; la lista può essere vuota (USA Grandi Città, Poland). Senza `longest_path` il percorso più lungo non viene calcolato, salvo che sia richiesto dallo spareggio `longestPath`.
- `countryCards` (opzionale, Poland): per ogni Paese i valori delle carte dall'alto in basso (pile non mescolate). Quando la rete di un giocatore che contiene la rotta appena reclamata raggiunge almeno due Paesi e ne include uno che non era in una rete già premiata prima della presa (rete premiata: almeno due Paesi), prende la carta in cima alla pila di **ogni** Paese della rete; pile vuote non danno nulla. Unire due reti già premiate, o aggiungere una seconda rotta verso un Paese già in rete, non dà carte; l'esito non dipende dall'ordine delle prese (D38). A fine partita la somma va nel totale (`countryPoints`).
- `passengers` (opzionale, Germany 2017): `colors`, `perColor`, `placement` (passeggeri per città, Paesi compresi) e `majorityPoints` `[primo, secondo]`. Il sacchetto è mescolato dopo tutto il consumo RNG preesistente e distribuito sulle città in ordine di id. A ogni rotta presa il giocatore prende un passeggero per estremo (sceglie il colore solo dove ce n'è più di uno: `CLAIM_ROUTE.passengers`). A fine partita, per colore, conta solo chi ne ha almeno uno: il massimo prende il primo valore (pari merito: tutti il primo, nessuno il secondo), altrimenti chi ha il secondo valore più alto prende il secondo (anche a pari merito). Totale in `passengerPoints`.
- `tieBreakers` (opzionale): spareggi dopo il totale, in ordine. Chiavi: `completedTickets`, `completedTicketPoints`, `longestPathBonus` (bonus del percorso più lungo ricevuto), `longestPath` (**lunghezza** del percorso più lungo del giocatore, calcolata a fine partita anche senza il bonus `longest_path`: Nordic), `passengers` (passeggeri raccolti). Default `["completedTickets", "longestPathBonus"]` (comportamento storico). Pari merito dopo tutte le chiavi: vincono tutti. Con `longestPath` il controllo di densità della mappa (sotto) si applica anche senza il bonus; senza bonus nessuno prende punti per il percorso e la partita non evidenzia alcun percorso.

## Locomotive, traghetti e pagamento delle rotte

Tutti i campi sono opzionali: senza di essi vale il comportamento classico (locomotive jolly su ogni rotta, pagamento di esattamente `len` carte, pesca con le regole base). I due ruleset che li usano sono Nordic Countries e Switzerland; il motore non conosce le mappe, legge solo questi dati.

- `locomotives.usableOn` (array non vuoto, senza duplicati, valori `"tunnel"` e `"ferry"`): tipi di rotta su cui **una locomotiva può pagare da sola uno spazio colore**. Assente = tutte le rotte. `"tunnel"` vale per le rotte con `tunnel: true`, `"ferry"` per quelle con `ferries > 0`. Sugli spazi locomotiva del traghetto la locomotiva è sempre ammessa; dentro un gruppo di carte qualsiasi (sotto) è sempre ammessa. Vale anche per il costo extra dei tunnel. Nordic `["tunnel", "ferry"]`, Switzerland `["tunnel"]`. Con la restrizione attiva niente sostituzione «carta mancante → locomotiva» sulle rotte dove la locomotiva è vietata.
- `locomotives.freeDraw` (bool, default `false`): la locomotiva scoperta conta come una carta normale: non chiude il turno quando è la prima pescata e si può prendere come seconda carta. Con `false` resta la regola classica (una locomotiva scoperta vale l'intero turno e non si prende come seconda). Le pescate dal mazzo coperto sono sempre normali.
- `locomotives.faceUpReset` (bool, default `true`): con `false` il mercato non si scarta e rimpiazza quando restano 3 locomotive scoperte (Nordic); non viene controllato né a inizio partita né dopo le pescate. Switzerland lo lascia a `true`.
- `ferries.anyCardsPerLocomotive` (intero ≥ 2): sulle rotte con `ferries > 0`, questo numero di carte qualsiasi (locomotive comprese) vale **una locomotiva**: per ogni simbolo locomotiva serve 1 locomotiva oppure un gruppo, e per ogni altro spazio una carta del colore, 1 locomotiva (se ammessa) oppure un gruppo. Assente = nessuna sostituzione. Nordic: 3.
- `routes[].anyCardsPerSpace` (intero ≥ 2): ogni spazio della rotta si paga anche con quel numero di carte qualsiasi (locomotive comprese) al posto della carta del colore, ripetutamente (Nordic: Murmansk–Lieksa, lunghezza 9, grigia, 4 carte: 7 verdi + 8 qualsiasi).

**Regola di pagamento** (`CLAIM_ROUTE.cards_discarded`, funzione pura in `ttr-core/src/payment.rs`, usata anche da `legal.rs` per le rotte reclamabili): con le sostituzioni la lista può essere più lunga di `len`. Il pagamento è valido se e solo se **tutte** le carte dichiarate si assegnano esattamente agli spazi, senza carte avanzate: spazio colore ← 1 carta del colore (sulle grigie un solo colore per tutte le carte singole colorate), 1 locomotiva (se ammessa) o un gruppo; spazio locomotiva ← 1 locomotiva o un gruppo. Senza sostituzioni le carte sono esattamente `len`, come oggi, con lo stesso errore `InsufficientCards` per le locomotive del traghetto mancanti. Il costo extra dei tunnel non cambia. Una rotta `tunnel: true` non può avere `ferries > 0` né `anyCardsPerSpace` (il colore del tunnel si ricava dalla prima carta non locomotiva pagata, che con i gruppi potrebbe appartenere a un gruppo). Il catalogo `GET /api/rulesets` espone `locomotivesUsableOn` e `ferryAnyCards` (omessi quando assenti); `anyCardsPerSpace` arriva al client nelle rotte della mappa. Le carte scoperte pescabili (anche con `freeDraw`) arrivano dalle mosse legali (`legal_moves.draw_face_up`), non dal catalogo. Il client usa il catalogo solo per proporre pagamenti sensati, l'arbitro è il server.

## Validazione

`Ruleset::validate(&map)` in `ttr-core` controlla i vincoli di questo documento ed è eseguita da `Engine::new`, da `ttr-validate` e all'avvio del server. I colori delle rotte accettati sono solo quelli canonici elencati sopra (`Gray` e i nomi dei colori carta in PascalCase).

Vincoli su mappa e ruleset, ognuno con una variante dedicata di `EngineInitError`:

- id unici per città, rotte e biglietti;
- le città di rotte e biglietti esistono; una rotta e un biglietto hanno `from != to`;
- ogni rotta ha `len >= 1` e la sua lunghezza compare in `routePoints`;
- `trainsPerPlayer >= 1`;
- il ruleset ha un `name` non vuoto (nome mostrato nel selettore mappe, servito da `GET /api/rulesets`);
- una rotta con `double: true` appartiene a un gruppo di 2–4 binari paralleli: id con suffissi `_1`…`_n` contigui, stessi estremi, `alwaysOpen` uguale su tutti i binari; la lunghezza può differire tra i binari (Nordic: traghetto e tunnel sulla stessa coppia);
- `countryCards` ha come chiavi solo Paesi (`country: true`); `passengers` ha colori non vuoti e unici, `perColor ≥ 1`, `placement` su città esistenti con conteggi ≥ 1 e somma `colori × perColor`, `majorityPoints` di 2 valori;
- `ticketDraw.mergeDecks` con mazzi elencati in `ticketDecks`, senza ripetizioni e diversi da `ticketDraw.deck` (e `setupDiscards` diverso da `reshuffleBottom`); `keepMin` di ogni voce di `dealt` non oltre `count` né oltre `keepMax`, con somma non oltre `initialTickets.keepMin`; `tieBreakers` senza chiavi sconosciute o ripetute;
- ogni biglietto ha UNA sola forma: `to` + `points` oppure `destinations` (almeno 2 voci, `to` distinti, diversi da `from`, città esistenti): `TicketFormsBoth`, `TicketFormMissing`, `TicketDestinationsTooFew`, `TicketDestinationDuplicate` (più `TicketSameCity` e `TicketCityUnknown` per ogni destinazione); `ttr-validate` controlla che ogni destinazione sia raggiungibile dalla partenza;
- `ticketDraw.discards` ammette solo `bottom` e `removed` (anche con `mergeDecks`);
- `locomotives.usableOn` non vuoto, senza duplicati e solo con `tunnel`/`ferry`; `ferries.anyCardsPerLocomotive ≥ 2`; ogni `anyCardsPerSpace` ≥ 2;
- una rotta `tunnel: true` non ha `ferries > 0` (`RouteTunnelAndFerry`) né `anyCardsPerSpace` (`RouteTunnelAnyCardsPerSpace`);
- il mazzo treni ha `perColor × 8 >= 3 × locomotives`;
- `Engine::new` richiede inoltre id e colori dei giocatori distinti.

## Longest path e densità della mappa

Il percorso più lungo (un trail: nessuna rotta riusata, città ripetibili) è calcolato con un branch and bound sulle rotte che il trail lascia fuori: i componenti ad albero usano il diametro pesato, gli altri una ricerca esatta entro `MAX_SEARCH_NODES` nodi (in `scoring.rs`). Se il budget si esaurisce il risultato è solo un limite inferiore: `longest_path_report` lo segnala con `exact: false` e `Engine::longest_path_inexact_players` elenca i giocatori coinvolti a fine partita.

`Ruleset::validate` stima il numero ciclomatico massimo (rotte − città + 1) di una componente che un giocatore può costruire con `trainsPerPlayer` treni e rifiuta la mappa se la stima supera `MAX_CYCLOMATIC_NUMBER` (14, in `scoring.rs`). La stima è un limite superiore calcolato per ogni componente connessa della mappa (`density.rs`): per ogni moltiplicatore `a/b` il limite di treni diventa un costo `a/b` per ogni unità di lunghezza, e il sottografo più denso a quel prezzo è una chiusura di peso massimo risolta con un min-cut; vale il minimo sui moltiplicatori provati. Non assume che la mappa sia planare. Un giocatore prende al massimo un binario per gruppo parallelo, quindi la stima conta ogni gruppo `double` (stessi estremi e stessa base dell'id) come **una sola** rotta, di lunghezza pari al suo binario più corto (limite superiore valido anche con binari di lunghezza diversa). Stime misurate con `ttr-validate`: USA 9, Europe 10, Basilicata 3, Switzerland 11, Nordic 9, Germany 11. Il controllo si applica con il bonus `longest_path` o con lo spareggio `longestPath`.
