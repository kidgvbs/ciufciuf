//! Algoritmi di scoring: Union-Find per giocatore, Longest Continuous Path.
//!
//! Piano §9.1 (Union-Find), §9.3 (Longest path DFS con backtracking).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Bound;

use crate::types::{
    CityId, ClaimedRouteInfo, Route, RouteId, TicketDef, TicketResult, points_to_score,
};

/// Nome del nodo di un estremo di rotta: la città stessa, oppure per un Paese il nodo
/// privato `paese#rotta` (i Paesi sono vicoli ciechi: due rotte che vi entrano non sono
/// collegate tra loro).
pub(crate) fn route_node(countries: &BTreeSet<CityId>, route_id: &str, city: &str) -> String {
    if countries.contains(city) {
        format!("{city}#{route_id}")
    } else {
        city.to_string()
    }
}

// ── Union-Find per giocatore ─────────────────────────────────────────

/// Union-Find (Disjoint Set Union) con path compression e union by rank.
///
/// Tiene traccia della connettività delle città per un singolo giocatore.
/// Aggiornamento incrementale O(α(n)) a ogni claim.
#[derive(Debug, Clone)]
pub struct UnionFind {
    /// Mappa city_id → parent (se stesso = radice).
    parent: BTreeMap<String, String>,
    /// Mappa city_id → rank.
    rank: BTreeMap<String, usize>,
}

impl UnionFind {
    /// Crea un nuovo Union-Find vuoto.
    pub fn new() -> Self {
        Self {
            parent: BTreeMap::new(),
            rank: BTreeMap::new(),
        }
    }

    /// Assicura che una città esista nell'insieme.
    fn ensure(&mut self, city: &str) {
        if !self.parent.contains_key(city) {
            self.parent.insert(city.to_string(), city.to_string());
            self.rank.insert(city.to_string(), 0);
        }
    }

    /// Trova il rappresentante (con path compression).
    pub fn find(&mut self, city: &str) -> String {
        self.ensure(city);
        let parent = self.parent[city].clone();
        if parent == city {
            return parent;
        }
        let root = self.find(&parent);
        self.parent.insert(city.to_string(), root.clone());
        root
    }

    /// Unisce due città (aggiunge una rotta tra di loro).
    pub fn union(&mut self, a: &str, b: &str) {
        self.ensure(a);
        self.ensure(b);
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        let rank_a = self.rank[&ra];
        let rank_b = self.rank[&rb];
        if rank_a < rank_b {
            self.parent.insert(ra, rb);
        } else if rank_a > rank_b {
            self.parent.insert(rb, ra);
        } else {
            self.parent.insert(rb, ra.clone());
            self.rank.insert(ra, rank_a + 1);
        }
    }

    /// Verifica se due città sono connesse.
    pub fn connected(&mut self, a: &str, b: &str) -> bool {
        self.ensure(a);
        self.ensure(b);
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return true;
        }
        // Un Paese è raggiunto da qualunque rotta che vi entra: i suoi nodi privati.
        let (a_nodes, b_nodes) = (self.country_nodes(a), self.country_nodes(b));
        if a_nodes.is_empty() && b_nodes.is_empty() {
            return false;
        }
        let a_roots = self.roots_of_nodes(a_nodes, ra);
        let b_roots = self.roots_of_nodes(b_nodes, rb);
        a_roots.iter().any(|root| b_roots.contains(root))
    }

    /// Nodi privati `{country}#{route}` presenti nell'insieme.
    fn country_nodes(&self, country: &str) -> Vec<String> {
        let prefix = format!("{country}#");
        self.parent
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
            .map(|(node, _)| node)
            .take_while(|node| node.starts_with(&prefix))
            .cloned()
            .collect()
    }

    /// Radici dei nodi dati; `fallback` (la radice del nome stesso) se non ce ne sono.
    fn roots_of_nodes(&mut self, nodes: Vec<String>, fallback: String) -> Vec<String> {
        if nodes.is_empty() {
            return vec![fallback];
        }
        nodes.iter().map(|node| self.find(node)).collect()
    }

    /// Paesi di `countries` con almeno una rotta nella componente di `anchor`.
    fn countries_connected_to(
        &mut self,
        anchor: &str,
        countries: &BTreeSet<CityId>,
    ) -> Vec<CityId> {
        let root = self.find(anchor);
        let mut reached = Vec::new();
        for country in countries {
            let nodes = self.country_nodes(country);
            if nodes.iter().any(|node| self.find(node) == root) {
                reached.push(country.clone());
            }
        }
        reached
    }

    /// Crea un clone profondo.
    pub fn deep_clone(&self) -> Self {
        Self {
            parent: self.parent.clone(),
            rank: self.rank.clone(),
        }
    }
}

impl Default for UnionFind {
    fn default() -> Self {
        Self::new()
    }
}

/// Union-Find manager per tutti i giocatori.
///
/// Aggiornamento incrementale a ogni claim.
#[derive(Debug, Clone)]
pub struct PlayerConnectivity {
    /// Un Union-Find per giocatore.
    ufs: Vec<UnionFind>,
    /// Paesi della mappa: vicoli ciechi, ogni rotta vi entra con un nodo privato.
    countries: BTreeSet<CityId>,
}

impl PlayerConnectivity {
    pub fn new(num_players: usize) -> Self {
        Self::with_countries(num_players, BTreeSet::new())
    }

    /// Connettività su una mappa con Paesi (`City::country`).
    pub fn with_countries(num_players: usize, countries: BTreeSet<CityId>) -> Self {
        Self {
            ufs: vec![UnionFind::new(); num_players],
            countries,
        }
    }

    /// Aggiunge una rotta reclamata alla connettività del giocatore. Un estremo Paese
    /// diventa il nodo privato `paese#rotta`: due rotte verso lo stesso Paese non sono
    /// collegate tra loro.
    pub fn add_route(&mut self, player_index: usize, route_id: &str, from: &str, to: &str) {
        let a = route_node(&self.countries, route_id, from);
        let b = route_node(&self.countries, route_id, to);
        if player_index < self.ufs.len() {
            self.ufs[player_index].union(&a, &b);
        }
    }

    /// Verifica se un biglietto è completato per un giocatore.
    pub fn ticket_completed(&mut self, player_index: usize, from: &str, to: &str) -> bool {
        if player_index >= self.ufs.len() {
            return false;
        }
        self.ufs[player_index].connected(from, to)
    }

    /// Clona una copia (per calcoli offline).
    pub fn clone_uf(&self, player_index: usize) -> Option<UnionFind> {
        self.ufs.get(player_index).map(|uf| uf.deep_clone())
    }

    /// Paesi della mappa.
    pub(crate) fn countries(&self) -> &BTreeSet<CityId> {
        &self.countries
    }

    /// Paesi (in ordine di id) raggiunti dalla rete del giocatore che contiene la rotta
    /// `route_id` appena aggiunta.
    pub(crate) fn countries_in_network(
        &mut self,
        player_index: usize,
        route_id: &str,
        from: &str,
    ) -> Vec<CityId> {
        let anchor = route_node(&self.countries, route_id, from);
        let Some(uf) = self.ufs.get_mut(player_index) else {
            return Vec::new();
        };
        uf.countries_connected_to(&anchor, &self.countries)
    }
}

impl PlayerConnectivity {
    /// Paesi (in ordine di id) raggiunti dalla rete del giocatore che contiene la città
    /// `city`; vuoto se `city` è un Paese (un Paese entra in rete solo con una rotta).
    pub(crate) fn countries_in_city_network(
        &mut self,
        player_index: usize,
        city: &str,
    ) -> Vec<CityId> {
        if self.countries.contains(city) {
            return Vec::new();
        }
        let Some(uf) = self.ufs.get_mut(player_index) else {
            return Vec::new();
        };
        uf.countries_connected_to(city, &self.countries)
    }

    /// Union-Find del giocatore, per valutare i biglietti.
    pub(crate) fn uf_mut(&mut self, player_index: usize) -> Option<&mut UnionFind> {
        self.ufs.get_mut(player_index)
    }
}

// ── Stazioni e biglietti (D32) ───────────────────────────────────────

/// Rotta avversaria incidente alla città di una stazione.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationOption {
    pub route_id: RouteId,
    /// L'estremo della rotta opposto alla città della stazione.
    pub other_city: CityId,
}

/// Una stazione con le rotte avversarie che può prendere in prestito, per id crescente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationCandidates {
    pub city: CityId,
    pub options: Vec<StationOption>,
}

/// Esito dei biglietti di un giocatore con la migliore assegnazione delle stazioni.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketEvaluation {
    /// Un esito per biglietto, nell'ordine di `tickets`.
    pub completed: Vec<bool>,
    /// Per ogni biglietto, l'indice in `TicketSpec::destinations` della destinazione
    /// realizzata (la collegata di punti più alti, la prima a parità); `None` se fallito.
    pub reached: Vec<Option<usize>>,
    /// Somma dei punti dei biglietti completati.
    pub completed_points: i32,
    /// Somma negata dei punti dei biglietti falliti (≤ 0).
    pub failed_points: i32,
    pub completed_count: u32,
    /// Rotte prese in prestito dall'assegnazione scelta: (città stazione, rotta).
    pub borrowed: Vec<(CityId, RouteId)>,
}

impl TicketEvaluation {
    fn net(&self) -> i32 {
        self.completed_points.saturating_add(self.failed_points)
    }
}

/// Un biglietto da valutare: partenza e destinazioni `(città, punti)`; quello classico ha
/// una sola destinazione.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketSpec<'a> {
    pub from: &'a str,
    pub destinations: Vec<(&'a str, u32)>,
}

impl<'a> TicketSpec<'a> {
    #[must_use]
    pub fn of(def: &'a TicketDef) -> Self {
        Self {
            from: &def.from,
            destinations: def.destinations().collect(),
        }
    }

    /// Biglietto classico: una sola destinazione.
    #[must_use]
    pub fn single(from: &'a str, to: &'a str, points: u32) -> Self {
        Self {
            from,
            destinations: vec![(to, points)],
        }
    }

    /// Punti persi se nessuna destinazione è collegata: il valore più basso.
    fn failed_points(&self) -> u32 {
        self.destinations.iter().map(|&(_, p)| p).min().unwrap_or(0)
    }

    /// La destinazione collegata di punti più alti (la prima a parità): `(indice, punti)`.
    fn best_connected(&self, uf: &mut UnionFind) -> Option<(usize, u32)> {
        let mut best: Option<(usize, u32)> = None;
        for (i, &(to, points)) in self.destinations.iter().enumerate() {
            if best.is_some_and(|(_, p)| p >= points) {
                continue;
            }
            if uf.connected(self.from, to) {
                best = Some((i, points));
            }
        }
        best
    }
}

fn score_tickets(uf: &mut UnionFind, tickets: &[TicketSpec]) -> TicketEvaluation {
    let mut eval = TicketEvaluation {
        completed: Vec::with_capacity(tickets.len()),
        reached: Vec::with_capacity(tickets.len()),
        completed_points: 0,
        failed_points: 0,
        completed_count: 0,
        borrowed: Vec::new(),
    };
    for spec in tickets {
        let best = spec.best_connected(uf);
        eval.completed.push(best.is_some());
        eval.reached.push(best.map(|(i, _)| i));
        if let Some((_, points)) = best {
            eval.completed_points = eval
                .completed_points
                .saturating_add(points_to_score(points));
            eval.completed_count += 1;
        } else {
            eval.failed_points = eval
                .failed_points
                .saturating_sub(points_to_score(spec.failed_points()));
        }
    }
    eval
}

/// Esito per biglietto di una valutazione: destinazione realizzata e punti guadagnati o persi.
fn ticket_results(
    defs: &[&TicketDef],
    specs: &[TicketSpec],
    eval: &TicketEvaluation,
) -> Vec<TicketResult> {
    defs.iter()
        .zip(specs)
        .zip(&eval.reached)
        .map(|((def, spec), reached)| match reached {
            Some(k) => {
                let (to, points) = spec.destinations[*k];
                TicketResult {
                    ticket_id: def.id.clone(),
                    completed: true,
                    destination: Some(to.to_string()),
                    mandala: false,
                    points: points_to_score(points),
                }
            }
            None => TicketResult {
                ticket_id: def.id.clone(),
                completed: false,
                destination: None,
                mandala: false,
                points: -points_to_score(spec.failed_points()),
            },
        })
        .collect()
}

/// Valuta i biglietti con la connettività propria `own` e, per ogni
/// stazione, la scelta di non usarne nessuna o di prendere in prestito una delle sue
/// rotte avversarie. Vince l'assegnazione che massimizza punti completati meno punti
/// falliti; a parità la prima in ordine canonico (stazioni in ordine di costruzione,
/// per ciascuna prima "nessuna rotta", poi le rotte per id crescente).
///
/// Prima della ricerca le opzioni equivalenti a un'altra precedente nell'ordine
/// canonico vengono scartate (vedi `prune_station_options`): il risultato non cambia.
pub fn evaluate_tickets(
    own: &mut UnionFind,
    tickets: &[TicketSpec],
    stations: &[StationCandidates],
) -> TicketEvaluation {
    if stations.is_empty() {
        return score_tickets(own, tickets);
    }
    let pruned = prune_station_options(own, stations);
    let mut best: Option<TicketEvaluation> = None;
    let mut picks: Vec<Option<usize>> = Vec::with_capacity(pruned.len());
    search_stations(own, tickets, &pruned, &mut picks, &mut best);
    best.expect("la ricerca visita almeno l'assegnazione vuota")
}

/// Scarta le opzioni che non possono mai vincere, senza cambiare l'esito della ricerca.
///
/// Le assegnazioni delle stazioni precedenti possono solo aggiungere unioni a `own`, quindi:
/// - un'opzione il cui `other_city` è già connesso alla città della stazione in `own` non
///   cambia la connettività, come "nessuna rotta", che viene prima: non vince mai;
/// - fra le opzioni di una stazione con `other_city` nella stessa componente di `own`,
///   l'effetto è identico, quindi vince sempre la prima per id.
fn prune_station_options(
    own: &mut UnionFind,
    stations: &[StationCandidates],
) -> Vec<StationCandidates> {
    stations
        .iter()
        .map(|station| {
            let city_root = own.find(&station.city);
            let mut seen_roots: Vec<String> = Vec::new();
            let mut options = Vec::new();
            for option in &station.options {
                let root = own.find(&option.other_city);
                if root == city_root || seen_roots.contains(&root) {
                    continue;
                }
                seen_roots.push(root);
                options.push(option.clone());
            }
            StationCandidates {
                city: station.city.clone(),
                options,
            }
        })
        .collect()
}

fn search_stations(
    uf: &mut UnionFind,
    tickets: &[TicketSpec],
    stations: &[StationCandidates],
    picks: &mut Vec<Option<usize>>,
    best: &mut Option<TicketEvaluation>,
) {
    let depth = picks.len();
    if depth == stations.len() {
        let mut eval = score_tickets(uf, tickets);
        if best.as_ref().is_none_or(|b| eval.net() > b.net()) {
            eval.borrowed = picks
                .iter()
                .zip(stations)
                .filter_map(|(pick, st)| {
                    pick.map(|k| (st.city.clone(), st.options[k].route_id.clone()))
                })
                .collect();
            *best = Some(eval);
        }
        return;
    }
    let station = &stations[depth];
    picks.push(None);
    search_stations(uf, tickets, stations, picks, best);
    picks.pop();
    for (k, option) in station.options.iter().enumerate() {
        let mut next = uf.deep_clone();
        next.union(&station.city, &option.other_city);
        picks.push(Some(k));
        search_stations(&mut next, tickets, stations, picks, best);
        picks.pop();
    }
}

// ── Longest Continuous Path ──────────────────────────────────────────

/// Max search nodes summed over all components; past it the result is a lower bound.
pub(crate) const MAX_SEARCH_NODES: u64 = 4_000_000;

/// Max cyclomatic number (routes minus cities plus one) that map validation admits for a
/// component a player can build; the search solves every such component exactly.
pub const MAX_CYCLOMATIC_NUMBER: u32 = 14;

/// Lato orientato del grafo delle rotte reclamate.
#[derive(Debug, Clone, Copy)]
struct Arc {
    to: usize,
    /// Indice globale della rotta.
    edge: usize,
    len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leave {
    /// Not decided yet.
    Open,
    /// The trail does not use this route.
    Skipped,
    /// The trail uses this route.
    Kept,
}

/// Branch and bound for the longest trail of a cyclic component.
///
/// A trail is a connected set of routes with at most two odd cities, so the routes it skips
/// must fix the parity of every other odd city. The search branches on an odd city: skip one
/// of its routes, or keep them all and make it a trail end.
struct TrailSearch<'a> {
    ends: &'a [(usize, usize)],
    lens: &'a [u32],
    /// Routes between two different cities at each city, shortest first.
    incident: Vec<Vec<usize>>,
    /// Cities and routes of the component being searched.
    cities: Vec<usize>,
    routes: Vec<usize>,
    leave: Vec<Leave>,
    odd: Vec<bool>,
    odd_count: usize,
    /// Cities that end the trail: all their routes are kept, so their parity stays odd.
    trail_end: Vec<bool>,
    trail_ends: usize,
    /// Total length of the component and of the routes skipped so far.
    total: u32,
    skipped: u32,
    /// Routes set to `Leave::Kept` by the branches being explored.
    kept_stack: Vec<usize>,
    /// Scratch for `extra_skip_cost` and the piece search.
    costs: Vec<u32>,
    parent: Vec<usize>,
    piece_len: Vec<u32>,
    nodes: u64,
    node_budget: u64,
    /// Longest trail found so far over all components.
    best: u32,
    /// Routes of the trail that reached `best`, as global route indices.
    best_routes: Vec<usize>,
    /// Set when the node budget ended the search before it was complete.
    exhausted: bool,
}

impl TrailSearch<'_> {
    /// Finds the longest trail of the component with these cities and routes, raising `best`.
    fn run_component(&mut self, cities: &[usize], routes: &[usize]) {
        self.cities.clear();
        self.cities.extend_from_slice(cities);
        self.routes.clear();
        self.routes.extend_from_slice(routes);
        self.total = routes
            .iter()
            .fold(0u32, |acc, &e| acc.saturating_add(self.lens[e]));
        self.skipped = 0;
        self.odd_count = 0;
        self.trail_ends = 0;
        for &city in cities {
            self.odd[city] = false;
            self.trail_end[city] = false;
        }
        for &route in routes {
            self.leave[route] = Leave::Open;
            let (a, b) = self.ends[route];
            if a != b {
                self.flip(a);
                self.flip(b);
            }
        }
        self.search();
    }

    /// Flips the parity of `city`.
    fn flip(&mut self, city: usize) {
        self.odd[city] = !self.odd[city];
        if self.odd[city] {
            self.odd_count += 1;
        } else {
            self.odd_count -= 1;
        }
    }

    /// Lower bound on the length still to skip: every odd city that is not a trail end
    /// needs an adjacent skipped route, and a route serves two cities. `None` when more
    /// odd cities than the two trail ends have no route left to skip.
    fn extra_skip_cost(&mut self) -> Option<u32> {
        let spare_ends = 2 - self.trail_ends;
        let mut stuck = 0;
        self.costs.clear();
        for &city in &self.cities {
            if !self.odd[city] || self.trail_end[city] {
                continue;
            }
            match self.incident[city]
                .iter()
                .find(|&&e| self.leave[e] == Leave::Open)
            {
                Some(&e) => self.costs.push(self.lens[e]),
                None => stuck += 1,
            }
        }
        if stuck > spare_ends {
            return None;
        }
        self.costs.sort_unstable_by(|a, b| b.cmp(a));
        let cheapest: u32 = self
            .costs
            .iter()
            .skip(spare_ends - stuck)
            .fold(0u32, |acc, &c| acc.saturating_add(c));
        Some(cheapest.div_ceil(2))
    }

    /// Odd city that is not a trail end with the fewest routes left to decide.
    fn branch_city(&self) -> Option<usize> {
        self.cities
            .iter()
            .copied()
            .filter(|&c| self.odd[c] && !self.trail_end[c])
            .min_by_key(|&c| {
                self.incident[c]
                    .iter()
                    .filter(|&&e| self.leave[e] == Leave::Open)
                    .count()
            })
    }

    fn search(&mut self) {
        if self.nodes >= self.node_budget {
            self.exhausted = true;
            return;
        }
        self.nodes += 1;
        let kept = self.total - self.skipped;
        if kept <= self.best {
            return;
        }
        if self.odd_count <= 2 {
            self.score_leaf();
            return;
        }
        let Some(extra) = self.extra_skip_cost() else {
            return;
        };
        if kept.saturating_sub(extra) <= self.best {
            return;
        }
        let Some(city) = self.branch_city() else {
            return;
        };

        let base = self.kept_stack.len();
        for k in 0..self.incident[city].len() {
            let route = self.incident[city][k];
            if self.leave[route] != Leave::Open {
                continue;
            }
            let (a, b) = self.ends[route];
            let other = if a == city { b } else { a };
            self.leave[route] = Leave::Skipped;
            self.skipped += self.lens[route];
            self.flip(city);
            self.flip(other);
            self.search();
            self.flip(other);
            self.flip(city);
            self.skipped -= self.lens[route];
            self.leave[route] = Leave::Kept;
            self.kept_stack.push(route);
            if self.exhausted {
                break;
            }
        }
        if !self.exhausted && self.trail_ends < 2 {
            self.trail_end[city] = true;
            self.trail_ends += 1;
            self.search();
            self.trail_ends -= 1;
            self.trail_end[city] = false;
        }
        while self.kept_stack.len() > base {
            if let Some(route) = self.kept_stack.pop() {
                self.leave[route] = Leave::Open;
            }
        }
    }

    /// Skipped routes make at most two cities odd: the heaviest connected piece of the
    /// routes still kept has an Euler trail.
    fn score_leaf(&mut self) {
        for &city in &self.cities {
            self.parent[city] = city;
            self.piece_len[city] = 0;
        }
        for &route in &self.routes {
            if self.leave[route] != Leave::Skipped {
                let (a, b) = self.ends[route];
                let (ra, rb) = (root_of(&mut self.parent, a), root_of(&mut self.parent, b));
                self.parent[ra] = rb;
            }
        }
        for &route in &self.routes {
            if self.leave[route] != Leave::Skipped {
                let root = root_of(&mut self.parent, self.ends[route].0);
                self.piece_len[root] = self.piece_len[root].saturating_add(self.lens[route]);
            }
        }
        let heaviest = self
            .cities
            .iter()
            .map(|&c| self.piece_len[c])
            .max()
            .unwrap_or(0);
        if heaviest > self.best {
            self.best = heaviest;
            let root = self
                .cities
                .iter()
                .copied()
                .find(|&c| self.piece_len[c] == heaviest);
            self.best_routes.clear();
            if let Some(root) = root {
                for &route in &self.routes {
                    if self.leave[route] != Leave::Skipped
                        && root_of(&mut self.parent, self.ends[route].0) == root
                    {
                        self.best_routes.push(route);
                    }
                }
            }
        }
    }
}

/// Distanza pesata massima da `start` in un albero, con la città più lontana.
/// `pred[città]` riceve (città precedente, rotta) del percorso da `start`.
fn farthest_in_tree(adj: &[Vec<Arc>], start: usize, pred: &mut [(usize, usize)]) -> (usize, u32) {
    let mut far = (start, 0);
    let mut stack: Vec<(usize, usize, usize, u32)> = vec![(start, usize::MAX, usize::MAX, 0)];
    while let Some((city, from, parent_edge, dist)) = stack.pop() {
        pred[city] = (from, parent_edge);
        if dist > far.1 {
            far = (city, dist);
        }
        for arc in &adj[city] {
            if arc.edge != parent_edge {
                stack.push((arc.to, city, arc.edge, dist.saturating_add(arc.len)));
            }
        }
    }
    far
}

/// Indice della radice di `x` in un union-find su vettore.
fn root_of(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Longest continuous path (a trail: no route is reused, cities may repeat) over the
/// claimed routes, as the sum of the route lengths along the trail.
///
/// Tree components use the weighted diameter, the others a branch and bound over the routes
/// the trail leaves out. `Ruleset::validate` bounds the cyclomatic number of any component
/// a player can build, so on validated maps the `MAX_SEARCH_NODES` budget is a safety net
/// that is never reached.
pub fn longest_continuous_path(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
) -> u32 {
    longest_path_report(claimed_routes, routes).length
}

/// Result of the longest path search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LongestPath {
    pub length: u32,
    /// False when the node budget stopped the search: `length` is then a lower bound that
    /// depends on the route and city ids.
    pub exact: bool,
}

/// Longest path with the exactness flag. The search order depends only on route and city
/// ids, never on the order of `claimed_routes`.
pub fn longest_path_report(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
) -> LongestPath {
    longest_path_trail(claimed_routes, routes).path
}

/// A longest path together with the routes that form it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongestTrail {
    pub path: LongestPath,
    /// Ids of the routes of one longest trail, sorted. Their lengths add up to
    /// `path.length`. The choice depends only on route and city ids: of several trails of
    /// the same length the first one found by the search wins.
    pub route_ids: Vec<RouteId>,
}

/// Longest path and the routes of one such trail, with the `MAX_SEARCH_NODES` budget.
pub fn longest_path_trail(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
) -> LongestTrail {
    longest_path_trail_with_countries(claimed_routes, routes, &BTreeSet::new())
}

/// Like `longest_path_trail` on a map with `countries`: dead-end nodes, so two routes that
/// enter the same country are not connected through it and the trail cannot cross it.
pub fn longest_path_trail_with_countries(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
    countries: &BTreeSet<CityId>,
) -> LongestTrail {
    longest_path_search(claimed_routes, routes, countries, MAX_SEARCH_NODES)
}

#[cfg(test)]
fn longest_path_with_budget(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
    node_budget: u64,
) -> LongestTrail {
    longest_path_search(claimed_routes, routes, &BTreeSet::new(), node_budget)
}

fn longest_path_search(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
    countries: &BTreeSet<CityId>,
    node_budget: u64,
) -> LongestTrail {
    let mut by_id: BTreeMap<&str, &Route> = BTreeMap::new();
    for cri in claimed_routes {
        if let Some(route) = routes.get(&cri.route_id) {
            by_id.insert(cri.route_id.as_str(), route);
        }
    }
    let graph_routes: Vec<&Route> = by_id.into_values().collect();

    // Ogni rotta che entra in un Paese vi arriva con un nodo privato.
    let nodes: Vec<(String, String)> = graph_routes
        .iter()
        .map(|r| {
            (
                route_node(countries, &r.id, &r.from),
                route_node(countries, &r.id, &r.to),
            )
        })
        .collect();
    let cities: BTreeSet<&str> = nodes
        .iter()
        .flat_map(|(a, b)| [a.as_str(), b.as_str()])
        .collect();
    let city_index: BTreeMap<&str, usize> =
        cities.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let city_count = city_index.len();

    let endpoints: Vec<(usize, usize)> = nodes
        .iter()
        .map(|(a, b)| (city_index[a.as_str()], city_index[b.as_str()]))
        .collect();
    let lens: Vec<u32> = graph_routes.iter().map(|r| r.len).collect();

    // Componenti connesse.
    let mut parent: Vec<usize> = (0..city_count).collect();
    for &(a, b) in &endpoints {
        let (ra, rb) = (root_of(&mut parent, a), root_of(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut component_edges: Vec<Vec<usize>> = vec![Vec::new(); city_count];
    let mut component_cities: Vec<Vec<usize>> = vec![Vec::new(); city_count];
    for city in 0..city_count {
        component_cities[root_of(&mut parent, city)].push(city);
    }
    for (edge, &(a, _)) in endpoints.iter().enumerate() {
        component_edges[root_of(&mut parent, a)].push(edge);
    }

    let mut adj: Vec<Vec<Arc>> = vec![Vec::new(); city_count];
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); city_count];
    for (edge, &(a, b)) in endpoints.iter().enumerate() {
        let len = lens[edge];
        adj[a].push(Arc { to: b, edge, len });
        adj[b].push(Arc { to: a, edge, len });
        if a != b {
            incident[a].push(edge);
            incident[b].push(edge);
        }
    }
    for edges in &mut incident {
        edges.sort_by_key(|&e| (lens[e], e));
    }

    let mut search = TrailSearch {
        ends: &endpoints,
        lens: &lens,
        incident,
        cities: Vec::new(),
        routes: Vec::new(),
        leave: vec![Leave::Open; graph_routes.len()],
        odd: vec![false; city_count],
        odd_count: 0,
        trail_end: vec![false; city_count],
        trail_ends: 0,
        total: 0,
        skipped: 0,
        kept_stack: Vec::new(),
        costs: Vec::new(),
        parent: vec![0; city_count],
        piece_len: vec![0; city_count],
        nodes: 0,
        node_budget,
        best: 0,
        best_routes: Vec::new(),
        exhausted: false,
    };
    // Scratch per i componenti ad albero: (città precedente, rotta) di ogni città.
    let mut pred: Vec<(usize, usize)> = vec![(usize::MAX, usize::MAX); city_count];
    // Componenti in ordine canonico: per città minima (id), non per radice union-find.
    for first in 0..city_count {
        let root = root_of(&mut parent, first);
        let edges = &component_edges[root];
        let cities_here = &component_cities[root];
        if edges.is_empty() || cities_here[0] != first {
            continue;
        }
        if edges.len() + 1 == cities_here.len() {
            let (end, _) = farthest_in_tree(&adj, cities_here[0], &mut pred);
            let (far, diameter) = farthest_in_tree(&adj, end, &mut pred);
            if diameter > search.best {
                search.best = diameter;
                search.best_routes.clear();
                let mut city = far;
                while city != end {
                    let (previous, edge) = pred[city];
                    search.best_routes.push(edge);
                    city = previous;
                }
            }
        } else {
            search.run_component(cities_here, edges);
        }
    }
    // Gli indici seguono l'ordine degli id: ordinare gli indici ordina gli id.
    search.best_routes.sort_unstable();
    LongestTrail {
        path: LongestPath {
            length: search.best,
            exact: !search.exhausted,
        },
        route_ids: search
            .best_routes
            .iter()
            .map(|&edge| graph_routes[edge].id.clone())
            .collect(),
    }
}

// ── Bonus Mandala (India) ────────────────────────────────────────────

/// Rete di un giocatore per il bonus `mandala`: solo le sue rotte, ognuna un arco non
/// orientato di capacità 1. Gli estremi Paese sono nodi privati (`route_node`), quindi un
/// Paese non collega mai due rotte e non è mai un estremo raggiungibile di un biglietto.
pub(crate) struct MandalaGraph {
    /// Nome del nodo → indice, in ordine alfabetico.
    nodes: BTreeMap<String, usize>,
    /// Per nodo, gli archi uscenti (indici in `arcs`) nell'ordine di id delle rotte.
    adjacency: Vec<Vec<usize>>,
    /// Arco `2e` = `u → v`, arco `2e + 1` = `v → u` (il residuo l'uno dell'altro); l'elemento
    /// è il nodo di arrivo.
    arcs: Vec<usize>,
}

impl MandalaGraph {
    pub(crate) fn new(
        claimed: &[ClaimedRouteInfo],
        routes_map: &BTreeMap<RouteId, Route>,
        countries: &BTreeSet<CityId>,
    ) -> Self {
        let mut ids: Vec<&RouteId> = claimed.iter().map(|c| &c.route_id).collect();
        ids.sort();
        let ends: Vec<(String, String)> = ids
            .iter()
            .filter_map(|id| routes_map.get(*id))
            .map(|r| {
                (
                    route_node(countries, &r.id, &r.from),
                    route_node(countries, &r.id, &r.to),
                )
            })
            .collect();
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for (from, to) in &ends {
            names.insert(from);
            names.insert(to);
        }
        let nodes: BTreeMap<String, usize> = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name.to_string(), i))
            .collect();
        let mut adjacency = vec![Vec::new(); nodes.len()];
        let mut arcs = Vec::with_capacity(ends.len() * 2);
        for (from, to) in &ends {
            let (u, v) = (nodes[from], nodes[to]);
            adjacency[u].push(arcs.len());
            arcs.push(v);
            adjacency[v].push(arcs.len());
            arcs.push(u);
        }
        Self {
            nodes,
            adjacency,
            arcs,
        }
    }

    /// `true` se `from` e `to` sono collegati da almeno due percorsi continui senza rotte in
    /// comune (possono incrociarsi nelle città): flusso massimo ≥ 2 con capacità unitarie,
    /// cioè due aumenti BFS.
    pub(crate) fn two_disjoint_paths(&self, from: &str, to: &str) -> bool {
        let (Some(&source), Some(&sink)) = (self.nodes.get(from), self.nodes.get(to)) else {
            return false;
        };
        if source == sink {
            return false;
        }
        let mut capacity = vec![1u8; self.arcs.len()];
        (0..2).all(|_| self.augment(source, sink, &mut capacity))
    }

    /// Un cammino aumentante BFS da `source` a `sink` sulla rete residua.
    fn augment(&self, source: usize, sink: usize, capacity: &mut [u8]) -> bool {
        let mut via: Vec<Option<usize>> = vec![None; self.adjacency.len()];
        let mut seen = vec![false; self.adjacency.len()];
        seen[source] = true;
        let mut queue = VecDeque::from([source]);
        while let Some(node) = queue.pop_front() {
            for &arc in &self.adjacency[node] {
                let next = self.arcs[arc];
                if capacity[arc] == 0 || seen[next] {
                    continue;
                }
                seen[next] = true;
                via[next] = Some(arc);
                if next == sink {
                    let mut at = sink;
                    while let Some(arc) = via[at] {
                        capacity[arc] -= 1;
                        capacity[arc ^ 1] += 1;
                        at = self.arcs[arc ^ 1];
                    }
                    return true;
                }
                queue.push_back(next);
            }
        }
        false
    }
}

/// Bonus `mandala` con `count` biglietti qualificati: `table[count - 1]`, l'ultimo valore
/// oltre la fine della tabella, 0 senza biglietti.
#[must_use]
pub fn mandala_points(table: &[u32], count: usize) -> u32 {
    match count {
        0 => 0,
        n => table.get(n - 1).or(table.last()).copied().unwrap_or(0),
    }
}

// ── Bonus rete più grande (Asian Explorer, Legendary Asia) ───────────

/// Numero di città della componente più grande della rete delle rotte `claimed` del
/// giocatore (0 senza rotte). Conta solo le sue rotte, non quelle prese in prestito dalle
/// stazioni; rami e anelli sono ammessi e ogni città vale una volta. Gli estremi Paese sono
/// nodi privati (`route_node`): non uniscono due rotte, e un Paese conta una sola volta
/// per componente.
#[must_use]
pub fn largest_network_size(
    claimed: &[ClaimedRouteInfo],
    routes_map: &BTreeMap<RouteId, Route>,
    countries: &BTreeSet<CityId>,
) -> u32 {
    let mut uf = UnionFind::new();
    // Nodo → città che rappresenta.
    let mut nodes: BTreeMap<String, &str> = BTreeMap::new();
    for route in claimed.iter().filter_map(|c| routes_map.get(&c.route_id)) {
        let from = route_node(countries, &route.id, &route.from);
        let to = route_node(countries, &route.id, &route.to);
        uf.union(&from, &to);
        nodes.insert(from, &route.from);
        nodes.insert(to, &route.to);
    }
    let mut components: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (node, city) in &nodes {
        components.entry(uf.find(node)).or_default().insert(city);
    }
    let largest = components.values().map(BTreeSet::len).max().unwrap_or(0);
    u32::try_from(largest).unwrap_or(u32::MAX)
}

/// Indici dei giocatori con la rete più grande (parità → tutti i pari merito); nessuno se
/// il massimo è 0.
#[must_use]
pub fn largest_network_winners(sizes: &[u32]) -> Vec<usize> {
    let max = sizes.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return Vec::new();
    }
    sizes
        .iter()
        .enumerate()
        .filter(|&(_, &size)| size == max)
        .map(|(i, _)| i)
        .collect()
}

// ── Final scoring ────────────────────────────────────────────────────

/// Risultato dello scoring finale per un giocatore.
#[derive(Debug, Clone)]
pub struct PlayerScore {
    pub route_points: i32,
    pub completed_tickets: i32,
    /// Numero di biglietti completati (spareggio).
    pub completed_count: u32,
    pub failed_tickets: i32,
    pub longest_path: u32,
    /// False when the longest path search hit its budget: `longest_path` is a lower bound.
    pub longest_path_exact: bool,
    /// Ids (sorted) of the routes of one longest path of the player.
    pub longest_path_routes: Vec<RouteId>,
    /// Rotte avversarie prese in prestito dalle stazioni: (città, rotta).
    pub borrowed_routes: Vec<(CityId, RouteId)>,
    /// Esito di ogni biglietto, nell'ordine degli id.
    pub ticket_results: Vec<TicketResult>,
}

/// Regole sul percorso più lungo di un ruleset: punti del bonus (`None` = nessun bonus) e se
/// serve calcolarlo come spareggio `longestPath` anche senza bonus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LongestPathRules {
    pub bonus_points: Option<u32>,
    pub tie_break: bool,
}

/// Calcola il punteggio finale per tutti i giocatori.
///
/// - Somma punti rotte (già accumulati in `player.score`)
/// - Biglietti: ±valore in base alla connettività Union-Find, con la migliore
///   assegnazione delle stazioni (D32, `stations` per giocatore)
/// - Longest Continuous Path: +bonus a chi ha il percorso più lungo (parità → bonus a tutti
///   i pari merito). Il percorso si calcola solo con il bonus o con lo spareggio
///   (`path_rules`); senza bonus nessuno vince il bonus
pub fn compute_final_scores(
    claimed_routes: &[Vec<ClaimedRouteInfo>], // per giocatore
    routes_map: &BTreeMap<RouteId, Route>,
    tickets: &[TicketDef],
    player_tickets: &[BTreeMap<String, bool>], // ticket_id → kept
    stations: &[Vec<StationCandidates>],       // per giocatore, in ordine di costruzione
    connectivity: &mut PlayerConnectivity,
    path_rules: LongestPathRules,
) -> (Vec<PlayerScore>, Vec<usize>, u32) {
    let bonus_points = path_rules.bonus_points;
    let num_players = claimed_routes.len();
    let mut scores: Vec<PlayerScore> = (0..num_players)
        .map(|_| PlayerScore {
            route_points: 0,
            completed_tickets: 0,
            completed_count: 0,
            failed_tickets: 0,
            longest_path: 0,
            longest_path_exact: true,
            longest_path_routes: Vec::new(),
            borrowed_routes: Vec::new(),
            ticket_results: Vec::new(),
        })
        .collect();

    // Calcola longest path per ogni giocatore, solo se serve (bonus o spareggio)
    let mut longest_paths: Vec<u32> = vec![0; num_players];
    let path_routes: &[Vec<ClaimedRouteInfo>] = if bonus_points.is_some() || path_rules.tie_break {
        claimed_routes
    } else {
        &[]
    };
    let countries = connectivity.countries().clone();
    for (pi, routes) in path_routes.iter().enumerate() {
        let cri_refs: Vec<&ClaimedRouteInfo> = routes.iter().collect();
        let trail = longest_path_trail_with_countries(&cri_refs, routes_map, &countries);
        longest_paths[pi] = trail.path.length;
        scores[pi].longest_path = trail.path.length;
        scores[pi].longest_path_exact = trail.path.exact;
        scores[pi].longest_path_routes = trail.route_ids;
    }

    // Determina il massimo
    let max_path = longest_paths.iter().copied().max().unwrap_or(0);

    // Chi ha il max (o pari merito) prende il bonus
    let bonus_winners: Vec<usize> = if bonus_points.is_some() && max_path > 0 {
        longest_paths
            .iter()
            .enumerate()
            .filter(|&(_, len)| *len == max_path)
            .map(|(i, _)| i)
            .collect()
    } else {
        vec![]
    };

    // Calcola biglietti
    for (pi, pt) in player_tickets.iter().enumerate() {
        let defs: Vec<&TicketDef> = pt
            .keys()
            .filter_map(|tid| tickets.iter().find(|t| &t.id == tid))
            .collect();
        let list: Vec<TicketSpec> = defs.iter().map(|t| TicketSpec::of(t)).collect();
        let own_stations = stations.get(pi).map_or(&[][..], Vec::as_slice);
        let eval = match connectivity.uf_mut(pi) {
            Some(uf) => evaluate_tickets(uf, &list, own_stations),
            None => evaluate_tickets(&mut UnionFind::new(), &list, own_stations),
        };
        scores[pi].completed_tickets = eval.completed_points;
        scores[pi].failed_tickets = eval.failed_points;
        scores[pi].completed_count = eval.completed_count;
        scores[pi].ticket_results = ticket_results(&defs, &list, &eval);
        scores[pi].borrowed_routes = eval.borrowed;
    }

    (scores, bonus_winners, bonus_points.unwrap_or(0))
}

/// Indici dei giocatori con più biglietti completati (parità → tutti i pari merito);
/// nessuno se il massimo è 0 (bonus globetrotter).
#[must_use]
pub fn globetrotter_winners(scores: &[PlayerScore]) -> Vec<usize> {
    let max = scores.iter().map(|s| s.completed_count).max().unwrap_or(0);
    if max == 0 {
        return Vec::new();
    }
    scores
        .iter()
        .enumerate()
        .filter(|(_, s)| s.completed_count == max)
        .map(|(i, _)| i)
        .collect()
}

/// Punti delle maggioranze di passeggeri (Germany 2017), per giocatore. Per ogni colore
/// contano solo i giocatori con almeno un passeggero di quel colore: il massimo prende
/// `first`; con il massimo condiviso tutti i pari prendono `first` e nessuno `second`;
/// altrimenti chi ha il secondo valore più alto prende `second` (anche a pari merito).
#[must_use]
pub fn passenger_majority_points(
    held: &[&BTreeMap<String, u32>],
    colors: &[String],
    first: u32,
    second: u32,
) -> Vec<u32> {
    let mut points = vec![0u32; held.len()];
    for color in colors {
        let counts: Vec<u32> = held
            .iter()
            .map(|p| p.get(color).copied().unwrap_or(0))
            .collect();
        let Some(&max) = counts.iter().max() else {
            continue;
        };
        if max == 0 {
            continue;
        }
        let leaders = counts.iter().filter(|&&c| c == max).count();
        let runner_up = counts
            .iter()
            .copied()
            .filter(|&c| c < max)
            .max()
            .unwrap_or(0);
        for (pi, &count) in counts.iter().enumerate() {
            let award = if count == max {
                first
            } else if leaders == 1 && runner_up > 0 && count == runner_up {
                second
            } else {
                0
            };
            points[pi] = points[pi].saturating_add(award);
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClaimedRouteInfo;
    use proptest::prelude::*;

    fn make_route(id: &str, from: &str, to: &str, len: u32) -> Route {
        Route {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            len,
            color: "Gray".to_string(),
            double: false,
            always_open: false,
            tunnel: false,
            ferries: 0,
            any_cards_per_space: None,
            mountains: 0,
        }
    }

    /// Maggioranze dei passeggeri: leader unico e secondi a pari merito, leader a pari
    /// merito (nessun secondo), zero passeggeri non danno punti nemmeno come secondi.
    #[test]
    fn passenger_majorities_follow_the_rulebook() {
        let held = |counts: &[u32]| -> Vec<BTreeMap<String, u32>> {
            counts
                .iter()
                .map(|&n| {
                    let mut m = BTreeMap::new();
                    if n > 0 {
                        m.insert("Red".to_string(), n);
                    }
                    m
                })
                .collect()
        };
        let score = |counts: &[u32]| {
            let players = held(counts);
            let refs: Vec<&BTreeMap<String, u32>> = players.iter().collect();
            passenger_majority_points(&refs, &["Red".to_string(), "Blue".to_string()], 20, 10)
        };
        assert_eq!(score(&[3, 1, 1, 0]), [20, 10, 10, 0]);
        assert_eq!(score(&[2, 2, 1]), [20, 20, 0]);
        assert_eq!(score(&[1, 0]), [20, 0]);
        assert_eq!(score(&[0, 0]), [0, 0]);
    }

    fn make_cri(id: &str) -> ClaimedRouteInfo {
        ClaimedRouteInfo {
            route_id: id.to_string(),
            claimed_by: 0,
        }
    }

    #[test]
    fn test_union_find_basic() {
        let mut uf = UnionFind::new();
        uf.union("a", "b");
        uf.union("b", "c");
        assert!(uf.connected("a", "c"));
        assert!(!uf.connected("a", "d"));
    }

    #[test]
    fn test_longest_path_simple() {
        let cri = [make_cri("a_b_1"), make_cri("b_c_1"), make_cri("c_d_1")];
        let mut routes = BTreeMap::new();
        routes.insert("a_b_1".into(), make_route("a_b_1", "a", "b", 3));
        routes.insert("b_c_1".into(), make_route("b_c_1", "b", "c", 2));
        routes.insert("c_d_1".into(), make_route("c_d_1", "c", "d", 4));

        let cri_refs: Vec<&ClaimedRouteInfo> = cri.iter().collect();
        let result = longest_continuous_path(&cri_refs, &routes);
        assert_eq!(result, 9); // 3+2+4
    }

    #[test]
    fn test_longest_path_y_shaped() {
        // a-b-c e b-d: il cammino più lungo è a-b-d (o d-b-c), non può riusare b
        let cri = [make_cri("a_b_1"), make_cri("b_c_1"), make_cri("b_d_1")];
        let mut routes = BTreeMap::new();
        routes.insert("a_b_1".into(), make_route("a_b_1", "a", "b", 3));
        routes.insert("b_c_1".into(), make_route("b_c_1", "b", "c", 2));
        routes.insert("b_d_1".into(), make_route("b_d_1", "b", "d", 5));

        let cri_refs: Vec<&ClaimedRouteInfo> = cri.iter().collect();
        let result = longest_continuous_path(&cri_refs, &routes);
        assert_eq!(result, 8); // a-b-d = 3+5 = 8
    }

    #[test]
    fn test_longest_path_city_reenter() {
        // Triangolo: a-b (3), b-c (2), c-a (4)
        // Cammino a-b-c-a = 3+2+4 = 9 (tutte e 3 le rotte, città riattraversabili)
        let cri = [make_cri("a_b_1"), make_cri("b_c_1"), make_cri("a_c_1")];
        let mut routes = BTreeMap::new();
        routes.insert("a_b_1".into(), make_route("a_b_1", "a", "b", 3));
        routes.insert("b_c_1".into(), make_route("b_c_1", "b", "c", 2));
        routes.insert("a_c_1".into(), make_route("a_c_1", "a", "c", 4));

        let cri_refs: Vec<&ClaimedRouteInfo> = cri.iter().collect();
        let result = longest_continuous_path(&cri_refs, &routes);
        assert_eq!(result, 9);
    }

    /// Algoritmo di riferimento: DFS esaustivo su trail da ogni città.
    fn reference_longest(claimed: &[&ClaimedRouteInfo], routes: &BTreeMap<RouteId, Route>) -> u32 {
        fn dfs<'a>(
            city: &'a str,
            current_len: u32,
            adj: &BTreeMap<&'a str, Vec<(&'a str, &'a str, u32)>>,
            used: &mut BTreeSet<&'a str>,
            best: &mut u32,
        ) {
            *best = (*best).max(current_len);
            if let Some(neighbors) = adj.get(city) {
                for &(next_city, route_id, len) in neighbors {
                    if used.insert(route_id) {
                        dfs(next_city, current_len + len, adj, used, best);
                        used.remove(route_id);
                    }
                }
            }
        }

        let mut adj: BTreeMap<&str, Vec<(&str, &str, u32)>> = BTreeMap::new();
        for cri in claimed {
            if let Some(route) = routes.get(&cri.route_id) {
                adj.entry(&route.from)
                    .or_default()
                    .push((&route.to, &cri.route_id, route.len));
                adj.entry(&route.to)
                    .or_default()
                    .push((&route.from, &cri.route_id, route.len));
            }
        }
        let mut best = 0;
        let mut used = BTreeSet::new();
        for &start in adj.keys() {
            dfs(start, 0, &adj, &mut used, &mut best);
        }
        best
    }

    fn route_map(routes: Vec<Route>) -> BTreeMap<RouteId, Route> {
        routes.into_iter().map(|r| (r.id.clone(), r)).collect()
    }

    fn claim_all(routes: &BTreeMap<RouteId, Route>) -> Vec<ClaimedRouteInfo> {
        routes.keys().map(|id| make_cri(id)).collect()
    }

    /// Le rotte restituite formano un cammino senza rotta ripetuta (connesse, al più due
    /// città di grado dispari) la cui lunghezza è quella del percorso più lungo.
    fn assert_trail_matches(
        claimed: &[ClaimedRouteInfo],
        routes: &BTreeMap<RouteId, Route>,
        case: &str,
    ) {
        let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        let trail = longest_path_trail(&refs, routes);
        assert!(
            trail.route_ids.windows(2).all(|w| w[0] < w[1]),
            "{case}: id non ordinati o ripetuti: {:?}",
            trail.route_ids
        );
        let mut total = 0;
        let mut degree: BTreeMap<&str, u32> = BTreeMap::new();
        let mut uf = UnionFind::new();
        for id in &trail.route_ids {
            assert!(
                claimed.iter().any(|c| &c.route_id == id),
                "{case}: rotta non reclamata {id}"
            );
            let route = &routes[id];
            total += route.len;
            *degree.entry(route.from.as_str()).or_default() += 1;
            *degree.entry(route.to.as_str()).or_default() += 1;
            uf.union(&route.from, &route.to);
        }
        assert_eq!(total, trail.path.length, "{case}: somma delle lunghezze");
        let odd = degree.values().filter(|d| **d % 2 == 1).count();
        assert!(odd <= 2, "{case}: {odd} città di grado dispari");
        let components: BTreeSet<String> = degree.keys().map(|c| uf.find(c)).collect();
        assert!(components.len() <= 1, "{case}: rotte non connesse");
    }

    fn longest(claimed: &[ClaimedRouteInfo], routes: &BTreeMap<RouteId, Route>) -> u32 {
        let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        longest_continuous_path(&refs, routes)
    }

    #[test]
    fn longest_path_matches_reference_on_random_usa_subgraphs() {
        use crate::rng::SeededRng;
        use crate::types::MapData;

        let map: MapData =
            serde_json::from_str(include_str!("../../../data/maps/usa.json")).unwrap();
        let routes: BTreeMap<RouteId, Route> =
            map.routes.into_iter().map(|r| (r.id.clone(), r)).collect();
        let mut ids: Vec<&RouteId> = routes.keys().collect();
        let mut rng = SeededRng::new(2026);

        for case in 0..500u32 {
            let budget = 10 + case % 36;
            rng.shuffle(&mut ids);
            let mut total = 0;
            let mut claimed: Vec<ClaimedRouteInfo> = Vec::new();
            for id in &ids {
                let len = routes[*id].len;
                if total + len <= budget {
                    total += len;
                    claimed.push(make_cri(id));
                }
            }
            let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
            assert!(longest_path_report(&refs, &routes).exact, "caso {case}");
            assert_eq!(
                longest_continuous_path(&refs, &routes),
                reference_longest(&refs, &routes),
                "caso {case}: {:?}",
                claimed.iter().map(|c| &c.route_id).collect::<Vec<_>>()
            );
            assert_trail_matches(&claimed, &routes, &format!("caso {case}"));
        }
    }

    fn complete_graph(n: usize) -> BTreeMap<RouteId, Route> {
        let mut routes = Vec::new();
        for a in 0..n {
            for b in (a + 1)..n {
                routes.push(make_route(
                    &format!("c{a}_c{b}_1"),
                    &format!("c{a}"),
                    &format!("c{b}"),
                    1,
                ));
            }
        }
        route_map(routes)
    }

    fn report(claimed: &[ClaimedRouteInfo], routes: &BTreeMap<RouteId, Route>) -> LongestPath {
        let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        longest_path_report(&refs, routes)
    }

    /// Time limit for K7 and K8: 2 s in release; debug code is much slower.
    #[test]
    fn longest_path_dense_graph_is_bounded() {
        let limit = if cfg!(debug_assertions) {
            std::time::Duration::from_secs(30)
        } else {
            std::time::Duration::from_secs(2)
        };
        for (n, expected) in [(7usize, 21u32), (8, 25)] {
            let routes = complete_graph(n);
            let claimed = claim_all(&routes);
            let started = std::time::Instant::now();
            let result = report(&claimed, &routes);
            let elapsed = started.elapsed();
            assert!(elapsed < limit, "K{n}: {elapsed:?}");
            assert_eq!(
                result,
                LongestPath {
                    length: expected,
                    exact: true
                },
                "K{n}"
            );
        }
    }

    /// K10 with 45 length-1 routes is the densest graph 45 trains can build.
    #[test]
    fn longest_path_k10_is_fast() {
        let routes = complete_graph(10);
        let claimed = claim_all(&routes);
        let started = std::time::Instant::now();
        let result = report(&claimed, &routes);
        let elapsed = started.elapsed();
        assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
        assert_eq!(
            result,
            LongestPath {
                length: 41,
                exact: true
            }
        );
    }

    #[test]
    fn longest_path_does_not_depend_on_claim_order() {
        let routes = complete_graph(7);
        let forward = claim_all(&routes);
        let mut backward = forward.clone();
        backward.reverse();
        assert_eq!(report(&forward, &routes), report(&backward, &routes));
    }

    /// Complete-graph routes among `n` cities named after `names[i]`, with the given lengths.
    fn labelled_graph(n: usize, keep: &[bool], lens: &[u32], names: &[usize]) -> Vec<Route> {
        let mut routes = Vec::new();
        let mut edge = 0;
        for a in 0..n {
            for b in (a + 1)..n {
                if keep[edge] {
                    let (from, to) = (format!("c{:02}", names[a]), format!("c{:02}", names[b]));
                    routes.push(make_route(
                        &format!("{from}_{to}_1"),
                        &from,
                        &to,
                        lens[edge],
                    ));
                }
                edge += 1;
            }
        }
        routes
    }

    /// Cyclomatic number of the graph on `n` cities with the kept edges.
    fn cyclomatic(n: usize, keep: &[bool]) -> u32 {
        let mut parent: Vec<usize> = (0..n).collect();
        let mut touched: BTreeSet<usize> = BTreeSet::new();
        let mut edges = 0u32;
        let mut edge = 0;
        for a in 0..n {
            for b in (a + 1)..n {
                if keep[edge] {
                    edges += 1;
                    touched.extend([a, b]);
                    let (ra, rb) = (root_of(&mut parent, a), root_of(&mut parent, b));
                    parent[ra] = rb;
                }
                edge += 1;
            }
        }
        let components: BTreeSet<usize> =
            touched.iter().map(|&c| root_of(&mut parent, c)).collect();
        let (components, touched) = (components.len(), touched.len());
        edges + u32::try_from(components).unwrap() - u32::try_from(touched).unwrap()
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

        /// Renaming the cities of a dense graph never changes the longest path.
        #[test]
        fn longest_path_invariant_under_city_relabeling(
            keep in proptest::collection::vec(any::<bool>(), 36),
            lens in proptest::collection::vec(1u32..=3, 36),
            perms in [
                Just((0..9usize).collect::<Vec<_>>()).prop_shuffle(),
                Just((0..9usize).collect::<Vec<_>>()).prop_shuffle(),
                Just((0..9usize).collect::<Vec<_>>()).prop_shuffle(),
                Just((0..9usize).collect::<Vec<_>>()).prop_shuffle(),
            ],
        ) {
            let n = 9;
            prop_assume!(cyclomatic(n, &keep[..]) <= MAX_CYCLOMATIC_NUMBER);
            let total: u32 = (0..36).filter(|&e| keep[e]).map(|e| lens[e]).sum();
            prop_assume!(total <= 45);

            let identity: Vec<usize> = (0..n).collect();
            let reference_routes = route_map(labelled_graph(n, &keep[..], &lens[..], &identity));
            let reference = report(&claim_all(&reference_routes), &reference_routes);
            prop_assert!(reference.exact);
            for perm in &perms {
                let routes = route_map(labelled_graph(n, &keep[..], &lens[..], perm));
                prop_assert_eq!(report(&claim_all(&routes), &routes), reference);
            }
        }
    }

    /// Multigraphs with self-loops and parallel routes agree with the exhaustive reference.
    #[test]
    fn longest_path_matches_reference_on_random_multigraphs() {
        use crate::rng::SeededRng;

        let mut rng = SeededRng::new(77);
        for case in 0..1500u32 {
            let cities = 2 + rng.next_word() as usize % 6;
            let count = 1 + rng.next_word() as usize % 8;
            let routes = route_map(
                (0..count)
                    .map(|i| {
                        let (a, b) = (
                            rng.next_word() as usize % cities,
                            rng.next_word() as usize % cities,
                        );
                        make_route(
                            &format!("r{i:02}"),
                            &format!("c{a}"),
                            &format!("c{b}"),
                            1 + (rng.next_word() % 4) as u32,
                        )
                    })
                    .collect(),
            );
            let claimed = claim_all(&routes);
            assert_trail_matches(&claimed, &routes, &format!("caso {case}"));
            let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
            assert_eq!(
                report(&claimed, &routes),
                LongestPath {
                    length: reference_longest(&refs, &routes),
                    exact: true
                },
                "caso {case}: {routes:?}"
            );
        }
    }

    /// A budget that ends the search early flags the length as a lower bound.
    #[test]
    fn longest_path_reports_when_the_budget_is_exhausted() {
        let routes = complete_graph(8);
        let claimed = claim_all(&routes);
        let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        let exact = longest_path_with_budget(&refs, &routes, MAX_SEARCH_NODES).path;
        assert!(exact.exact);
        let cut = longest_path_with_budget(&refs, &routes, 1).path;
        assert!(!cut.exact);
        assert!(cut.length <= exact.length);
    }

    /// With the budget spent on the first component, the one holding the smallest city id
    /// is searched first, whatever the union-find roots are.
    #[test]
    fn longest_path_with_exhausted_budget_searches_components_by_smallest_city() {
        // Triangle c0-c1-c6 (union-find root c6) and K4 on c2..c5 (root below c6).
        let mut list = vec![
            make_route("a0", "c0", "c6", 1),
            make_route("a1", "c1", "c6", 1),
            make_route("a2", "c0", "c1", 1),
        ];
        let k4 = ["c2", "c3", "c4", "c5"];
        for i in 0..4 {
            for j in i + 1..4 {
                list.push(make_route(&format!("b{i}{j}"), k4[i], k4[j], 5));
            }
        }
        let routes = route_map(list);
        let claimed = claim_all(&routes);
        let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        assert_eq!(
            longest_path_with_budget(&refs, &routes, 1).path,
            LongestPath {
                length: 3,
                exact: false
            }
        );
    }

    #[test]
    fn longest_path_tree_uses_weighted_diameter() {
        // a-b(3), b-c(2), b-d(5), d-e(4): il percorso più lungo è a-b-d-e = 12.
        let routes = route_map(vec![
            make_route("a_b_1", "a", "b", 3),
            make_route("b_c_1", "b", "c", 2),
            make_route("b_d_1", "b", "d", 5),
            make_route("d_e_1", "d", "e", 4),
        ]);
        assert_eq!(longest(&claim_all(&routes), &routes), 12);
    }

    #[test]
    fn longest_path_takes_best_component() {
        // Triangolo (3+3+3) separato da una catena più corta.
        let routes = route_map(vec![
            make_route("a_b_1", "a", "b", 3),
            make_route("b_c_1", "b", "c", 3),
            make_route("a_c_1", "a", "c", 3),
            make_route("x_y_1", "x", "y", 6),
        ]);
        assert_eq!(longest(&claim_all(&routes), &routes), 9);
    }

    #[test]
    fn longest_path_with_more_than_64_routes_in_a_component() {
        // Anello di 70 città: il trail attraversa tutte le rotte.
        let routes = route_map(
            (0..70)
                .map(|i| {
                    make_route(
                        &format!("r{i:02}_1"),
                        &format!("n{i:02}"),
                        &format!("n{:02}", (i + 1) % 70),
                        1,
                    )
                })
                .collect(),
        );
        assert_eq!(longest(&claim_all(&routes), &routes), 70);
    }

    #[test]
    fn longest_path_ignores_duplicate_and_unknown_routes() {
        let routes = route_map(vec![make_route("a_b_1", "a", "b", 3)]);
        let claimed = [make_cri("a_b_1"), make_cri("a_b_1"), make_cri("ghost_1")];
        assert_eq!(longest(&claimed, &routes), 3);
        assert_eq!(longest(&[], &routes), 0);
    }

    #[test]
    fn longest_path_routes_on_known_graphs() {
        // Albero: a-b-d-e (12), senza la foglia c.
        let tree = route_map(vec![
            make_route("a_b_1", "a", "b", 3),
            make_route("b_c_1", "b", "c", 2),
            make_route("b_d_1", "b", "d", 5),
            make_route("d_e_1", "d", "e", 4),
        ]);
        let refs: Vec<ClaimedRouteInfo> = claim_all(&tree);
        let cri: Vec<&ClaimedRouteInfo> = refs.iter().collect();
        assert_eq!(
            longest_path_trail(&cri, &tree).route_ids,
            ["a_b_1", "b_d_1", "d_e_1"]
        );
        assert_trail_matches(&refs, &tree, "albero");

        // Triangolo (9) contro una catena più corta (6): vince il triangolo.
        let two = route_map(vec![
            make_route("a_b_1", "a", "b", 3),
            make_route("b_c_1", "b", "c", 3),
            make_route("a_c_1", "a", "c", 3),
            make_route("x_y_1", "x", "y", 6),
        ]);
        let claimed = claim_all(&two);
        let cri: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        assert_eq!(
            longest_path_trail(&cri, &two).route_ids,
            ["a_b_1", "a_c_1", "b_c_1"]
        );
        assert_trail_matches(&claimed, &two, "triangolo");

        for n in [4, 7, 8] {
            let routes = complete_graph(n);
            assert_trail_matches(&claim_all(&routes), &routes, &format!("K{n}"));
        }
        assert_eq!(
            longest_path_trail(&[], &tree).route_ids,
            Vec::<String>::new()
        );
    }

    fn candidates(city: &str, options: &[(&str, &str)]) -> StationCandidates {
        StationCandidates {
            city: city.into(),
            options: options
                .iter()
                .map(|(route_id, other)| StationOption {
                    route_id: (*route_id).into(),
                    other_city: (*other).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn station_assignment_picks_the_first_maximum_in_canonical_order() {
        let mut own = UnionFind::new();
        own.union("a", "m");
        let station = candidates("s", &[("r1", "z"), ("r2", "m"), ("r3", "m")]);

        // A–S si completa solo con una rotta verso M: la prima in ordine di id.
        let eval = evaluate_tickets(
            &mut own.deep_clone(),
            &[TicketSpec::single("a", "s", 8)],
            std::slice::from_ref(&station),
        );
        assert_eq!(eval.completed, [true]);
        assert_eq!(eval.borrowed, [("s".to_string(), "r2".to_string())]);

        // Già completo senza stazione: "nessuna rotta" vince a parità.
        let eval = evaluate_tickets(
            &mut own.deep_clone(),
            &[TicketSpec::single("a", "m", 8)],
            std::slice::from_ref(&station),
        );
        assert_eq!(eval.completed, [true]);
        assert!(eval.borrowed.is_empty());

        // Con più biglietti vince il netto migliore, non il numero di completati.
        let tickets = [
            TicketSpec::single("a", "s", 3),
            TicketSpec::single("s", "z", 5),
        ];
        let eval = evaluate_tickets(&mut own.deep_clone(), &tickets, &[station]);
        assert_eq!(eval.completed, [false, true]);
        assert_eq!(eval.borrowed, [("s".to_string(), "r1".to_string())]);
        assert_eq!((eval.completed_points, eval.failed_points), (5, -3));
    }

    #[test]
    fn each_station_borrows_at_most_one_route() {
        let mut own = UnionFind::new();
        own.union("a", "m");
        let station = candidates("s", &[("r1", "a"), ("r2", "b")]);
        // S–A e S–B richiedono due rotte diverse: una stazione ne prende una sola.
        let tickets = [
            TicketSpec::single("s", "a", 4),
            TicketSpec::single("s", "b", 5),
        ];
        let eval = evaluate_tickets(&mut own, &tickets, &[station]);
        assert_eq!(eval.completed, [false, true]);
        assert_eq!(eval.borrowed, [("s".to_string(), "r2".to_string())]);
    }

    fn multi(from: &'static str, destinations: &[(&'static str, u32)]) -> TicketSpec<'static> {
        TicketSpec {
            from,
            destinations: destinations.to_vec(),
        }
    }

    /// Biglietto multi-destinazione: nessuna collegata = perde il valore più basso; una =
    /// i suoi punti; più d'una = il massimo (anche se non è la prima nell'elenco).
    #[test]
    fn multi_destination_ticket_scores_best_connected_or_loses_the_lowest() {
        let ticket = multi("a", &[("b", 5), ("c", 13), ("d", 8)]);
        let eval = |edges: &[(&str, &str)]| {
            let mut uf = UnionFind::new();
            for (x, y) in edges {
                uf.union(x, y);
            }
            evaluate_tickets(&mut uf, std::slice::from_ref(&ticket), &[])
        };

        let none = eval(&[("a", "z")]);
        assert_eq!(none.completed, [false]);
        assert_eq!(none.reached, [None]);
        assert_eq!((none.completed_points, none.failed_points), (0, -5));
        assert_eq!(none.completed_count, 0);

        let one = eval(&[("a", "d")]);
        assert_eq!(one.completed, [true]);
        assert_eq!(one.reached, [Some(2)]);
        assert_eq!((one.completed_points, one.failed_points), (8, 0));
        assert_eq!(one.completed_count, 1);

        let several = eval(&[("a", "b"), ("b", "c"), ("c", "d")]);
        assert_eq!(several.reached, [Some(1)]);
        assert_eq!((several.completed_points, several.failed_points), (13, 0));
        assert_eq!(several.completed_count, 1);

        // A pari punti vince la prima destinazione dell'elenco.
        let tied = multi("a", &[("b", 7), ("c", 7)]);
        let mut uf = UnionFind::new();
        uf.union("a", "b");
        uf.union("a", "c");
        let eval = evaluate_tickets(&mut uf, &[tied], &[]);
        assert_eq!(eval.reached, [Some(0)]);
    }

    /// Con i Paesi (vicoli ciechi) come partenza e come destinazioni: una rotta che entra
    /// nel Paese basta, ma due rotte verso lo stesso Paese non si collegano tra loro.
    #[test]
    fn multi_destination_ticket_with_countries() {
        let countries: BTreeSet<CityId> = ["fr", "de", "it"].iter().map(|s| (*s).into()).collect();
        let mut conn = PlayerConnectivity::with_countries(1, countries);
        let ticket = multi("fr", &[("de", 5), ("it", 13)]);
        let eval = |conn: &mut PlayerConnectivity| {
            let uf = conn.uf_mut(0).unwrap();
            evaluate_tickets(uf, std::slice::from_ref(&ticket), &[])
        };

        // Solo la rotta verso la Francia: nessuna destinazione collegata.
        conn.add_route(0, "r1", "a", "fr");
        let eval0 = eval(&mut conn);
        assert_eq!((eval0.completed_points, eval0.failed_points), (0, -5));

        // La Germania entra nella stessa rete: 5 punti.
        conn.add_route(0, "r2", "a", "de");
        let eval1 = eval(&mut conn);
        assert_eq!(eval1.reached, [Some(0)]);
        assert_eq!((eval1.completed_points, eval1.failed_points), (5, 0));

        // L'Italia è raggiunta da una rete separata: non cambia nulla.
        conn.add_route(0, "r3", "b", "it");
        assert_eq!(eval(&mut conn).reached, [Some(0)]);

        // Collegando le due reti vale la destinazione migliore.
        conn.add_route(0, "r4", "a", "b");
        let eval2 = eval(&mut conn);
        assert_eq!(eval2.reached, [Some(1)]);
        assert_eq!((eval2.completed_points, eval2.failed_points), (13, 0));

        // Partenza città e destinazioni Paesi; due rotte verso lo stesso Paese non si collegano.
        let mut conn = PlayerConnectivity::with_countries(
            1,
            ["fr", "de"].iter().map(|s| (*s).into()).collect(),
        );
        conn.add_route(0, "x1", "a", "fr");
        conn.add_route(0, "x2", "b", "fr");
        let uf = conn.uf_mut(0).unwrap();
        let to_fr = multi("a", &[("fr", 4), ("de", 9)]);
        let between = multi("a", &[("b", 3), ("c", 6)]);
        let eval = evaluate_tickets(uf, &[to_fr, between], &[]);
        assert_eq!(eval.completed, [true, false]);
        assert_eq!((eval.completed_points, eval.failed_points), (4, -3));
    }

    /// Riferimento: la ricerca esaustiva su tutte le opzioni, senza potatura.
    fn evaluate_tickets_brute_force(
        own: &mut UnionFind,
        tickets: &[TicketSpec],
        stations: &[StationCandidates],
    ) -> TicketEvaluation {
        if stations.is_empty() {
            return score_tickets(own, tickets);
        }
        let mut best: Option<TicketEvaluation> = None;
        let mut picks: Vec<Option<usize>> = Vec::with_capacity(stations.len());
        search_stations(own, tickets, stations, &mut picks, &mut best);
        best.expect("la ricerca visita almeno l'assegnazione vuota")
    }

    const PRUNE_CITIES: usize = 9;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

        /// La potatura delle opzioni equivalenti non cambia mai l'esito (completati,
        /// punti, rotte prese in prestito) rispetto alla ricerca esaustiva.
        #[test]
        fn pruned_station_search_matches_brute_force(
            own_edges in proptest::collection::vec((0..PRUNE_CITIES, 0..PRUNE_CITIES), 0..7),
            station_cities in Just((0..PRUNE_CITIES).collect::<Vec<_>>())
                .prop_shuffle()
                .prop_map(|v| v.into_iter().take(3).collect::<Vec<_>>()),
            num_stations in 1usize..=3,
            option_ends in proptest::collection::vec(
                proptest::collection::vec(0..PRUNE_CITIES, 0..=8),
                3,
            ),
            raw_tickets in proptest::collection::vec(
                (0..PRUNE_CITIES, 0..PRUNE_CITIES, 1u32..=20),
                1..=10,
            ),
        ) {
            let city = |i: usize| format!("c{i}");
            let mut own = UnionFind::new();
            for &(a, b) in &own_edges {
                own.union(&city(a), &city(b));
            }
            let stations: Vec<StationCandidates> = station_cities
                .iter()
                .zip(&option_ends)
                .take(num_stations)
                .map(|(&s, ends)| StationCandidates {
                    city: city(s),
                    // Rotte per id crescente; l'estremo opposto non è la città stessa.
                    options: ends
                        .iter()
                        .filter(|&&e| e != s)
                        .enumerate()
                        .map(|(k, &e)| StationOption {
                            route_id: format!("r{s}_{k:02}"),
                            other_city: city(e),
                        })
                        .collect(),
                })
                .collect();
            let names: Vec<(String, String, u32)> = raw_tickets
                .iter()
                .map(|&(a, b, p)| (city(a), city(b), p))
                .collect();
            let tickets: Vec<TicketSpec> = names
                .iter()
                .map(|(a, b, p)| TicketSpec::single(a, b, *p))
                .collect();

            let expected = evaluate_tickets_brute_force(&mut own.deep_clone(), &tickets, &stations);
            let pruned = evaluate_tickets(&mut own, &tickets, &stations);
            prop_assert_eq!(pruned, expected);
        }
    }
}
