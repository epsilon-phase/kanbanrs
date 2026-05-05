#[cfg(target_arch = "wasm32")]
const KEY_PREFIX: &str = "kanban/";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn list_documents() -> Vec<String> {
    let Some(storage) = local_storage() else {
        return Vec::new();
    };
    let len = storage.length().unwrap_or(0);
    let mut result = Vec::new();
    for i in 0..len {
        if let Ok(Some(key)) = storage.key(i) {
            if let Some(name) = key.strip_prefix(KEY_PREFIX) {
                result.push(name.to_string());
            }
        }
    }
    result.sort();
    result
}

#[cfg(target_arch = "wasm32")]
pub fn load_document(name: &str) -> Option<String> {
    let key = format!("{KEY_PREFIX}{name}");
    local_storage()?.get_item(&key).ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn save_document(name: &str, data: &str) {
    if let Some(storage) = local_storage() {
        let key = format!("{KEY_PREFIX}{name}");
        storage.set_item(&key, data).ok();
    }
}

#[cfg(target_arch = "wasm32")]
pub fn delete_document(name: &str) {
    if let Some(storage) = local_storage() {
        let key = format!("{KEY_PREFIX}{name}");
        storage.remove_item(&key).ok();
    }
}

// ── Tree ─────────────────────────────────────────────────────────────────────

pub struct DocTreeNode {
    pub name: String,
    /// Full slash-separated path (e.g. "work/project-alpha"). For folders this
    /// is the path up to and including the folder component.
    pub full_path: String,
    pub children: Vec<DocTreeNode>,
    pub is_file: bool,
}

pub fn build_tree(names: &[String]) -> Vec<DocTreeNode> {
    let mut roots = Vec::new();
    for name in names {
        let parts: Vec<&str> = name.split('/').collect();
        insert_node(&mut roots, &parts, name, "");
    }
    roots
}

pub(crate) fn flatten_tree(nodes: &[DocTreeNode]) -> Vec<String> {
    let mut result = Vec::new();
    for node in nodes {
        if node.is_file {
            result.push(node.full_path.clone());
        } else {
            result.extend(flatten_tree(&node.children));
        }
    }
    result.sort();
    result
}

fn insert_node(nodes: &mut Vec<DocTreeNode>, parts: &[&str], full_path: &str, path_prefix: &str) {
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        nodes.push(DocTreeNode {
            name: parts[0].to_string(),
            full_path: full_path.to_string(),
            children: Vec::new(),
            is_file: true,
        });
        return;
    }
    let folder = parts[0];
    let folder_path = if path_prefix.is_empty() {
        folder.to_string()
    } else {
        format!("{path_prefix}/{folder}")
    };
    if let Some(existing) = nodes.iter_mut().find(|n| !n.is_file && n.name == folder) {
        insert_node(&mut existing.children, &parts[1..], full_path, &folder_path);
    } else {
        let mut new_folder = DocTreeNode {
            name: folder.to_string(),
            full_path: folder_path.clone(),
            children: Vec::new(),
            is_file: false,
        };
        insert_node(
            &mut new_folder.children,
            &parts[1..],
            full_path,
            &folder_path,
        );
        nodes.push(new_folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── strategies ───────────────────────────────────────────────────────

    /// A single path segment: lowercase letters and digits, non-empty, no slashes.
    fn arb_segment() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9]{0,10}".prop_map(|s| s)
    }

    /// A slash-separated path of 1–4 segments, e.g. "work/project/notes".
    fn arb_path() -> impl Strategy<Value = String> {
        prop::collection::vec(arb_segment(), 1..=4).prop_map(|segs| segs.join("/"))
    }

    prop_compose! {
        /// A sorted, deduplicated list of 0–20 paths.
        fn arb_names()(mut names in prop::collection::vec(arb_path(), 0..=20)) -> Vec<String> {
            names.sort();
            names.dedup();
            names
        }
    }

    // ── properties ───────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn round_trip_flatten(names in arb_names()) {
            let tree = build_tree(&names);
            let mut flat = flatten_tree(&tree);
            flat.sort();
            prop_assert_eq!(flat, names);
        }

        #[test]
        fn all_leaves_are_files(names in arb_names()) {
            fn check(nodes: &[DocTreeNode]) {
                for node in nodes {
                    if node.children.is_empty() {
                        assert!(node.is_file, "leaf node {:?} is not marked as file", node.full_path);
                    }
                    check(&node.children);
                }
            }
            check(&build_tree(&names));
        }

        #[test]
        fn folders_are_not_files(names in arb_names()) {
            fn check(nodes: &[DocTreeNode]) {
                for node in nodes {
                    if !node.is_file {
                        assert!(!node.children.is_empty(), "folder {:?} has no children", node.full_path);
                    }
                    check(&node.children);
                }
            }
            check(&build_tree(&names));
        }

        #[test]
        fn no_duplicate_full_paths(names in arb_names()) {
            let flat = flatten_tree(&build_tree(&names));
            let mut seen = std::collections::HashSet::new();
            for path in &flat {
                prop_assert!(seen.insert(path), "duplicate path: {}", path);
            }
        }
    }
}
