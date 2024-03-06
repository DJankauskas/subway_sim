use std::{cmp::Ordering, collections::{BinaryHeap, HashMap, HashSet, hash_map::Entry::{Occupied, Vacant}}};
use petgraph::{graph::{EdgeIndex, EdgeReference, NodeIndex}, visit::{EdgeRef, VisitMap, Visitable}};

use crate::{simulator::SearchGraph, Edge};

pub enum Terminated {
    Exhaustive,
    At(NodeIndex),
}

// Algorithm based on petgraph impl
pub fn dijkstra<F>(
    graph: &SearchGraph,
    start: NodeIndex,
    goal: &HashSet<NodeIndex>,
    mut edge_cost: F,
) -> (HashMap<NodeIndex, (u16, Option<EdgeIndex>)>, Terminated)
where
    F: FnMut(EdgeReference<Edge>) -> u16,
{
    let mut visited = graph.visit_map();
    let mut scores = HashMap::new();
    //let mut predecessor = HashMap::new();
    let mut visit_next = BinaryHeap::new();
    let zero_score = 0;
    scores.insert(start, (zero_score, None));
    visit_next.push(MinScored(zero_score, start));
    while let Some(MinScored(node_score, node)) = visit_next.pop() {
        if visited.is_visited(&node) {
            continue;
        }
        if goal.contains(&node) {
            return (scores, Terminated::At(node));
        }
        for edge in graph.edges(node) {
            let next = edge.target();
            if visited.is_visited(&next) {
                continue;
            }
            let next_score = node_score + edge_cost(edge);
            match scores.entry(next) {
                Occupied(ent) => {
                    if next_score < ent.get().0 {
                        *ent.into_mut() = (next_score, Some(edge.id()));
                        visit_next.push(MinScored(next_score, next));
                        //predecessor.insert(next.clone(), node.clone());
                    }
                }
                Vacant(ent) => {
                    ent.insert((next_score, Some(edge.id())));
                    visit_next.push(MinScored(next_score, next));
                    //predecessor.insert(next.clone(), node.clone());
                }
            }
        }
        visited.visit(node);
    }
    (scores, Terminated::Exhaustive)
}

// Struct from petgraph crate
struct MinScored<K, T>(pub K, pub T);

impl<K: PartialOrd, T> PartialEq for MinScored<K, T> {
    #[inline]
    fn eq(&self, other: &MinScored<K, T>) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<K: PartialOrd, T> Eq for MinScored<K, T> {}

impl<K: PartialOrd, T> PartialOrd for MinScored<K, T> {
    #[inline]
    fn partial_cmp(&self, other: &MinScored<K, T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: PartialOrd, T> Ord for MinScored<K, T> {
    #[inline]
    fn cmp(&self, other: &MinScored<K, T>) -> Ordering {
        let a = &self.0;
        let b = &other.0;
        if a == b {
            Ordering::Equal
        } else if a < b {
            Ordering::Greater
        } else if a > b {
            Ordering::Less
        } else if a.ne(a) && b.ne(b) {
            // these are the NaN cases
            Ordering::Equal
        } else if a.ne(a) {
            // Order NaN less, so that it is last in the MinScore order
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}