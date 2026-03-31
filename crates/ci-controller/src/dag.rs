use std::collections::{HashMap, HashSet, VecDeque};

/// Topological sort on a stage dependency graph.
///
/// `deps`: stage_name -> list of stage names it depends on.
/// Returns `Ok(sorted)` (leaves first, roots last) or `Err(cycle_node)`.
pub fn topo_sort(deps: &HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
    // Build in-degree map and adjacency list
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();

    for (node, parents) in deps {
        in_degree.entry(node.as_str()).or_insert(0);
        for parent in parents {
            in_degree.entry(parent.as_str()).or_insert(0);
            children
                .entry(parent.as_str())
                .or_default()
                .push(node.as_str());
            *in_degree.entry(node.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&n, _)| n)
        .collect();

    let mut sorted = Vec::new();
    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(kids) = children.get(node) {
            for &kid in kids {
                let d = in_degree.get_mut(kid).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push_back(kid);
                }
            }
        }
    }

    if sorted.len() < in_degree.len() {
        // Find a node still with in-degree > 0
        let cycle_node = in_degree
            .iter()
            .find(|(_, &d)| d > 0)
            .map(|(&n, _)| n.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Err(cycle_node)
    } else {
        Ok(sorted)
    }
}

/// Validate that `deps` forms a DAG (no cycles).
/// Returns `Err(stage_name)` if a cycle is detected.
pub fn validate_dag(deps: &HashMap<String, Vec<String>>) -> Result<(), String> {
    topo_sort(deps).map(|_| ())
}

/// Check if a stage's dependencies are all satisfied (in the provided terminal set).
pub fn deps_satisfied(
    stage: &str,
    deps: &HashMap<String, Vec<String>>,
    terminal_success: &HashSet<String>,
) -> bool {
    deps.get(stage)
        .map(|parents| parents.iter().all(|p| terminal_success.contains(p)))
        .unwrap_or(true)
}
