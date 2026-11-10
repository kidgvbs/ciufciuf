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

/// Massimo di stati `(città, rotte usate)` memorizzati per componente.
const MAX_MEMO_STATES: usize = 2_000_000;

/// Massimo di nodi visitati senza memo, sommato su tutte le componenti.
const MAX_SEARCH_NODES: u64 = 20_000_000;

/// Massimo di rotte per componente per cui la maschera di rotte usate sta in un `u64`.
const MAX_MEMO_EDGES: usize = 64;

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

/// Hasher moltiplicativo per le chiavi del memo, che non sono controllate da un avversario.
#[derive(Default)]
struct MemoHasher(u64);

impl Hasher for MemoHasher {
    fn finish(&self) -> u64 {
        self.0
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
/// Ricerca esaustiva del cammino più lungo (trail) su una componente ciclica.
struct TrailSearch<'a> {
    adj: &'a [Vec<Arc>],
    used: Vec<bool>,
    memo: MemoMap,
    memo_enabled: bool,
    component_total: u32,
    nodes: u64,
    best: u32,
    stopped: bool,
}

impl TrailSearch<'_> {
    /// Cerca da ogni città in `starts`, aggiornando `best`.
    fn run_component(&mut self, starts: &[usize], edge_count: usize, total_len: u32) {
        self.memo.clear();
        self.memo_enabled = edge_count <= MAX_MEMO_EDGES;
        self.component_total = total_len;
        self.stopped = self.nodes > MAX_SEARCH_NODES;
        for &start in starts {
            if self.stopped {
                break;
            }
            self.dfs(start, 0, 0, total_len);
        }
    }

    /// Lunghezza massima ancora percorribile da `(city, mask)`.
    ///
    /// Con il memo attivo il valore è esatto; senza memo il ramo viene
    /// potato quando non può superare `best`, e il valore restituito è solo
    /// un limite inferiore (`best` resta corretto).
    fn dfs(&mut self, city: usize, mask: u64, cur: u32, remaining: u32) -> u32 {
        self.best = self.best.max(cur);
        if self.best >= self.component_total {
            // La componente non può migliorare il risultato.
            self.stopped = true;
        }
        if self.stopped {
            return 0;
        }

        let memoize = self.memo_enabled && self.memo.len() < MAX_MEMO_STATES;
        if memoize {
            if let Some(&rest) = self.memo.get(&(city, mask)) {
                self.best = self.best.max(cur.saturating_add(rest));
                return rest;
            }
        } else {
            self.nodes += 1;
            if self.nodes > MAX_SEARCH_NODES {
                self.stopped = true;
                return 0;
            }
            if cur.saturating_add(remaining) <= self.best {
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
            let bit = if self.memo_enabled {
                1u64 << arc.bit
            } else {
                0
            };
            let sub = self.dfs(
                arc.to,
                mask | bit,
                cur.saturating_add(arc.len),
                remaining.saturating_sub(arc.len),
            );
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

/// Calcola il Longest Continuous Path su un multigrafo di rotte reclamate.
///
/// Il cammino è un trail: nessuna rotta viene riutilizzata, le città sì.
/// Restituisce la somma delle lunghezze delle rotte del trail più lungo.
///
/// Le componenti connesse si risolvono separatamente: un albero con il
/// diametro pesato, le altre con una ricerca esaustiva con memo
/// `(città, rotte usate)` limitato a `MAX_MEMO_STATES` stati e un budget di
/// `MAX_SEARCH_NODES` nodi. Oltre il budget il risultato è la migliore
/// lunghezza trovata, cioè un'approssimazione per difetto (mai raggiunta con
/// i 45 treni di una partita reale).
pub fn longest_continuous_path(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
) -> u32 {
    // Rotte distinte e note alla mappa, nell'ordine di input.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut graph_routes: Vec<&Route> = Vec::new();
    for cri in claimed_routes {
        if seen.insert(cri.route_id.as_str())
            && let Some(route) = routes.get(&cri.route_id)
        {
            graph_routes.push(route);
        }
    }

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
        stopped: false,
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
    search.best
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
                    scores[pi].completed_tickets += points_to_score(ticket.points);
                    scores[pi].completed_count += 1;
                } else {
                    scores[pi].failed_tickets -= points_to_score(ticket.points);
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

    /// Limite di tempo per K7 e K8: 2 s in release; il codice di debug è molto più lento.
    #[test]
    fn longest_path_dense_graph_is_bounded() {
        let limit = if cfg!(debug_assertions) {
            std::time::Duration::from_secs(30)
        } else {
            std::time::Duration::from_secs(2)
        };
        for (n, expected_at_least) in [(7usize, 21u32), (8, 24)] {
            let routes = complete_graph(n);
            let claimed = claim_all(&routes);
            let started = std::time::Instant::now();
            let result = longest(&claimed, &routes);
            let elapsed = started.elapsed();
            assert!(elapsed < limit, "K{n}: {elapsed:?}");
            assert!(
                result >= expected_at_least,
                "K{n}: trovato {result}, atteso almeno {expected_at_least}"
            );
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
