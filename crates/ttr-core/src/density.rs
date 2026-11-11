//! Upper bound on the cyclomatic number of any component a player can build.
//!
//! A length limit becomes a price `a / b` per unit of length, and the densest route set at
//! that price is a maximum-weight closure found with one min-cut; the best bound over a fixed
//! set of prices holds for every connected group of routes.

use std::collections::BTreeMap;

/// Largest denominator of the prices `a / b` tried (with `0 <= a <= b`).
const MAX_DENOMINATOR: i64 = 12;

/// Capacity of the arcs that force a route's cities into the closure.
const INFINITE: i64 = i64::MAX / 4;

/// A route as `(city index, city index, length)`.
pub(crate) type RouteSpan = (usize, usize, u64);

/// Residual graph of the min-cut (Dinic's algorithm).
struct Flow {
    /// Arc `i` goes to `to[i]`; its reverse is `i ^ 1`.
    to: Vec<usize>,
    cap: Vec<i64>,
    out: Vec<Vec<usize>>,
    level: Vec<i32>,
    next_arc: Vec<usize>,
}

impl Flow {
    fn new(nodes: usize) -> Self {
        Flow {
            to: Vec::new(),
            cap: Vec::new(),
            out: vec![Vec::new(); nodes],
            level: vec![0; nodes],
            next_arc: vec![0; nodes],
        }
    }

    fn add_arc(&mut self, from: usize, to: usize, cap: i64) {
        self.out[from].push(self.to.len());
        self.to.push(to);
        self.cap.push(cap);
        self.out[to].push(self.to.len());
        self.to.push(from);
        self.cap.push(0);
    }

    fn build_levels(&mut self, source: usize, sink: usize) -> bool {
        self.level.fill(-1);
        self.level[source] = 0;
        let mut queue = vec![source];
        let mut head = 0;
        while head < queue.len() {
            let node = queue[head];
            head += 1;
            for &arc in &self.out[node] {
                let next = self.to[arc];
                if self.cap[arc] > 0 && self.level[next] < 0 {
                    self.level[next] = self.level[node] + 1;
                    queue.push(next);
                }
            }
        }
        self.level[sink] >= 0
    }

    fn push(&mut self, node: usize, sink: usize, limit: i64) -> i64 {
        if node == sink {
            return limit;
        }
        while self.next_arc[node] < self.out[node].len() {
            let arc = self.out[node][self.next_arc[node]];
            let next = self.to[arc];
            if self.cap[arc] > 0 && self.level[next] == self.level[node] + 1 {
                let sent = self.push(next, sink, limit.min(self.cap[arc]));
                if sent > 0 {
                    self.cap[arc] -= sent;
                    self.cap[arc ^ 1] += sent;
                    return sent;
                }
            }
            self.next_arc[node] += 1;
        }
        0
    }

    fn max_flow(&mut self, source: usize, sink: usize) -> i64 {
        let mut total = 0;
        while self.build_levels(source, sink) {
            self.next_arc.fill(0);
            loop {
                let sent = self.push(source, sink, INFINITE);
                if sent == 0 {
                    break;
                }
                total += sent;
            }
        }
        total
    }
}

/// Bound `floor((closure + a * trains) / b) + 1` on the cyclomatic number of a connected set
/// of routes of total length at most `trains`, for the price `a / b` per unit of length.
fn bound_for(routes: &[RouteSpan], trains: u64, a: i64, b: i64) -> u64 {
    let mut city_node: BTreeMap<usize, usize> = BTreeMap::new();
    let (source, sink) = (0, 1);
    let mut flow = Flow::new(2);
    let mut profit_total = 0i64;
    for &(from, to, len) in routes {
        let len = i64::try_from(len).unwrap_or(i64::MAX);
        let profit = b.saturating_sub(a.saturating_mul(len));
        if profit <= 0 {
            continue;
        }
        profit_total += profit;
        let route = flow.out.len();
        flow.out.push(Vec::new());
        flow.level.push(0);
        flow.next_arc.push(0);
        flow.add_arc(source, route, profit);
        for city in [from, to] {
            let node = *city_node.entry(city).or_insert_with(|| {
                let node = flow.out.len();
                flow.out.push(Vec::new());
                flow.level.push(0);
                flow.next_arc.push(0);
                flow.add_arc(node, sink, b);
                node
            });
            flow.add_arc(route, node, INFINITE);
        }
    }
    let closure = profit_total - flow.max_flow(source, sink);
    let budget = i128::from(a) * i128::from(trains);
    let bound = (i128::from(closure) + budget) / i128::from(b) + 1;
    u64::try_from(bound).unwrap_or(u64::MAX)
}

/// Upper bound on the cyclomatic number (routes minus cities plus one) of a connected set of
/// routes of total length at most `trains`; a player's component lies inside one connected
/// group of `routes`, so the worst group counts.
pub(crate) fn max_cyclomatic_number(routes: &[RouteSpan], cities: usize, trains: u64) -> u64 {
    let mut parent: Vec<usize> = (0..cities).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(from, to, _) in routes {
        let (a, b) = (root(&mut parent, from), root(&mut parent, to));
        parent[a] = b;
    }
    let mut groups: BTreeMap<usize, Vec<RouteSpan>> = BTreeMap::new();
    for &route in routes {
        groups
            .entry(root(&mut parent, route.0))
            .or_default()
            .push(route);
    }

    let mut worst = 0;
    for group in groups.values() {
        let mut best = u64::MAX;
        for b in 1..=MAX_DENOMINATOR {
            for a in 0..=b {
                best = best.min(bound_for(group, trains, a, b));
            }
        }
        worst = worst.max(best);
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(n: usize) -> Vec<RouteSpan> {
        (0..n)
            .flat_map(|a| ((a + 1)..n).map(move |b| (a, b, 1)))
            .collect()
    }

    #[test]
    fn bound_covers_complete_graphs_that_the_trains_can_build() {
        // All 45 routes of K10 fit in 45 trains: cyclomatic number 45 - 10 + 1.
        assert!(max_cyclomatic_number(&complete(10), 10, 45) >= 36);
        // With 12 trains at most 12 routes fit; K5 (10 routes) has cyclomatic 6.
        assert!(max_cyclomatic_number(&complete(5), 5, 12) >= 6);
    }

    #[test]
    fn bound_is_per_connected_group() {
        let single = complete(6);
        let mut two = single.clone();
        two.extend(complete(6).into_iter().map(|(a, b, l)| (a + 6, b + 6, l)));
        assert_eq!(
            max_cyclomatic_number(&two, 12, 20),
            max_cyclomatic_number(&single, 6, 20)
        );
    }

    #[test]
    fn trees_and_empty_maps_have_a_trivial_bound() {
        assert_eq!(max_cyclomatic_number(&[], 0, 45), 0);
        let path: Vec<RouteSpan> = (0..9).map(|i| (i, i + 1, 2)).collect();
        assert!(max_cyclomatic_number(&path, 10, 45) <= 1);
    }

    #[test]
    fn longer_routes_lower_the_bound() {
        let short: Vec<RouteSpan> = complete(9);
        let long: Vec<RouteSpan> = short.iter().map(|&(a, b, _)| (a, b, 3)).collect();
        assert!(max_cyclomatic_number(&long, 9, 45) < max_cyclomatic_number(&short, 9, 45));
    }
}
