//! PATH parsing, `~` expansion, and shadowing (overwrite) detection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single entry in `$PATH`.
#[derive(Debug, Clone)]
#[allow(dead_code)] // kept for future CLI flags / debug output
pub struct PathEntry {
    /// Position in PATH (0 = first, highest priority).
    pub index: usize,
    /// Raw string as it appears in PATH (may start with `~`).
    pub raw: String,
    /// Absolute, canonicalized path used for filesystem access.
    pub resolved: PathBuf,
    /// Whether the directory exists / is readable.
    pub exists: bool,
    /// Direct children of this directory (file name only).
    pub children: Vec<ChildEntry>,
    /// How many of our children are shadowed by an earlier PATH entry.
    pub shadowed_count: usize,
    /// If `Some`, this entry's resolved path is identical to an earlier
    /// entry's (i.e. the same directory appears twice in PATH). The
    /// value is the index of the first occurrence.
    pub duplicate_of: Option<usize>,
}

/// A file inside a PATH directory.
#[derive(Debug, Clone)]
pub struct ChildEntry {
    pub name: String,
    /// If `Some`, this child is shadowed by the given absolute path that
    /// lives in an *earlier* (higher-priority) PATH entry. Shell lookup
    /// returns the earlier match, so this later occurrence is effectively
    /// "overwritten".
    pub overwritten_by: Option<PathBuf>,
}

/// Split `$PATH` using the platform separator (`:` on unix, `;` on windows).
pub fn split_path_var(path_var: &str) -> Vec<String> {
    let sep = if cfg!(windows) { ';' } else { ':' };
    path_var
        .split(sep)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Expand a leading `~` (or `~user`) to the user's home directory.
pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if p == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(p)
}

/// Pretty-print a path, collapsing `$HOME` back to `~` for display.
pub fn display_path(p: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = p.strip_prefix(&home) {
            let rest_str = rest.to_string_lossy();
            if rest_str.is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest_str.trim_start_matches(std::path::is_separator));
        }
    }
    p.to_string_lossy().to_string()
}

/// Read directory children (file names only, not recursive).
fn list_children(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for ent in rd.flatten() {
            let ft = match ent.file_type() {
                Ok(f) => f,
                Err(_) => continue,
            };
            if ft.is_dir() {
                continue;
            }
            names.push(ent.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    names
}

/// Parse the full PATH string and compute overwriting + duplicate info.
pub fn parse(path_var: &str) -> Vec<PathEntry> {
    let raws = split_path_var(path_var);

    // Resolve each entry and detect duplicates by canonical resolved path.
    let mut first_index: HashMap<PathBuf, usize> = HashMap::new();
    let mut entries: Vec<PathEntry> = raws
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let resolved = expand_tilde(raw);
            let exists = resolved.exists() && resolved.is_dir();
            let duplicate_of = first_index.get(&resolved).copied();
            if duplicate_of.is_none() {
                first_index.insert(resolved.clone(), i);
            }
            PathEntry {
                index: i,
                raw: raw.clone(),
                resolved,
                exists,
                children: Vec::new(),
                shadowed_count: 0,
                duplicate_of,
            }
        })
        .collect();

    // Populate children (skip duplicates — they point to the same dir
    // anyway, and we don't want to double-count in overwriting logic).
    for e in entries.iter_mut() {
        if e.exists && e.duplicate_of.is_none() {
            let names = list_children(&e.resolved);
            e.children = names
                .into_iter()
                .map(|n| ChildEntry {
                    name: n,
                    overwritten_by: None,
                })
                .collect();
        }
    }

    // Compute overwriting across entries.
    let mut first_seen: HashMap<String, PathBuf> = HashMap::new();

    // First, pre-compute children for duplicates by cloning from the
    // first occurrence. We do this in a separate pass so we don't hold
    // a mutable borrow of `entries` while indexing into it immutably.
    let mut dup_children: Vec<Option<Vec<ChildEntry>>> = vec![None; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        if let Some(orig_idx) = e.duplicate_of {
            let kids: Vec<ChildEntry> = entries[orig_idx]
                .children
                .iter()
                .map(|c| ChildEntry {
                    name: c.name.clone(),
                    overwritten_by: None,
                })
                .collect();
            dup_children[i] = Some(kids);
        }
    }
    for (i, kids) in dup_children.into_iter().enumerate() {
        if let Some(k) = kids {
            entries[i].children = k;
        }
    }

    for e in entries.iter_mut() {
        for c in e.children.iter_mut() {
            let abs = e.resolved.join(&c.name);
            if let Some(owner) = first_seen.get(&c.name) {
                c.overwritten_by = Some(owner.clone());
            } else {
                first_seen.insert(c.name.clone(), abs);
            }
        }
        e.shadowed_count = e
            .children
            .iter()
            .filter(|c| c.overwritten_by.is_some())
            .count();
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_works() {
        let v = split_path_var("/a:/b::/c");
        assert_eq!(v, vec!["/a", "/b", "/c"]);
    }

    #[test]
    fn tilde_roundtrip() {
        let p = expand_tilde("~");
        assert!(p.starts_with(dirs::home_dir().unwrap()));
        let d = display_path(&p);
        assert_eq!(d, "~");
    }

    #[test]
    fn duplicate_detection() {
        let home = dirs::home_dir().unwrap();
        let s = format!(
            "{}:{}",
            home.to_string_lossy(),
            home.to_string_lossy()
        );
        let entries = parse(&s);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].duplicate_of.is_none());
        assert_eq!(entries[1].duplicate_of, Some(0));
    }
}
