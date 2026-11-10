//! Algoritmi di scoring: Union-Find per giocatore, Longest Continuous Path.
//!
//! Piano §9.1 (Union-Find), §9.3 (Longest path DFS con backtracking).

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{ClaimedRouteInfo, Route, RouteId, TicketDef};

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

/// Calcola il Longest Continuous Path su un multigrafo di rotte reclamate.
///
/// Algoritmo: DFS con backtracking (piano §9.3).
/// - Cammino *semplice* sui lati: nessuna rotta riutilizzata.
/// - Città riattraversabili (il cammino è un trail, non un path semplice sui nodi).
///
/// Su un multigrafo, ogni rotta è un arco distinto con un id. Il backtracking
/// può riattraversare una città ma non riusare lo stesso route_id.
///
/// Restituisce la lunghezza massima (somma delle lunghezze delle rotte nel cammino).
pub fn longest_continuous_path(
    claimed_routes: &[&ClaimedRouteInfo],
    routes: &BTreeMap<RouteId, Route>,
) -> u32 {
    if claimed_routes.is_empty() {
        return 0;
    }

    // Costruisci la lista di adiacenza: city → [(neighbor, route_id, len)]
    let mut adj: BTreeMap<&str, Vec<(&str, &str, u32)>> = BTreeMap::new();
    for cri in claimed_routes {
        if let Some(route) = routes.get(&cri.route_id) {
            adj.entry(&route.from)
                .or_default()
                .push((&route.to, &cri.route_id, route.len));
            adj.entry(&route.to)
                .or_default()
                .push((&route.from, &cri.route_id, route.len));
        }
    }

    // Backtracking DFS: stato = (città corrente, insieme route_id usate)
    let mut best: u32 = 0;
    let mut used: BTreeSet<&str> = BTreeSet::new();

    // Per ogni città di partenza...
    for &start_city in adj.keys() {
        dfs_longest(start_city, 0, &adj, &mut used, &mut best);
    }

    best
}

fn dfs_longest<'a>(
    city: &'a str,
    current_len: u32,
    adj: &BTreeMap<&'a str, Vec<(&'a str, &'a str, u32)>>,
    used: &mut BTreeSet<&'a str>,
    best: &mut u32,
) {
    if current_len > *best {
        *best = current_len;
    }

    if let Some(neighbors) = adj.get(city) {
        for &(next_city, route_id, len) in neighbors {
            if used.contains(route_id) {
                continue;
            }
            used.insert(route_id);
            dfs_longest(next_city, current_len + len, adj, used, best);
            used.remove(route_id);
        }
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
                    scores[pi].completed_tickets += ticket.points as i32;
                    scores[pi].completed_count += 1;
                } else {
                    scores[pi].failed_tickets -= ticket.points as i32;
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
}
