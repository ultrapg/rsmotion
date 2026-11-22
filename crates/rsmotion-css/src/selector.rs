use rsmotion_scene::{NodeKind, SceneTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

impl Specificity {
    pub const ZERO: Self = Self(0, 0, 0);

    pub fn combine(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1, self.2 + other.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrOp {
    Present,
    Exact,
    Includes,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrMatch {
    pub name: String,
    pub op: AttrOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NthExpr {
    pub step: i32,
    pub offset: i32,
}

impl NthExpr {
    pub fn matches_1based(self, index: usize) -> bool {
        let idx = index as i32;
        if self.step == 0 {
            return idx == self.offset;
        }
        let diff = idx - self.offset;
        if self.step > 0 {
            diff >= 0 && diff % self.step == 0
        } else {
            diff <= 0 && diff % (-self.step) == 0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    Root,
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    NthChild(NthExpr),
    NthLastChild(NthExpr),
    Not(Vec<Selector>),
    Is(Vec<Selector>),
    Where(Vec<Selector>),
    NeverMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompoundSelector {
    pub tag: Option<String>, // None = `*` or omitted
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attrs: Vec<AttrMatch>,
    pub pseudos: Vec<PseudoClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Ordered left-to-right from rootmost ancestor to target node.
    /// `parts[i].1` is the combinator joining `parts[i]` -> `parts[i + 1]`.
    pub parts: Vec<(CompoundSelector, Option<Combinator>)>,
    pub specificity: Specificity,
}

/// Per-node structural metadata computed once per `SceneTree` in O(N).
#[derive(Debug, Clone, Default)]
pub struct NodeDomMeta {
    pub parent: Option<usize>,
    pub prev_element_sibling: Option<usize>,
    pub element_index_1based: usize,
    pub parent_element_count: usize,
    pub is_empty_element: bool,
}

pub fn build_dom_meta(tree: &SceneTree) -> Vec<NodeDomMeta> {
    let mut meta = vec![NodeDomMeta::default(); tree.nodes.len()];
    if tree.root < tree.nodes.len() {
        fill_dom_meta(tree, tree.root, None, 1, 1, None, &mut meta);
    }
    meta
}

fn fill_dom_meta(
    tree: &SceneTree,
    node_id: usize,
    parent: Option<usize>,
    idx_1based: usize,
    sibling_count: usize,
    prev_sib: Option<usize>,
    meta: &mut [NodeDomMeta],
) {
    let children_ids: Vec<usize> = match &tree.nodes[node_id].node {
        NodeKind::Element { children, .. } => children.clone(),
        NodeKind::Svg { .. }
        | NodeKind::Canvas2d { .. }
        | NodeKind::Lottie { .. }
        | NodeKind::Text(_) => Vec::new(),
    };

    let elem_children: Vec<usize> = children_ids
        .iter()
        .copied()
        .filter(|&cid| {
            tree.nodes.get(cid).is_some_and(|n| {
                matches!(
                    n.node,
                    NodeKind::Element { .. }
                        | NodeKind::Svg { .. }
                        | NodeKind::Canvas2d { .. }
                        | NodeKind::Lottie { .. }
                )
            })
        })
        .collect();
    let has_nonempty_text = children_ids.iter().any(|&cid| {
        tree.nodes
            .get(cid)
            .is_some_and(|n| matches!(&n.node, NodeKind::Text(t) if !t.trim().is_empty()))
    });

    meta[node_id] = NodeDomMeta {
        parent,
        prev_element_sibling: prev_sib,
        element_index_1based: idx_1based,
        parent_element_count: sibling_count,
        is_empty_element: elem_children.is_empty() && !has_nonempty_text,
    };

    let total = elem_children.len();
    let mut prev_elem = None;
    for (pos, cid) in elem_children.into_iter().enumerate() {
        fill_dom_meta(tree, cid, Some(node_id), pos + 1, total, prev_elem, meta);
        prev_elem = Some(cid);
    }
}

pub fn split_selector_list(raw: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let bytes = raw.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
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
                let piece = raw[start..i].trim();
                if !piece.is_empty() {
                    parts.push(piece);
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

struct SelCursor<'a> {
    raw: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> SelCursor<'a> {
    fn new(raw: &'a str) -> Self {
        Self {
            raw,
            bytes: raw.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) -> bool {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
        self.pos > start
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Reads an unescaped CSS identifier, decoding Tailwind escapes (`\:`, `\[`, `\#`, `\/`, `\xxxxxx `).
    fn read_ident(&mut self) -> Option<String> {
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'\\' {
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    break;
                }
                let next = self.bytes[self.pos];
                if next.is_ascii_hexdigit() {
                    let hex_start = self.pos;
                    let mut count = 0;
                    while self.pos < self.bytes.len()
                        && count < 6
                        && self.bytes[self.pos].is_ascii_hexdigit()
                    {
                        self.pos += 1;
                        count += 1;
                    }
                    if let Ok(cp) = u32::from_str_radix(&self.raw[hex_start..self.pos], 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                    if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                        self.pos += 1;
                    }
                } else {
                    let ch = self.raw[self.pos..].chars().next()?;
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
                continue;
            }
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b >= 0x80 {
                let ch = self.raw[self.pos..].chars().next()?;
                out.push(ch);
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn read_paren_content(&mut self) -> Option<String> {
        if self.peek() != Some(b'(') {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        let mut depth = 1usize;
        let mut quote: Option<u8> = None;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'\\' && self.pos + 1 < self.bytes.len() {
                self.pos += 2;
                continue;
            }
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
                self.pos += 1;
                continue;
            }
            match b {
                b'"' | b'\'' => quote = Some(b),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let res = self.raw[start..self.pos].to_string();
                        self.pos += 1;
                        return Some(res);
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        None
    }

    fn read_attr(&mut self) -> Option<AttrMatch> {
        if self.peek() != Some(b'[') {
            return None;
        }
        self.pos += 1;
        self.skip_ws();
        let name = self.read_ident()?.to_ascii_lowercase();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Some(AttrMatch {
                name,
                op: AttrOp::Present,
                value: String::new(),
            });
        }
        let op = match (self.peek(), self.bytes.get(self.pos + 1).copied()) {
            (Some(b'='), _) => {
                self.pos += 1;
                AttrOp::Exact
            }
            (Some(b'~'), Some(b'=')) => {
                self.pos += 2;
                AttrOp::Includes
            }
            (Some(b'^'), Some(b'=')) => {
                self.pos += 2;
                AttrOp::Prefix
            }
            (Some(b'$'), Some(b'=')) => {
                self.pos += 2;
                AttrOp::Suffix
            }
            (Some(b'*'), Some(b'=')) => {
                self.pos += 2;
                AttrOp::Substring
            }
            _ => return None,
        };
        self.skip_ws();
        let value = match self.peek() {
            Some(q @ (b'"' | b'\'')) => {
                self.pos += 1;
                let mut v = String::new();
                while self.pos < self.bytes.len() {
                    let b = self.bytes[self.pos];
                    if b == b'\\' && self.pos + 1 < self.bytes.len() {
                        self.pos += 1;
                        let ch = self.raw[self.pos..].chars().next()?;
                        v.push(ch);
                        self.pos += ch.len_utf8();
                        continue;
                    }
                    if b == q {
                        self.pos += 1;
                        break;
                    }
                    let ch = self.raw[self.pos..].chars().next()?;
                    v.push(ch);
                    self.pos += ch.len_utf8();
                }
                v
            }
            _ => self.read_ident().unwrap_or_default(),
        };
        self.skip_ws();
        // Skip optional `i` / `s` flags
        if matches!(self.peek(), Some(b'i' | b'I' | b's' | b'S')) {
            self.pos += 1;
            self.skip_ws();
        }
        if self.peek() != Some(b']') {
            return None;
        }
        self.pos += 1;
        Some(AttrMatch { name, op, value })
    }
}

fn parse_nth_expr(raw: &str) -> Option<NthExpr> {
    let s: String = raw
        .split(" of ")
        .next()
        .unwrap_or(raw)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if s == "odd" {
        return Some(NthExpr { step: 2, offset: 1 });
    }
    if s == "even" {
        return Some(NthExpr { step: 2, offset: 0 });
    }
    if let Some((a_part, b_part)) = s.split_once('n') {
        let step = match a_part {
            "" | "+" => 1,
            "-" => -1,
            _ => a_part.parse::<i32>().ok()?,
        };
        let offset = if b_part.is_empty() {
            0
        } else {
            b_part.parse::<i32>().ok()?
        };
        Some(NthExpr { step, offset })
    } else {
        let offset = s.parse::<i32>().ok()?;
        Some(NthExpr { step: 0, offset })
    }
}

/// Parses a single complex CSS selector into `Selector` (`None` if syntactically invalid).
pub fn parse_selector(raw: &str) -> Option<Selector> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut cur = SelCursor::new(trimmed);
    let mut parts: Vec<(CompoundSelector, Option<Combinator>)> = Vec::new();
    let mut total_spec = Specificity::ZERO;

    while cur.pos < cur.bytes.len() {
        let mut comp = CompoundSelector::default();
        let mut comp_spec = Specificity::ZERO;
        let mut parsed_any = false;

        if cur.peek() == Some(b'*') {
            cur.pos += 1;
            parsed_any = true;
        } else if matches!(cur.peek(), Some(b) if b != b'.' && b != b'#' && b != b'[' && b != b':')
        {
            if let Some(tag) = cur.read_ident() {
                comp.tag = Some(tag.to_ascii_lowercase());
                comp_spec.2 += 1;
                parsed_any = true;
            }
        }

        while let Some(b) = cur.peek() {
            match b {
                b'#' => {
                    cur.pos += 1;
                    let id = cur.read_ident()?;
                    comp.id = Some(id);
                    comp_spec.0 += 1;
                    parsed_any = true;
                }
                b'.' => {
                    cur.pos += 1;
                    let cls = cur.read_ident()?;
                    comp.classes.push(cls);
                    comp_spec.1 += 1;
                    parsed_any = true;
                }
                b'[' => {
                    let attr = cur.read_attr()?;
                    comp.attrs.push(attr);
                    comp_spec.1 += 1;
                    parsed_any = true;
                }
                b':' => {
                    cur.pos += 1;
                    if cur.peek() == Some(b':') {
                        // Pseudo-element (`::before`, `::after`): skip cleanly and never match element
                        cur.pos += 1;
                        let _ = cur.read_ident();
                        if cur.peek() == Some(b'(') {
                            let _ = cur.read_paren_content();
                        }
                        comp.pseudos.push(PseudoClass::NeverMatch);
                        comp_spec.2 += 1;
                        parsed_any = true;
                        continue;
                    }
                    let pname = cur.read_ident()?.to_ascii_lowercase();
                    let arg = if cur.peek() == Some(b'(') {
                        Some(cur.read_paren_content()?)
                    } else {
                        None
                    };
                    let (pseudo, p_spec) = match pname.as_str() {
                        "root" => (PseudoClass::Root, Specificity(0, 1, 0)),
                        "first-child" => (PseudoClass::FirstChild, Specificity(0, 1, 0)),
                        "last-child" => (PseudoClass::LastChild, Specificity(0, 1, 0)),
                        "only-child" => (PseudoClass::OnlyChild, Specificity(0, 1, 0)),
                        "empty" => (PseudoClass::Empty, Specificity(0, 1, 0)),
                        "nth-child" => {
                            let n = parse_nth_expr(arg.as_deref()?)?;
                            (PseudoClass::NthChild(n), Specificity(0, 1, 0))
                        }
                        "nth-last-child" => {
                            let n = parse_nth_expr(arg.as_deref()?)?;
                            (PseudoClass::NthLastChild(n), Specificity(0, 1, 0))
                        }
                        "not" => {
                            let list = parse_selector_list(arg.as_deref()?);
                            let max_sp = list
                                .iter()
                                .map(|s| s.specificity)
                                .max()
                                .unwrap_or(Specificity::ZERO);
                            (PseudoClass::Not(list), max_sp)
                        }
                        "is" => {
                            let list = parse_selector_list(arg.as_deref()?);
                            let max_sp = list
                                .iter()
                                .map(|s| s.specificity)
                                .max()
                                .unwrap_or(Specificity::ZERO);
                            (PseudoClass::Is(list), max_sp)
                        }
                        "where" => {
                            let list = parse_selector_list(arg.as_deref()?);
                            (PseudoClass::Where(list), Specificity::ZERO)
                        }
                        // Dynamic/interactive pseudo-classes never match static frame snapshots
                        _ => (PseudoClass::NeverMatch, Specificity(0, 1, 0)),
                    };
                    comp.pseudos.push(pseudo);
                    comp_spec = comp_spec.combine(p_spec);
                    parsed_any = true;
                }
                _ => break,
            }
        }

        if !parsed_any {
            return None;
        }
        total_spec = total_spec.combine(comp_spec);

        let had_ws = cur.skip_ws();
        if cur.pos >= cur.bytes.len() {
            parts.push((comp, None));
            break;
        }

        let comb = match cur.peek() {
            Some(b'>') => {
                cur.pos += 1;
                cur.skip_ws();
                Combinator::Child
            }
            Some(b'+') => {
                cur.pos += 1;
                cur.skip_ws();
                Combinator::AdjacentSibling
            }
            Some(b'~') => {
                cur.pos += 1;
                cur.skip_ws();
                Combinator::GeneralSibling
            }
            Some(_) if had_ws => Combinator::Descendant,
            _ => return None,
        };
        parts.push((comp, Some(comb)));
    }

    (!parts.is_empty()).then_some(Selector {
        parts,
        specificity: total_spec,
    })
}

pub fn parse_selector_list(raw: &str) -> Vec<Selector> {
    split_selector_list(raw)
        .into_iter()
        .filter_map(parse_selector)
        .collect()
}

pub type NodeSelectorInfo<'a> = (&'a str, &'a [String], &'a [(String, String)]);

pub fn node_tag_classes_attrs(tree: &SceneTree, node_id: usize) -> Option<NodeSelectorInfo<'_>> {
    match &tree.nodes.get(node_id)?.node {
        NodeKind::Element {
            tag,
            classes,
            attrs,
            ..
        } => Some((tag.as_str(), classes.as_slice(), attrs.as_slice())),
        NodeKind::Svg { classes, attrs, .. } => Some(("svg", classes.as_slice(), attrs.as_slice())),
        NodeKind::Canvas2d { classes, attrs, .. } => {
            Some(("canvas", classes.as_slice(), attrs.as_slice()))
        }
        NodeKind::Lottie { classes, attrs, .. } => {
            Some(("div", classes.as_slice(), attrs.as_slice()))
        }
        NodeKind::Text(_) => None,
    }
}

fn match_attr(attrs: &[(String, String)], m: &AttrMatch) -> bool {
    let Some((_, val)) = attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(&m.name)) else {
        return false;
    };
    match m.op {
        AttrOp::Present => true,
        AttrOp::Exact => val == &m.value,
        AttrOp::Includes => !m.value.is_empty() && val.split_whitespace().any(|w| w == m.value),
        AttrOp::Prefix => !m.value.is_empty() && val.starts_with(&m.value),
        AttrOp::Suffix => !m.value.is_empty() && val.ends_with(&m.value),
        AttrOp::Substring => !m.value.is_empty() && val.contains(&m.value),
    }
}

fn match_compound(
    tree: &SceneTree,
    meta: &[NodeDomMeta],
    node_id: usize,
    comp: &CompoundSelector,
) -> bool {
    let Some((tag, classes, attrs)) = node_tag_classes_attrs(tree, node_id) else {
        return false;
    };

    if let Some(ref want_tag) = comp.tag {
        if !tag.eq_ignore_ascii_case(want_tag) {
            return false;
        }
    }

    if let Some(ref want_id) = comp.id {
        let has_id = attrs.iter().any(|(k, v)| k == "id" && v == want_id);
        if !has_id {
            return false;
        }
    }

    for req_cls in &comp.classes {
        if !classes.iter().any(|c| c == req_cls) {
            return false;
        }
    }

    for attr in &comp.attrs {
        if !match_attr(attrs, attr) {
            return false;
        }
    }

    let m = &meta[node_id];
    for pseudo in &comp.pseudos {
        let ok = match pseudo {
            PseudoClass::Root => m.parent.is_none() || tag == "html",
            PseudoClass::FirstChild => m.element_index_1based == 1,
            PseudoClass::LastChild => {
                m.parent_element_count > 0 && m.element_index_1based == m.parent_element_count
            }
            PseudoClass::OnlyChild => m.parent_element_count == 1 && m.element_index_1based == 1,
            PseudoClass::Empty => m.is_empty_element,
            PseudoClass::NthChild(nth) => nth.matches_1based(m.element_index_1based),
            PseudoClass::NthLastChild(nth) => {
                let rev = m.parent_element_count + 1 - m.element_index_1based;
                nth.matches_1based(rev)
            }
            PseudoClass::Not(list) => !list.iter().any(|s| s.matches(tree, meta, node_id)),
            PseudoClass::Is(list) | PseudoClass::Where(list) => {
                list.iter().any(|s| s.matches(tree, meta, node_id))
            }
            PseudoClass::NeverMatch => false,
        };
        if !ok {
            return false;
        }
    }

    true
}

impl Selector {
    pub fn matches(&self, tree: &SceneTree, meta: &[NodeDomMeta], node_id: usize) -> bool {
        self.matches_part(tree, meta, node_id, self.parts.len() - 1)
    }

    fn matches_part(
        &self,
        tree: &SceneTree,
        meta: &[NodeDomMeta],
        node_id: usize,
        part_idx: usize,
    ) -> bool {
        if !match_compound(tree, meta, node_id, &self.parts[part_idx].0) {
            return false;
        }
        if part_idx == 0 {
            return true;
        }
        let Some(comb) = self.parts[part_idx - 1].1 else {
            return false;
        };
        match comb {
            Combinator::Child => {
                let Some(p) = meta[node_id].parent else {
                    return false;
                };
                self.matches_part(tree, meta, p, part_idx - 1)
            }
            Combinator::Descendant => {
                let mut cur = meta[node_id].parent;
                while let Some(anc) = cur {
                    if self.matches_part(tree, meta, anc, part_idx - 1) {
                        return true;
                    }
                    cur = meta[anc].parent;
                }
                false
            }
            Combinator::AdjacentSibling => {
                let Some(prev) = meta[node_id].prev_element_sibling else {
                    return false;
                };
                self.matches_part(tree, meta, prev, part_idx - 1)
            }
            Combinator::GeneralSibling => {
                let mut cur = meta[node_id].prev_element_sibling;
                while let Some(sib) = cur {
                    if self.matches_part(tree, meta, sib, part_idx - 1) {
                        return true;
                    }
                    cur = meta[sib].prev_element_sibling;
                }
                false
            }
        }
    }
}
