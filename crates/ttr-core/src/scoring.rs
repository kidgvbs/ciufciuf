//! Algoritmi di scoring: Union-Find per giocatore, Longest Continuous Path.
//!
//! Piano §9.1 (Union-Find), §9.3 (Longest path DFS con backtracking).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{BuildHasherDefault, Hasher};

use crate::types::{ClaimedRouteInfo, Route, RouteId, TicketDef, points_to_score};

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
        self.find(a) == self.find(b)
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
}

impl PlayerConnectivity {
    pub fn new(num_players: usize) -> Self {
        Self {
            ufs: vec![UnionFind::new(); num_players],
        }
    }

    /// Aggiunge una rotta reclamata alla connettività del giocatore.
    pub fn add_route(&mut self, player_index: usize, from: &str, to: &str) {
        if player_index < self.ufs.len() {
            self.ufs[player_index].union(from, to);
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
}

// ── Longest Continuous Path ──────────────────────────────────────────

/// Max `(city, used routes)` states memoized per component.
const MAX_MEMO_STATES: usize = 2_000_000;

/// Max nodes visited without memo, summed over all components.
pub(crate) const MAX_SEARCH_NODES: u64 = 20_000_000;

/// Max routes per component for which the used-route mask fits in a `u64`.
const MAX_MEMO_EDGES: usize = 64;

/// Max cyclomatic number (routes minus cities plus one) that map validation admits for a
/// component a player can build; random dense graphs up to 14 were always solved exactly.
pub(crate) const MAX_CYCLOMATIC_NUMBER: u32 = 20;

/// Lato orientato del grafo delle rotte reclamate.
#[derive(Debug, Clone, Copy)]
struct Arc {
    to: usize,
    /// Indice globale della rotta.
    edge: usize,
    /// Posizione della rotta nella sua componente (bit della maschera).
    bit: usize,
    len: u32,
}

/// Hasher for the memo keys: a multiply-xorshift mix, then a splitmix64 finalizer so the
/// low bits that hashbrown uses for buckets depend on every input bit.
#[derive(Default)]
struct MemoHasher(u64);

impl Hasher for MemoHasher {
    fn finish(&self) -> u64 {
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

type MemoMap = HashMap<(usize, u64), u32, BuildHasherDefault<MemoHasher>>;
/// Exhaustive search for the longest trail on a cyclic component.
struct TrailSearch<'a> {
    adj: &'a [Vec<Arc>],
    used: Vec<bool>,
    memo: MemoMap,
    memo_enabled: bool,
    component_total: u32,
    nodes: u64,
    /// Longest trail found so far over all components.
    best: u32,
    /// Trail length the current pass looks for; shorter branches are pruned.
    goal: u32,
    /// Longest trail length seen in the current component (never below `best`).
    reached: u32,
    stopped: bool,
    /// Unused routes still incident to each city (a self-loop counts twice).
    unused_degree: Vec<usize>,
    /// Cities with an odd `unused_degree` in the current component.
    odd_cities: u32,
    /// `skip_cost[j]`: total length of the `j` shortest routes of the current component.
    skip_cost: Vec<u32>,
    /// Set when the node budget ended the search before it was complete.
    exhausted: bool,
}

impl TrailSearch<'_> {
    /// Finds the longest trail of the component made of `starts`, raising `best`.
    ///
    /// Passes look for a trail of `goal` length, from the parity upper bound down: the first
    /// pass that finds one proves it is the maximum, and each pass is pruned hard by `goal`.
    fn run_component(&mut self, starts: &[usize], edge_count: usize, total_len: u32) {
        self.memo_enabled = edge_count <= MAX_MEMO_EDGES;
        self.component_total = total_len;
        self.odd_cities = 0;
        let mut lens: Vec<u32> = Vec::with_capacity(edge_count);
        for &city in starts {
            let degree = self.adj[city].len();
            self.unused_degree[city] = degree;
            self.odd_cities += u32::from(degree % 2 == 1);
            lens.extend(
                self.adj[city]
                    .iter()
                    .filter(|a| a.to >= city)
                    .map(|a| a.len),
            );
        }
        lens.sort_unstable();
        self.skip_cost.clear();
        self.skip_cost.push(0);
        let mut sum = 0u32;
        for len in lens {
            sum = sum.saturating_add(len);
            self.skip_cost.push(sum);
        }

        self.reached = self.best;
        let mut goal = self.length_bound();
        while goal > self.best {
            self.goal = goal;
            self.memo.clear();
            self.stopped = self.nodes > MAX_SEARCH_NODES;
            self.exhausted |= self.stopped;
            for &start in starts {
                if self.stopped {
                    break;
                }
                self.dfs(start, 0, 0);
            }
            if self.reached >= goal || self.exhausted {
                break;
            }
            goal -= 1;
        }
        self.best = self.best.max(self.reached);
    }

    /// Upper bound on the length of any trail through the component given the routes used
    /// so far: each unused route changes the parity of two cities, and a trail leaves at
    /// most two cities with odd unused degree, so at least `(odd - 2) / 2` unused routes
    /// are never taken.
    fn length_bound(&self) -> u32 {
        let skipped = self.odd_cities.saturating_sub(2).div_ceil(2) as usize;
        let last = self.skip_cost.len() - 1;
        self.component_total
            .saturating_sub(self.skip_cost[skipped.min(last)])
    }

    /// Marks one more route end at `city` as used (`delta = -1`) or free (`delta = 1`).
    fn shift_degree(&mut self, city: usize, delta: isize) {
        let degree = &mut self.unused_degree[city];
        *degree = degree.wrapping_add_signed(delta);
        // The parity just flipped: an odd degree adds an odd city, an even one removes it.
        if *degree % 2 == 1 {
            self.odd_cities += 1;
        } else {
            self.odd_cities -= 1;
        }
    }

    /// Lunghezza massima ancora percorribile da `(city, mask)`.
    ///
    /// Un ramo che non può raggiungere `goal` viene potato e il valore
    /// restituito è solo un limite inferiore: `reached` resta corretto.
    fn dfs(&mut self, city: usize, mask: u64, cur: u32) -> u32 {
        self.reached = self.reached.max(cur);
        if self.reached >= self.goal {
            self.stopped = true;
        }
        if self.stopped || self.length_bound() < self.goal {
            return 0;
        }

        let memoize = self.memo_enabled && self.memo.len() < MAX_MEMO_STATES;
        if memoize {
            if let Some(&rest) = self.memo.get(&(city, mask)) {
                self.reached = self.reached.max(cur.saturating_add(rest));
                return rest;
            }
        } else {
            self.nodes += 1;
            if self.nodes > MAX_SEARCH_NODES {
                self.stopped = true;
                self.exhausted = true;
                return 0;
            }
        }

        let mut rest_best = 0;
        for i in 0..self.adj[city].len() {
            let arc = self.adj[city][i];
            if self.used[arc.edge] {
                continue;
            }
            self.used[arc.edge] = true;
            self.shift_degree(city, -1);
            self.shift_degree(arc.to, -1);
            let bit = if self.memo_enabled {
                1u64 << arc.bit
            } else {
                0
            };
            let sub = self.dfs(arc.to, mask | bit, cur.saturating_add(arc.len));
            self.shift_degree(arc.to, 1);
            self.shift_degree(city, 1);
            self.used[arc.edge] = false;
            rest_best = rest_best.max(arc.len.saturating_add(sub));
            if self.stopped {
                break;
            }
        }

        if memoize && !self.stopped && self.memo.len() < MAX_MEMO_STATES {
            self.memo.insert((city, mask), rest_best);
        }
        rest_best
    }
}

/// Distanza pesata massima da `start` in un albero, con la città più lontana.
fn farthest_in_tree(adj: &[Vec<Arc>], start: usize) -> (usize, u32) {
    let mut far = (start, 0);
    let mut stack: Vec<(usize, usize, u32)> = vec![(start, usize::MAX, 0)];
    while let Some((city, parent_edge, dist)) = stack.pop() {
        if dist > far.1 {
            far = (city, dist);
        }
        for arc in &adj[city] {
            if arc.edge != parent_edge {
                stack.push((arc.to, arc.edge, dist.saturating_add(arc.len)));
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
/// Tree components use the weighted diameter, the others an exhaustive search memoized on
/// `(city, used routes)`. `Ruleset::validate` bounds the cyclomatic number of any component
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
pub(crate) struct LongestPath {
    pub length: u32,
    /// False when the node budget stopped the search: `length` is then a lower bound.
    pub exact: bool,
}

/// Longest path with the exactness flag; the search order depends only on route and city
/// ids, never on the order of `claimed_routes`.
pub(crate) fn longest_path_report(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
) -> LongestPath {
    let mut by_id: BTreeMap<&str, &Route> = BTreeMap::new();
    for cri in claimed_routes {
        if let Some(route) = routes.get(&cri.route_id) {
            by_id.insert(cri.route_id.as_str(), route);
        }
    }
    let graph_routes: Vec<&Route> = by_id.into_values().collect();

    let cities: BTreeSet<&str> = graph_routes
        .iter()
        .flat_map(|r| [r.from.as_str(), r.to.as_str()])
        .collect();
    let city_index: BTreeMap<&str, usize> =
        cities.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let city_count = city_index.len();

    let endpoints: Vec<(usize, usize)> = graph_routes
        .iter()
        .map(|r| (city_index[r.from.as_str()], city_index[r.to.as_str()]))
        .collect();

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
    let mut bit_of_edge = vec![0usize; graph_routes.len()];
    for (edge, &(a, _)) in endpoints.iter().enumerate() {
        let edges = &mut component_edges[root_of(&mut parent, a)];
        bit_of_edge[edge] = edges.len();
        edges.push(edge);
    }

    let mut adj: Vec<Vec<Arc>> = vec![Vec::new(); city_count];
    for (edge, (&(a, b), route)) in endpoints.iter().zip(&graph_routes).enumerate() {
        let bit = bit_of_edge[edge];
        adj[a].push(Arc {
            to: b,
            edge,
            bit,
            len: route.len,
        });
        adj[b].push(Arc {
            to: a,
            edge,
            bit,
            len: route.len,
        });
    }

    let mut search = TrailSearch {
        adj: &adj,
        used: vec![false; graph_routes.len()],
        memo: MemoMap::default(),
        memo_enabled: false,
        component_total: 0,
        nodes: 0,
        best: 0,
        goal: 0,
        reached: 0,
        stopped: false,
        unused_degree: vec![0; city_count],
        odd_cities: 0,
        skip_cost: Vec::new(),
        exhausted: false,
    };
    for (root, edges) in component_edges.iter().enumerate() {
        if edges.is_empty() {
            continue;
        }
        let cities_here = &component_cities[root];
        let total_len = edges
            .iter()
            .fold(0u32, |acc, &e| acc.saturating_add(graph_routes[e].len));
        if edges.len() + 1 == cities_here.len() {
            let (end, _) = farthest_in_tree(&adj, cities_here[0]);
            let (_, diameter) = farthest_in_tree(&adj, end);
            search.best = search.best.max(diameter);
        } else {
            search.run_component(cities_here, edges.len(), total_len);
        }
    }
    LongestPath {
        length: search.best,
        exact: !search.exhausted,
    }
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
}

/// Calcola il punteggio finale per tutti i giocatori.
///
/// - Somma punti rotte (già accumulati in `player.score`)
/// - Biglietti: ±valore in base alla connettività Union-Find
/// - Longest Continuous Path: +bonus a chi ha il percorso più lungo
///   (parità → bonus a tutti i pari merito)
pub fn compute_final_scores(
    claimed_routes: &[Vec<ClaimedRouteInfo>], // per giocatore
    routes_map: &BTreeMap<RouteId, Route>,
    tickets: &[TicketDef],
    player_tickets: &[BTreeMap<String, bool>], // ticket_id → kept
    connectivity: &mut PlayerConnectivity,
    bonus_points: u32,
) -> (Vec<PlayerScore>, Vec<usize>, u32) {
    let num_players = claimed_routes.len();
    let mut scores: Vec<PlayerScore> = (0..num_players)
        .map(|_| PlayerScore {
            route_points: 0,
            completed_tickets: 0,
            completed_count: 0,
            failed_tickets: 0,
            longest_path: 0,
        })
        .collect();

    // Calcola longest path per ogni giocatore
    let mut longest_paths: Vec<u32> = vec![0; num_players];
    for (pi, routes) in claimed_routes.iter().enumerate() {
        let cri_refs: Vec<&ClaimedRouteInfo> = routes.iter().collect();
        longest_paths[pi] = longest_continuous_path(&cri_refs, routes_map);
        scores[pi].longest_path = longest_paths[pi];
    }

    // Determina il massimo
    let max_path = longest_paths.iter().copied().max().unwrap_or(0);

    // Chi ha il max (o pari merito) prende il bonus
    let bonus_winners: Vec<usize> = if max_path > 0 {
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
        for tid in pt.keys() {
            if let Some(ticket) = tickets.iter().find(|t| &t.id == tid) {
                let completed = connectivity.ticket_completed(pi, &ticket.from, &ticket.to);
                if completed {
                    scores[pi].completed_tickets = scores[pi]
                        .completed_tickets
                        .saturating_add(points_to_score(ticket.points));
                    scores[pi].completed_count += 1;
                } else {
                    scores[pi].failed_tickets = scores[pi]
                        .failed_tickets
                        .saturating_sub(points_to_score(ticket.points));
                }
            }
        }
    }

    (scores, bonus_winners, bonus_points)
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
        }
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

    /// Dense graphs the relabeling test builds: the range the search always solves exactly.
    const RELABELING_MAX_CYCLOMATIC_NUMBER: u32 = 14;

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
            prop_assume!(cyclomatic(n, &keep[..]) <= RELABELING_MAX_CYCLOMATIC_NUMBER);
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
}
