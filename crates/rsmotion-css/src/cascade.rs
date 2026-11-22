use std::collections::{BTreeSet, HashMap};

use rsmotion_scene::SceneTree;

use crate::parser::{CssDecl, ParsedStylesheetAst};
use crate::selector::{
    build_dom_meta, node_tag_classes_attrs, parse_selector_list, NodeDomMeta, Selector, Specificity,
};

#[derive(Debug, Clone)]
pub struct IndexedRule {
    pub selector: Selector,
    pub declarations: Vec<CssDecl>,
    pub layer_rank: Option<usize>,
    pub source_order: usize,
}

#[derive(Debug, Clone)]
pub struct CompiledRuleIndex {
    pub rules: Vec<IndexedRule>,
    pub by_id: HashMap<String, Vec<usize>>,
    pub by_class: HashMap<String, Vec<usize>>,
    pub by_tag: HashMap<String, Vec<usize>>,
    pub universal: Vec<usize>,
    pub layer_count: usize,
}

impl CompiledRuleIndex {
    pub fn build(ast: &ParsedStylesheetAst) -> Self {
        let mut rules = Vec::new();
        let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_class: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_tag: HashMap<String, Vec<usize>> = HashMap::new();
        let mut universal = Vec::new();

        for rule_ast in &ast.rules {
            let selectors = parse_selector_list(&rule_ast.selectors_raw);
            for sel in selectors {
                let idx = rules.len();
                let target = &sel.parts[sel.parts.len() - 1].0;
                if let Some(ref id) = target.id {
                    by_id.entry(id.clone()).or_default().push(idx);
                } else if let Some(first_cls) = target.classes.first() {
                    by_class.entry(first_cls.clone()).or_default().push(idx);
                } else if let Some(ref tag) = target.tag {
                    by_tag
                        .entry(tag.to_ascii_lowercase())
                        .or_default()
                        .push(idx);
                } else {
                    universal.push(idx);
                }

                rules.push(IndexedRule {
                    selector: sel,
                    declarations: rule_ast.declarations.clone(),
                    layer_rank: rule_ast.layer_rank,
                    source_order: rule_ast.source_order,
                });
            }
        }

        Self {
            rules,
            by_id,
            by_class,
            by_tag,
            universal,
            layer_count: ast.layer_names.len(),
        }
    }

    /// Resolves CSS Cascade Level 5 ordering across stylesheet rules + `inline_style` for one node.
    pub fn resolve_declarations_for_node(
        &self,
        tree: &SceneTree,
        dom_meta: &[NodeDomMeta],
        node_id: usize,
        inline_style: &[(String, String)],
    ) -> Vec<(String, String)> {
        let mut candidate_indices = BTreeSet::new();
        if let Some((tag, classes, attrs)) = node_tag_classes_attrs(tree, node_id) {
            if let Some((_, id_val)) = attrs.iter().find(|(k, _)| k == "id") {
                if let Some(list) = self.by_id.get(id_val) {
                    candidate_indices.extend(list.iter().copied());
                }
            }
            for cls in classes {
                if let Some(list) = self.by_class.get(cls) {
                    candidate_indices.extend(list.iter().copied());
                }
            }
            let lower_tag = tag.to_ascii_lowercase();
            if let Some(list) = self.by_tag.get(&lower_tag) {
                candidate_indices.extend(list.iter().copied());
            }
        }
        candidate_indices.extend(self.universal.iter().copied());

        // Priority key: (origin_band, layer_order, specificity, decl_order)
        // Lower origin_band < higher origin_band:
        // 0: normal stylesheet (layered then unlayered via layer_order)
        // 1: normal inline
        // 2: !important stylesheet (unlayered then later layer then earlier layer)
        // 3: !important inline
        struct Entry<'a> {
            priority: (u8, usize, Specificity, usize),
            name: &'a str,
            value: &'a str,
        }

        let mut entries: Vec<Entry<'_>> = Vec::new();
        let mut seq = 0usize;

        for rule_idx in candidate_indices {
            let rule = &self.rules[rule_idx];
            if !rule.selector.matches(tree, dom_meta, node_id) {
                continue;
            }
            for decl in &rule.declarations {
                let priority = if !decl.important {
                    // Normal rules: earlier layer < later layer < unlayered (`layer_count`)
                    let layer_key = rule.layer_rank.unwrap_or(self.layer_count);
                    (0, layer_key, rule.selector.specificity, seq)
                } else {
                    // `!important`: unlayered (`0`) < later layer < earlier layer (`layer_count - r`)
                    let layer_key = match rule.layer_rank {
                        None => 0,
                        Some(r) => self.layer_count.saturating_sub(r) + 1,
                    };
                    (2, layer_key, rule.selector.specificity, seq)
                };
                entries.push(Entry {
                    priority,
                    name: decl.name.as_str(),
                    value: decl.value.as_str(),
                });
                seq += 1;
            }
        }

        for (k, v) in inline_style {
            let trimmed = v.trim();
            let lower = trimmed.to_ascii_lowercase();
            let (clean_val, is_imp) = if let Some(idx) = lower.rfind("important") {
                if lower[idx..].trim_end() == "important" {
                    let before = trimmed[..idx].trim_end();
                    if let Some(vp) = before.strip_suffix('!') {
                        (vp.trim(), true)
                    } else {
                        (trimmed, false)
                    }
                } else {
                    (trimmed, false)
                }
            } else {
                (trimmed, false)
            };
            entries.push(Entry {
                priority: if is_imp {
                    (3, 0, Specificity::ZERO, seq)
                } else {
                    (1, 0, Specificity::ZERO, seq)
                },
                name: k.as_str(),
                value: clean_val,
            });
            seq += 1;
        }

        entries.sort_by_key(|e| e.priority);

        // Preserve CSS cascade winning declaration order
        let mut final_map: HashMap<String, (usize, String)> = HashMap::new();
        for (pos, e) in entries.into_iter().enumerate() {
            let name = if e.name.starts_with("--") {
                e.name.to_string()
            } else {
                e.name.to_ascii_lowercase()
            };
            final_map.insert(name, (pos, e.value.to_string()));
        }

        let mut ordered: Vec<(usize, String, String)> = final_map
            .into_iter()
            .map(|(k, (pos, v))| (pos, k, v))
            .collect();
        ordered.sort_by_key(|(pos, _, _)| *pos);
        ordered.into_iter().map(|(_, k, v)| (k, v)).collect()
    }
}

pub fn precompute_dom_meta(tree: &SceneTree) -> Vec<NodeDomMeta> {
    build_dom_meta(tree)
}
