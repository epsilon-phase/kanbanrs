const KEY_PREFIX: &str = "kanban/";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

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

pub fn load_document(name: &str) -> Option<String> {
    let key = format!("{KEY_PREFIX}{name}");
    local_storage()?.get_item(&key).ok()?
}

pub fn save_document(name: &str, data: &str) {
    if let Some(storage) = local_storage() {
        let key = format!("{KEY_PREFIX}{name}");
        storage.set_item(&key, data).ok();
    }
}

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
