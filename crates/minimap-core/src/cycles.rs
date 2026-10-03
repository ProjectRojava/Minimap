//! Cycle detection for graphs that must stay acyclic (`blocks`, `depends_on`,
//! `reports_to`, team nesting).

use std::collections::{HashMap, VecDeque};

use minimap_types::Uuid;
use petgraph::graphmap::DiGraphMap;

/// Would adding the edge `from -> to` close a loop?
///
/// `edges` are the existing `(from, to)` pairs. Returns the loop as
/// `[from, to, ..., from]` (so it reads as a path and starts and ends on the same node),
/// or `None` if the edge is safe. A self-edge is the loop `[x, x]`.
pub fn find_cycle(edges: &[(Uuid, Uuid)], from: Uuid, to: Uuid) -> Option<Vec<Uuid>> {
    if from == to {
        return Some(vec![from, from]);
    }
    let graph: DiGraphMap<Uuid, ()> = edges.iter().copied().collect();
    if !graph.contains_node(to) || !graph.contains_node(from) {
        return None; // an unseen node has no edges, so cannot be part of a loop
    }

    // Breadth-first from `to`; reaching `from` means to ->* from, so from -> to closes a loop.
    let mut parent: HashMap<Uuid, Uuid> = HashMap::new();
    let mut queue = VecDeque::from([to]);
    while let Some(node) = queue.pop_front() {
        for next in graph.neighbors(node) {
            if next == to || parent.contains_key(&next) {
                continue;
            }
            parent.insert(next, node);
            if next == from {
                let mut path = vec![from];
                let mut at = from;
                while at != to {
                    at = parent[&at];
                    path.push(at);
                }
                path.reverse(); // to ... from
                let mut cycle = vec![from];
                cycle.extend(path);
                return Some(cycle);
            }
            queue.push_back(next);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn self_edge_is_a_loop() {
        assert_eq!(find_cycle(&[], id(1), id(1)), Some(vec![id(1), id(1)]));
    }

    #[test]
    fn two_node_loop() {
        // 2 -> 1 exists; adding 1 -> 2 closes 1 -> 2 -> 1.
        assert_eq!(
            find_cycle(&[(id(2), id(1))], id(1), id(2)),
            Some(vec![id(1), id(2), id(1)])
        );
    }

    #[test]
    fn reports_the_full_path() {
        // 2 -> 3 -> 4 -> 1 exists; adding 1 -> 2 loops through all of them.
        let edges = [(id(2), id(3)), (id(3), id(4)), (id(4), id(1))];
        assert_eq!(
            find_cycle(&edges, id(1), id(2)),
            Some(vec![id(1), id(2), id(3), id(4), id(1)])
        );
    }

    #[test]
    fn diamonds_and_forward_edges_are_fine() {
        let edges = [
            (id(1), id(2)),
            (id(1), id(3)),
            (id(2), id(4)),
            (id(3), id(4)),
        ];
        assert_eq!(find_cycle(&edges, id(1), id(4)), None); // shortcut, not a loop
        assert_eq!(find_cycle(&edges, id(5), id(1)), None); // new node
        assert_eq!(find_cycle(&[], id(1), id(2)), None);
    }

    #[test]
    fn picks_a_real_path_among_branches() {
        // to=2 reaches from=5 only via 2 -> 4 -> 5, not via the dead end 2 -> 3.
        let edges = [(id(2), id(3)), (id(2), id(4)), (id(4), id(5))];
        assert_eq!(
            find_cycle(&edges, id(5), id(2)),
            Some(vec![id(5), id(2), id(4), id(5)])
        );
    }

    proptest! {
        /// Whatever order edges are offered in, accepted edges never form a loop,
        /// and every reported cycle is a genuine closed path.
        #[test]
        fn accepted_inserts_stay_acyclic(pairs in proptest::collection::vec((0u128..8, 0u128..8), 0..60)) {
            let mut accepted: Vec<(Uuid, Uuid)> = Vec::new();
            for (a, b) in pairs {
                let (a, b) = (id(a), id(b));
                match find_cycle(&accepted, a, b) {
                    None => accepted.push((a, b)),
                    Some(cycle) => {
                        prop_assert_eq!(cycle.first(), cycle.last());
                        prop_assert_eq!(cycle[0], a);
                        prop_assert_eq!(cycle[1], b);
                        // Every hop after the new edge exists.
                        for w in cycle[1..].windows(2) {
                            prop_assert!(accepted.contains(&(w[0], w[1])));
                        }
                    }
                }
            }
            // Acyclic: a full topological sort exists.
            let g: DiGraphMap<Uuid, ()> = accepted.iter().copied().collect();
            prop_assert!(petgraph::algo::toposort(&g, None).is_ok());
        }
    }
}
