use serde::{Deserialize, Serialize};

/// A named folder for organizing papers, supporting nested hierarchies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    pub position: i32,
}

impl Collection {
    /// Create a new root-level collection with the given name.
    pub fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            parent_id: None,
            position: 0,
        }
    }
}

/// The direct children of `parent_id` (or the roots when `None`), in the order
/// they appear in `collections`. Callers pass a list already sorted by position.
pub fn children_of<'a>(
    collections: &'a [Collection],
    parent_id: Option<&str>,
) -> Vec<&'a Collection> {
    collections
        .iter()
        .filter(|c| c.parent_id.as_deref() == parent_id)
        .collect()
}

/// Whether any collection is parented under `id`.
pub fn has_children(collections: &[Collection], id: &str) -> bool {
    collections
        .iter()
        .any(|c| c.parent_id.as_deref() == Some(id))
}

/// Flatten `collections` into depth-first tree order, pairing each with its
/// nesting depth (roots at 0) and whether it has children. Sibling order follows
/// the input order. A `visited` set breaks any parent-pointer cycles so
/// malformed data can't loop forever.
///
/// Shared by every hierarchical collection view (sidebar tree, paper-detail
/// picker) so the traversal lives in exactly one place.
pub fn collection_tree(collections: &[Collection]) -> Vec<(Collection, usize, bool)> {
    fn walk(
        collections: &[Collection],
        parent: Option<&str>,
        depth: usize,
        visited: &mut std::collections::HashSet<String>,
        out: &mut Vec<(Collection, usize, bool)>,
    ) {
        for child in children_of(collections, parent) {
            let Some(id) = child.id.clone() else { continue };
            if !visited.insert(id.clone()) {
                continue;
            }
            out.push((child.clone(), depth, has_children(collections, &id)));
            walk(collections, Some(&id), depth + 1, visited, out);
        }
    }

    let mut out = Vec::with_capacity(collections.len());
    let mut visited = std::collections::HashSet::new();
    walk(collections, None, 0, &mut visited, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coll(id: &str, name: &str, parent: Option<&str>) -> Collection {
        Collection {
            id: Some(id.to_string()),
            name: name.to_string(),
            parent_id: parent.map(str::to_string),
            position: 0,
        }
    }

    #[test]
    fn tree_is_depth_first_with_depth_and_child_flags() {
        // root ─ a ─ a1
        //      │   └ a2
        //      └ b
        let colls = vec![
            coll("a", "A", None),
            coll("b", "B", None),
            coll("a1", "A1", Some("a")),
            coll("a2", "A2", Some("a")),
        ];
        let tree = collection_tree(&colls);
        let shape: Vec<(&str, usize, bool)> = tree
            .iter()
            .map(|(c, d, h)| (c.name.as_str(), *d, *h))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("A", 0, true),
                ("A1", 1, false),
                ("A2", 1, false),
                ("B", 0, false),
            ]
        );
    }

    #[test]
    fn cycle_does_not_loop_forever() {
        // x ↔ y point at each other; neither is a root, so both are unreachable
        // from None and the walk simply yields nothing rather than hanging.
        let colls = vec![coll("x", "X", Some("y")), coll("y", "Y", Some("x"))];
        assert!(collection_tree(&colls).is_empty());
    }

    #[test]
    fn children_and_has_children_helpers() {
        let colls = vec![coll("a", "A", None), coll("a1", "A1", Some("a"))];
        assert_eq!(children_of(&colls, None).len(), 1);
        assert_eq!(children_of(&colls, Some("a"))[0].name, "A1");
        assert!(has_children(&colls, "a"));
        assert!(!has_children(&colls, "a1"));
    }
}
