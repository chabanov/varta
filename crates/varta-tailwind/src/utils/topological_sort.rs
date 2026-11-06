//! Port of Tailwind CSS v4.3.5 `utils/topological-sort.ts` (Tailwind Labs,
//! MIT): order graph nodes after their dependencies.
//!
//! Tailwind reports a cycle through an `onCircularDependency` callback that
//! throws; the port returns it as an error instead. The depth-first search
//! keeps an explicit stack, so a deep graph cannot exhaust the native stack.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

/// A dependency cycle: `start` was reached again through `path`, the chain
/// of nodes being visited (outermost first).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("circular dependency detected: {path:?} -> {start:?}")]
pub struct CircularDependency<K: Debug> {
    pub path: Vec<K>,
    pub start: K,
}

/// One node being visited and the index of its next dependency.
struct Frame<'g, K> {
    node: &'g K,
    next_dependency: usize,
}

/// Sort the nodes of `graph` (node → dependencies, in insertion order) so
/// that every node follows its dependencies; dependencies that are not
/// nodes of the graph are ignored. Nodes are visited in insertion order.
///
/// A repeated node keeps its first position and its last dependency list,
/// as `Map.set` does.
pub fn topological_sort<K: Ord + Clone + Debug>(
    graph: &[(K, Vec<K>)],
) -> Result<Vec<K>, CircularDependency<K>> {
    let mut dependencies: BTreeMap<&K, &[K]> = BTreeMap::new();
    let mut order: Vec<&K> = Vec::new();
    for (node, node_dependencies) in graph {
        if dependencies.insert(node, node_dependencies).is_none() {
            order.push(node);
        }
    }

    let mut seen: BTreeSet<&K> = BTreeSet::new();
    let mut wip: BTreeSet<&K> = BTreeSet::new();
    let mut sorted: Vec<K> = Vec::new();
    let mut stack: Vec<Frame<'_, K>> = Vec::new();

    for root in order {
        let mut pending = Some(root);
        loop {
            if let Some(node) = pending.take()
                && dependencies.contains_key(node)
                && !seen.contains(node)
            {
                if wip.contains(node) {
                    return Err(CircularDependency {
                        path: stack.iter().map(|frame| frame.node.clone()).collect(),
                        start: node.clone(),
                    });
                }
                wip.insert(node);
                stack.push(Frame {
                    node,
                    next_dependency: 0,
                });
            }

            let Some(frame) = stack.last_mut() else {
                break;
            };
            let node_dependencies = dependencies.get(frame.node).copied().unwrap_or_default();
            if let Some(dependency) = node_dependencies.get(frame.next_dependency) {
                frame.next_dependency += 1;
                pending = Some(dependency);
                continue;
            }

            let node = frame.node;
            stack.pop();
            seen.insert(node);
            wip.remove(node);
            sorted.push(node.clone());
        }
    }

    Ok(sorted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(&'static str, &[&'static str])]) -> Vec<(&'static str, Vec<&'static str>)> {
        edges
            .iter()
            .map(|(node, dependencies)| (*node, dependencies.to_vec()))
            .collect()
    }

    #[test]
    fn sorts_dependencies_first() {
        let input = graph(&[("a", &["b", "c"]), ("b", &["c"]), ("c", &[]), ("d", &["x"])]);
        assert_eq!(topological_sort(&input), Ok(vec!["c", "b", "a", "d"]));
    }

    #[test]
    fn keeps_insertion_order_of_independent_nodes() {
        let input = graph(&[("z", &[]), ("a", &[]), ("m", &["a"])]);
        assert_eq!(topological_sort(&input), Ok(vec!["z", "a", "m"]));
    }

    #[test]
    fn reports_cycles_with_their_path() {
        let input = graph(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]);
        assert_eq!(
            topological_sort(&input),
            Err(CircularDependency {
                path: vec!["a", "b", "c"],
                start: "a",
            })
        );
        let input = graph(&[("a", &["a"])]);
        assert_eq!(
            topological_sort(&input),
            Err(CircularDependency {
                path: vec!["a"],
                start: "a",
            })
        );
    }

    #[test]
    fn handles_deep_chains() {
        const DEPTH: usize = 100_000;
        let input: Vec<(usize, Vec<usize>)> = (0..DEPTH).map(|i| (i, vec![i + 1])).collect();
        let sorted = topological_sort(&input).map(|sorted| sorted.len());
        assert_eq!(sorted, Ok(DEPTH));
    }
}
