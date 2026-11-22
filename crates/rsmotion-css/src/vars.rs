use std::collections::{HashMap, HashSet};

/// Inherits custom properties (`--*`) from parent and overlays winning custom properties on this node.
pub fn build_element_vars(
    parent_vars: &HashMap<String, String>,
    winning_decls: &[(String, String)],
) -> HashMap<String, String> {
    let mut vars = parent_vars.clone();
    for (k, v) in winning_decls {
        if k.starts_with("--") {
            vars.insert(k.clone(), v.clone());
        }
    }
    vars
}

fn find_var_call(s: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let lower = s.to_ascii_lowercase();
    let mut idx = from;
    while let Some(rel) = lower[idx..].find("var(") {
        let pos = idx + rel;
        let boundary_ok = pos == 0
            || !(bytes[pos - 1].is_ascii_alphanumeric()
                || bytes[pos - 1] == b'-'
                || bytes[pos - 1] == b'_');
        if !boundary_ok {
            idx = pos + 4;
            continue;
        }
        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut j = pos + 3;
        while j < bytes.len() {
            let b = bytes[j];
            if b == b'\\' && j + 1 < bytes.len() {
                j += 2;
                continue;
            }
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
                j += 1;
                continue;
            }
            match b {
                b'"' | b'\'' => quote = Some(b),
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((pos, j + 1));
                    }
                }
                _ => {}
            }
            j += 1;
        }
        return None;
    }
    None
}

fn split_first_top_level_comma(inner: &str) -> (&str, Option<&str>) {
    let bytes = inner.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => quote = Some(b),
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b',' if paren == 0 && bracket == 0 => {
                return (inner[..i].trim(), Some(inner[i + 1..].trim()));
            }
            _ => {}
        }
        i += 1;
    }
    (inner.trim(), None)
}

/// Resolves all `var(--name, fallback)` occurrences recursively with cycle detection.
pub fn resolve_var_references(raw: &str, vars: &HashMap<String, String>) -> Option<String> {
    if !raw.contains("var(") && !raw.contains("VAR(") {
        return Some(raw.to_string());
    }
    let mut active = HashSet::new();
    resolve_recursive(raw, vars, &mut active, 0)
}

fn resolve_recursive(
    raw: &str,
    vars: &HashMap<String, String>,
    active: &mut HashSet<String>,
    depth: usize,
) -> Option<String> {
    if depth >= 16 {
        return None;
    }
    if !raw.contains("var(") && !raw.contains("VAR(") {
        return Some(raw.to_string());
    }

    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    while let Some((start, end)) = find_var_call(raw, cursor) {
        out.push_str(&raw[cursor..start]);
        let inner = &raw[start + 4..end - 1];
        let (name_raw, fallback_raw) = split_first_top_level_comma(inner);
        let var_name = name_raw.trim();

        let substituted = if var_name.starts_with("--") && !active.contains(var_name) {
            if let Some(raw_def) = vars.get(var_name) {
                active.insert(var_name.to_string());
                let res = resolve_recursive(raw_def, vars, active, depth + 1);
                active.remove(var_name);
                res
            } else {
                None
            }
        } else {
            None
        };

        if let Some(val) = substituted {
            out.push_str(&val);
        } else {
            let fb = fallback_raw?;
            let fb_val = resolve_recursive(fb, vars, active, depth + 1)?;
            out.push_str(&fb_val);
        }
        cursor = end;
    }
    out.push_str(&raw[cursor..]);
    Some(out)
}
