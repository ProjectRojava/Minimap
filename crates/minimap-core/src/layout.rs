//! Layered left-to-right graph layout (Sugiyama-style). Pure, generic over what the nodes are.
//!
//! 1. **Layers**: a node goes one column right of its furthest-left predecessor (longest path);
//!    nodes with no predecessors are then pulled right next to their earliest successor so
//!    links stay short.
//! 2. **Virtual nodes** reserve a lane in every column an edge crosses, so a long edge is
//!    routed between the boxes rather than through them.
//! 3. **Crossing reduction**: alternating barycentre sweeps reorder each column by the average
//!    position of its neighbours; the arrangement with the fewest crossings is kept.
//! 4. **Coordinates**: with the order fixed, each column's vertical positions are the
//!    least-squares fit to its neighbours' centres that keeps every gap (pool-adjacent-violators),
//!    refined over a few alternating passes.
//!
//! Boxes never overlap: boxes in a column keep at least `node_gap` between them and columns
//! occupy disjoint horizontal ranges.

use std::collections::{HashSet, VecDeque};

/// Height of the lane reserved for an edge passing through a column, and the gap kept around it.
const LANE_H: f64 = 2.0;
const LANE_GAP: f64 = 6.0;
/// Barycentre sweeps (each is one pass over all columns in one direction).
const SWEEPS: usize = 24;
/// Coordinate refinement passes.
const REFINE_PASSES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Horizontal space between columns.
    pub layer_gap: f64,
    /// Minimum vertical space between boxes in a column.
    pub node_gap: f64,
    pub margin: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            layer_gap: 90.0,
            node_gap: 14.0,
            margin: 24.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// Top-left corner.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub layer: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub nodes: Vec<Placed>,
    /// One route per input edge: from the source's right edge, through each column in between,
    /// to the target's left edge.
    pub routes: Vec<Vec<(f64, f64)>>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("the links form a loop, so the work can't be laid out left to right")]
    Cycle,
}

/// Extra space (above and below) around boxes that sit inside a cluster, so a frame can be drawn
/// around them without touching the next box.
pub const CLUSTER_GAP: f64 = 30.0;

/// Lays out `sizes.len()` boxes (width, height) joined by directed `edges` (indices).
pub fn layout(
    sizes: &[(f64, f64)],
    edges: &[(usize, usize)],
    params: &Params,
) -> Result<Layout, LayoutError> {
    layout_clustered(sizes, edges, &[], params)
}

/// [`layout`] with clusters: `clusters[i]` is the cluster box `i` belongs to (or `None`; a slice
/// shorter than `sizes` means none). Boxes of one cluster are kept together in each column and
/// given room around them for a frame.
pub fn layout_clustered(
    sizes: &[(f64, f64)],
    edges: &[(usize, usize)],
    clusters: &[Option<usize>],
    params: &Params,
) -> Result<Layout, LayoutError> {
    let n = sizes.len();
    if n == 0 {
        return Ok(Layout {
            nodes: vec![],
            routes: vec![vec![]; edges.len()],
            width: params.margin * 2.0,
            height: params.margin * 2.0,
        });
    }
    // ---- 1. layers
    let mut preds: Vec<Vec<usize>> = vec![vec![]; n];
    let mut succs: Vec<Vec<usize>> = vec![vec![]; n];
    for &(a, b) in edges {
        if a == b {
            return Err(LayoutError::Cycle);
        }
        succs[a].push(b);
        preds[b].push(a);
    }
    let mut indegree: Vec<usize> = preds.iter().map(Vec::len).collect();
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(v) = queue.pop_front() {
        order.push(v);
        for &w in &succs[v] {
            indegree[w] -= 1;
            if indegree[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    if order.len() != n {
        return Err(LayoutError::Cycle);
    }
    let mut layer = vec![0usize; n];
    for &v in &order {
        for &u in &preds[v] {
            layer[v] = layer[v].max(layer[u] + 1);
        }
    }
    // Pull sources next to their nearest successor.
    for &v in order.iter().rev() {
        if preds[v].is_empty() {
            if let Some(m) = succs[v].iter().map(|&w| layer[w]).min() {
                layer[v] = m - 1;
            }
        }
    }
    let layers = layer.iter().copied().max().unwrap_or(0) + 1;

    // ---- 2. virtual nodes
    let mut ext_layer: Vec<usize> = layer.clone();
    let mut ext_h: Vec<f64> = sizes.iter().map(|s| s.1).collect();
    let mut is_lane: Vec<bool> = vec![false; n];
    let mut chains: Vec<Vec<usize>> = Vec::with_capacity(edges.len());
    for &(a, b) in edges {
        let mut chain = vec![a];
        for l in layer[a] + 1..layer[b] {
            ext_layer.push(l);
            ext_h.push(LANE_H);
            is_lane.push(true);
            chain.push(ext_layer.len() - 1);
        }
        chain.push(b);
        chains.push(chain);
    }
    let total = ext_layer.len();
    let mut up: Vec<Vec<usize>> = vec![vec![]; total];
    let mut down: Vec<Vec<usize>> = vec![vec![]; total];
    for chain in &chains {
        for pair in chain.windows(2) {
            down[pair[0]].push(pair[1]);
            up[pair[1]].push(pair[0]);
        }
    }
    let mut columns: Vec<Vec<usize>> = vec![vec![]; layers];
    for v in 0..total {
        columns[ext_layer[v]].push(v);
    }

    // ---- 3. crossing reduction
    let mut pos = vec![0usize; total];
    let set_pos = |columns: &Vec<Vec<usize>>, pos: &mut Vec<usize>| {
        for col in columns {
            for (i, &v) in col.iter().enumerate() {
                pos[v] = i;
            }
        }
    };
    set_pos(&columns, &mut pos);
    let crossings = |columns: &Vec<Vec<usize>>, pos: &Vec<usize>| -> usize {
        let mut total_crossings = 0;
        for column in columns.iter().take(layers.saturating_sub(1)) {
            let mut es: Vec<(usize, usize)> = Vec::new();
            for &u in column {
                for &v in &down[u] {
                    es.push((pos[u], pos[v]));
                }
            }
            for i in 0..es.len() {
                for j in i + 1..es.len() {
                    let (a, b) = (es[i], es[j]);
                    if (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1) {
                        total_crossings += 1;
                    }
                }
            }
        }
        total_crossings
    };
    let mut best = columns.clone();
    let mut best_crossings = crossings(&columns, &pos);
    let mut sweep = 0;
    while sweep < SWEEPS && best_crossings > 0 {
        let downward = sweep % 2 == 0;
        let range: Vec<usize> = if downward {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for l in range {
            let neighbours = if downward { &up } else { &down };
            let mut keyed: Vec<(f64, usize, usize)> = columns[l]
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let ns = &neighbours[v];
                    let bary = if ns.is_empty() {
                        i as f64
                    } else {
                        ns.iter().map(|&u| pos[u] as f64).sum::<f64>() / ns.len() as f64
                    };
                    (bary, i, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            columns[l] = keyed.into_iter().map(|k| k.2).collect();
            for (i, &v) in columns[l].iter().enumerate() {
                pos[v] = i;
            }
        }
        let c = crossings(&columns, &pos);
        if c < best_crossings {
            best_crossings = c;
            best = columns.clone();
        }
        sweep += 1;
    }
    columns = best;
    // Keep each cluster's boxes together (at the place of its first box in the column).
    let cluster_of = |v: usize| -> Option<usize> { clusters.get(v).copied().flatten() };
    for col in &mut columns {
        let mut out = Vec::with_capacity(col.len());
        let mut placed: HashSet<usize> = HashSet::new();
        for &v in col.iter() {
            match cluster_of(v) {
                Some(c) => {
                    if placed.insert(c) {
                        out.extend(col.iter().copied().filter(|&u| cluster_of(u) == Some(c)));
                    }
                }
                None => out.push(v),
            }
        }
        *col = out;
    }
    set_pos(&columns, &mut pos);

    // ---- 4. coordinates
    let gap_between = |a: usize, b: usize| -> f64 {
        let framed = |v: usize| !is_lane[v] && cluster_of(v).is_some();
        if is_lane[a] || is_lane[b] {
            // A lane passing a framed box keeps clear of the frame.
            let other = if is_lane[a] { b } else { a };
            LANE_GAP
                + if framed(other) {
                    CLUSTER_GAP / 3.0
                } else {
                    0.0
                }
        } else if cluster_of(a) == cluster_of(b) {
            params.node_gap
        } else if framed(a) || framed(b) {
            params.node_gap + CLUSTER_GAP
        } else {
            params.node_gap
        }
    };
    let mut y = vec![0.0f64; total];
    for col in &columns {
        let mut cursor = 0.0;
        for (i, &v) in col.iter().enumerate() {
            if i > 0 {
                cursor += gap_between(col[i - 1], v);
            }
            y[v] = cursor;
            cursor += ext_h[v];
        }
    }
    let centre = |y: &Vec<f64>, v: usize| y[v] + ext_h[v] / 2.0;
    for pass in 0..REFINE_PASSES {
        let downward = pass % 2 == 0;
        let range: Vec<usize> = if downward {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for l in range {
            let neighbours = if downward { &up } else { &down };
            let col = &columns[l];
            // Where each box would like to be: level with the average of its neighbours.
            let mut offsets = Vec::with_capacity(col.len());
            let mut cursor = 0.0;
            for (i, &v) in col.iter().enumerate() {
                if i > 0 {
                    cursor += gap_between(col[i - 1], v);
                }
                offsets.push(cursor);
                cursor += ext_h[v];
            }
            let wanted: Vec<f64> = col
                .iter()
                .zip(&offsets)
                .map(|(&v, off)| {
                    let ns = &neighbours[v];
                    let top = if ns.is_empty() {
                        y[v]
                    } else {
                        ns.iter().map(|&u| centre(&y, u)).sum::<f64>() / ns.len() as f64
                            - ext_h[v] / 2.0
                    };
                    top - off
                })
                .collect();
            let fitted = non_decreasing_fit(&wanted);
            for ((&v, off), z) in col.iter().zip(&offsets).zip(fitted) {
                y[v] = z + off;
            }
        }
    }
    let min_y = y.iter().copied().fold(f64::INFINITY, f64::min);
    let shift = params.margin - min_y;
    for v in y.iter_mut() {
        *v += shift;
    }

    // Columns: real boxes are left-aligned at the column's x; lanes run down its middle.
    let mut col_w = vec![0.0f64; layers];
    for v in 0..n {
        col_w[ext_layer[v]] = col_w[ext_layer[v]].max(sizes[v].0);
    }
    let mut col_x = vec![0.0f64; layers];
    let mut cursor = params.margin;
    for l in 0..layers {
        col_x[l] = cursor;
        cursor += col_w[l] + params.layer_gap;
    }
    let width = cursor - params.layer_gap + params.margin;
    let nodes: Vec<Placed> = (0..n)
        .map(|v| Placed {
            x: col_x[ext_layer[v]],
            y: y[v],
            w: sizes[v].0,
            h: sizes[v].1,
            layer: ext_layer[v],
        })
        .collect();
    let routes: Vec<Vec<(f64, f64)>> = chains
        .iter()
        .map(|chain| {
            let first = chain[0];
            let last = chain[chain.len() - 1];
            let mut pts = vec![(
                nodes[first].x + nodes[first].w,
                nodes[first].y + nodes[first].h / 2.0,
            )];
            for &lane in &chain[1..chain.len() - 1] {
                let l = ext_layer[lane];
                pts.push((col_x[l] + col_w[l] / 2.0, y[lane] + LANE_H / 2.0));
            }
            pts.push((nodes[last].x, nodes[last].y + nodes[last].h / 2.0));
            pts
        })
        .collect();
    let bottom = nodes
        .iter()
        .map(|p| p.y + p.h)
        .chain(chains.iter().flatten().map(|&v| y[v] + ext_h[v]))
        .fold(0.0, f64::max);
    Ok(Layout {
        nodes,
        routes,
        width,
        height: bottom + params.margin,
    })
}

/// The non-decreasing sequence closest (least squares) to `values`: pool adjacent violators.
fn non_decreasing_fit(values: &[f64]) -> Vec<f64> {
    // Blocks of (sum, count); merge while a block's mean is below the previous one's.
    let mut blocks: Vec<(f64, usize)> = Vec::with_capacity(values.len());
    for &v in values {
        blocks.push((v, 1));
        while blocks.len() > 1 {
            let (s2, c2) = blocks[blocks.len() - 1];
            let (s1, c1) = blocks[blocks.len() - 2];
            if s1 / c1 as f64 > s2 / c2 as f64 {
                blocks.truncate(blocks.len() - 2);
                blocks.push((s1 + s2, c1 + c2));
            } else {
                break;
            }
        }
    }
    let mut out = Vec::with_capacity(values.len());
    for (s, c) in blocks {
        out.extend(std::iter::repeat_n(s / c as f64, c));
    }
    out
}

/// Number of pairs of edges that cross in a finished layout (straight lines between centres).
/// Used by tests and to compare arrangements.
pub fn crossings_of(layout: &Layout, edges: &[(usize, usize)]) -> usize {
    let segs: Vec<((f64, f64), (f64, f64))> = edges
        .iter()
        .map(|&(a, b)| {
            let (pa, pb) = (layout.nodes[a], layout.nodes[b]);
            ((pa.x + pa.w, pa.y + pa.h / 2.0), (pb.x, pb.y + pb.h / 2.0))
        })
        .collect();
    let mut count = 0;
    for i in 0..segs.len() {
        for j in i + 1..segs.len() {
            let ((a1, a2), (b1, b2)) = (segs[i], segs[j]);
            // Only edges that span the same columns can be compared with a simple test; for
            // others fall back to the segment intersection test.
            if properly_intersect(a1, a2, b1, b2) {
                count += 1;
            }
        }
    }
    count
}

fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn properly_intersect(a1: (f64, f64), a2: (f64, f64), b1: (f64, f64), b2: (f64, f64)) -> bool {
    // Segments sharing an endpoint region (same source or target box) don't count as crossings.
    if a1 == b1 || a2 == b2 {
        return false;
    }
    let d1 = orient(a1, a2, b1);
    let d2 = orient(a1, a2, b2);
    let d3 = orient(b1, b2, a1);
    let d4 = orient(b1, b2, a2);
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const BOX: (f64, f64) = (180.0, 34.0);

    fn lay(n: usize, edges: &[(usize, usize)]) -> Layout {
        layout(&vec![BOX; n], edges, &Params::default()).unwrap()
    }

    fn overlap(a: &Placed, b: &Placed) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    // ------------------------------------------------------------- layers

    #[test]
    fn boxes_of_one_cluster_stay_together_with_room_for_a_frame() {
        // 0 and 2 are one cluster, 1 is not; all independent, so one column.
        let sizes = vec![(100.0, 20.0); 3];
        let l =
            layout_clustered(&sizes, &[], &[Some(0), None, Some(0)], &Params::default()).unwrap();
        let mut ys: Vec<(f64, usize)> = l.nodes.iter().enumerate().map(|(i, p)| (p.y, i)).collect();
        ys.sort_by(|a, b| a.0.total_cmp(&b.0));
        let order: Vec<usize> = ys.iter().map(|y| y.1).collect();
        // The cluster's two boxes are next to each other, whichever side the other box is on.
        let at = |i: usize| order.iter().position(|&x| x == i).unwrap();
        assert_eq!(at(0).abs_diff(at(2)), 1, "{order:?}");
        // Room around the frame: more than the plain gap to the box that is not in the cluster.
        let gap = |a: usize, b: usize| (l.nodes[b].y - (l.nodes[a].y + l.nodes[a].h)).abs();
        let plain = Params::default().node_gap;
        assert!(gap(0, 2).min(gap(2, 0)) <= plain + 1e-9 || order[0] != 0);
        let outside = if at(1) == 0 {
            gap(1, order[1])
        } else {
            gap(order[at(1) - 1], 1)
        };
        assert!(outside >= plain + CLUSTER_GAP - 1e-9, "{outside}");
    }

    #[test]
    fn no_clusters_is_the_plain_layout() {
        let sizes = vec![(100.0, 20.0); 4];
        let edges = [(0, 1), (1, 2), (0, 3)];
        assert_eq!(
            layout(&sizes, &edges, &Params::default()).unwrap(),
            layout_clustered(&sizes, &edges, &[None; 4], &Params::default()).unwrap()
        );
    }

    #[test]
    fn a_chain_runs_left_to_right() {
        let l = lay(3, &[(0, 1), (1, 2)]);
        assert_eq!(
            l.nodes.iter().map(|p| p.layer).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(l.nodes[0].x < l.nodes[1].x && l.nodes[1].x < l.nodes[2].x);
        // A straight chain has no vertical wiggle.
        assert_eq!(l.nodes[0].y, l.nodes[1].y);
        assert_eq!(l.nodes[1].y, l.nodes[2].y);
        assert_eq!(l.routes[0].len(), 2);
    }

    #[test]
    fn a_node_sits_one_column_after_its_furthest_predecessor() {
        // 0 -> 1 -> 3 and 0 -> 3 and 2 -> 3: node 3 is column 2.
        let l = lay(4, &[(0, 1), (1, 3), (0, 3), (2, 3)]);
        assert_eq!(l.nodes[3].layer, 2);
        assert_eq!(l.nodes[1].layer, 1);
        // The extra source is pulled up next to its successor, not left at column 0.
        assert_eq!(l.nodes[2].layer, 1);
    }

    #[test]
    fn long_edges_get_a_lane_in_each_column_they_cross() {
        // 0 -> 1 -> 2 -> 3, plus 0 -> 3 spanning three columns.
        let l = lay(4, &[(0, 1), (1, 2), (2, 3), (0, 3)]);
        let long = &l.routes[3];
        assert_eq!(long.len(), 2 + 2, "start, two lane points, end");
        // The route is monotone left to right and ends on the target's left edge.
        assert!(long.windows(2).all(|p| p[0].0 < p[1].0));
        assert_eq!(long[0].0, l.nodes[0].x + l.nodes[0].w);
        assert_eq!(long.last().unwrap().0, l.nodes[3].x);
        // The lane passes clear of the boxes in the columns it crosses.
        for mid in &long[1..3] {
            for p in &l.nodes {
                let inside =
                    mid.0 >= p.x && mid.0 <= p.x + p.w && mid.1 >= p.y && mid.1 <= p.y + p.h;
                assert!(!inside, "lane point {mid:?} is inside a box");
            }
        }
    }

    #[test]
    fn independent_nodes_share_the_first_column_without_overlapping() {
        let l = lay(5, &[]);
        assert!(l.nodes.iter().all(|p| p.layer == 0));
        for i in 0..5 {
            for j in i + 1..5 {
                assert!(!overlap(&l.nodes[i], &l.nodes[j]));
            }
        }
        assert!(l.width > 0.0 && l.height > 0.0);
    }

    // -------------------------------------------------- crossing reduction

    #[test]
    fn a_crossed_pair_is_uncrossed() {
        // Written so that the natural order crosses: 0->3 and 1->2 with 2 listed before 3.
        let edges = [(0, 3), (1, 2)];
        let l = lay(4, &edges);
        assert_eq!(crossings_of(&l, &edges), 0, "{l:?}");
    }

    #[test]
    fn a_larger_tangle_is_reduced_to_none_when_a_planar_drawing_exists() {
        // Two chains drawn interleaved: 0->2->4 and 1->3->5.
        let edges = [(0, 3), (3, 4), (1, 2), (2, 5)];
        let l = lay(6, &edges);
        assert_eq!(crossings_of(&l, &edges), 0);
        // A "ladder" of a diamond has no crossings either.
        let diamond = [(0, 1), (0, 2), (1, 3), (2, 3)];
        assert_eq!(crossings_of(&lay(4, &diamond), &diamond), 0);
    }

    // --------------------------------------------------------- coordinates

    #[test]
    fn a_node_is_level_with_the_middle_of_its_neighbours() {
        // One source feeding three targets: the source sits at the middle target's height.
        let l = lay(4, &[(0, 1), (0, 2), (0, 3)]);
        let src = l.nodes[0].y + l.nodes[0].h / 2.0;
        let targets: Vec<f64> = (1..4).map(|i| l.nodes[i].y + l.nodes[i].h / 2.0).collect();
        let mid = (targets[0] + targets[1] + targets[2]) / 3.0;
        assert!((src - mid).abs() < 1.0, "{src} vs {mid}");
        // And the targets keep their spacing.
        let mut ys = targets.clone();
        ys.sort_by(f64::total_cmp);
        assert!(ys[1] - ys[0] >= BOX.1 + Params::default().node_gap - 1e-9);
    }

    #[test]
    fn non_decreasing_fit_pools_violations() {
        assert_eq!(non_decreasing_fit(&[1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
        assert_eq!(non_decreasing_fit(&[3.0, 1.0]), [2.0, 2.0]);
        assert_eq!(
            non_decreasing_fit(&[1.0, 5.0, 2.0, 3.0]),
            [1.0, 10.0 / 3.0, 10.0 / 3.0, 10.0 / 3.0]
        );
        assert!(non_decreasing_fit(&[]).is_empty());
    }

    // ----------------------------------------------------------- the rest

    #[test]
    fn loops_and_self_links_are_errors_not_hangs() {
        assert_eq!(
            layout(&[BOX; 2], &[(0, 1), (1, 0)], &Params::default()).unwrap_err(),
            LayoutError::Cycle
        );
        assert_eq!(
            layout(&[BOX; 1], &[(0, 0)], &Params::default()).unwrap_err(),
            LayoutError::Cycle
        );
    }

    #[test]
    fn nothing_to_lay_out_is_fine_and_layouts_are_deterministic() {
        let e = layout(&[], &[], &Params::default()).unwrap();
        assert!(e.nodes.is_empty() && e.routes.is_empty());
        let edges = [(0, 2), (1, 2), (2, 3), (0, 4), (4, 3)];
        assert_eq!(lay(5, &edges), lay(5, &edges));
    }

    #[test]
    fn varying_box_sizes_are_respected() {
        let sizes = [(100.0, 20.0), (240.0, 60.0), (80.0, 30.0)];
        let l = layout(&sizes, &[(0, 1), (0, 2)], &Params::default()).unwrap();
        for (p, s) in l.nodes.iter().zip(sizes) {
            assert_eq!((p.w, p.h), s);
        }
        // Column 1 is as wide as its widest box; the next column starts after the gap.
        assert!(!overlap(&l.nodes[1], &l.nodes[2]));
        assert!(l.nodes[1].x >= l.nodes[0].x + 100.0 + Params::default().layer_gap - 1e-9);
    }

    proptest! {
        /// Whatever the DAG: boxes never overlap, every link points right and its route joins
        /// the right edge of the source to the left edge of the target, left to right.
        #[test]
        fn layouts_are_clean(
            n in 1usize..30,
            raw in proptest::collection::vec((0usize..30, 0usize..30), 0..60),
        ) {
            let mut seen = std::collections::HashSet::new();
            let edges: Vec<(usize, usize)> = raw.into_iter()
                .filter(|&(a, b)| a < b && b < n)
                .filter(|e| seen.insert(*e))
                .collect();
            let l = lay(n, &edges);
            for i in 0..n {
                for j in i + 1..n {
                    prop_assert!(!overlap(&l.nodes[i], &l.nodes[j]), "{i} and {j} overlap");
                }
                prop_assert!(l.nodes[i].x >= 0.0 && l.nodes[i].y >= 0.0);
                prop_assert!(l.nodes[i].x + l.nodes[i].w <= l.width + 1e-9);
                prop_assert!(l.nodes[i].y + l.nodes[i].h <= l.height + 1e-9);
            }
            for (k, &(a, b)) in edges.iter().enumerate() {
                prop_assert!(l.nodes[a].layer < l.nodes[b].layer);
                prop_assert!(l.nodes[a].x + l.nodes[a].w <= l.nodes[b].x);
                let route = &l.routes[k];
                prop_assert!(route.len() >= 2);
                prop_assert!(route.windows(2).all(|p| p[0].0 < p[1].0));
                prop_assert_eq!(route[0].0, l.nodes[a].x + l.nodes[a].w);
                prop_assert_eq!(route[route.len() - 1].0, l.nodes[b].x);
                prop_assert_eq!(route.len(), l.nodes[b].layer - l.nodes[a].layer + 1);
            }
        }
    }

    /// Spec 18: 100 nodes in under 100 ms. Timing depends on the machine, so it only runs on
    /// request: `cargo test -p minimap-core --release layout_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn layout_speed() {
        use std::time::Instant;
        for n in [100usize, 400] {
            // A layered-looking plan: each node blocks a few later ones.
            let mut edges = Vec::new();
            for i in 0..n {
                for step in [1usize, 2, 5, 11] {
                    if i + step < n && (i * 31 + step * 7) % 5 != 0 {
                        edges.push((i, i + step));
                    }
                }
            }
            let started = Instant::now();
            let l = layout(&vec![BOX; n], &edges, &Params::default()).unwrap();
            println!(
                "{n} nodes, {} edges: {:?} (crossings left: {})",
                edges.len(),
                started.elapsed(),
                crossings_of(&l, &edges)
            );
        }
    }
}
