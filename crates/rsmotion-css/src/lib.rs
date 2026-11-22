pub mod calc;
pub mod cascade;
pub mod parser;
pub mod selector;
pub mod vars;

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rsmotion_scene::{SceneTree, StyleSheetSource};

pub use calc::{eval_css_math, simplify_math_in_value, CalcVal, MathDim};
pub use cascade::{precompute_dom_meta, CompiledRuleIndex};
pub use parser::{parse_stylesheets, CssDecl, FontFaceDecl, ParsedStylesheetAst, StyleRuleAst};
pub use selector::NodeDomMeta;
pub use vars::{build_element_vars, resolve_var_references};

const MAX_COMPILED_CACHE_ENTRIES: usize = 8;

/// Compiled CSS rules, `@font-face` rules, and `@property` defaults ready for O(N) cascade.
#[derive(Debug, Clone)]
pub struct CompiledStyles {
    pub ast: ParsedStylesheetAst,
    pub index: CompiledRuleIndex,
}

impl CompiledStyles {
    pub fn compile(sheets: &[StyleSheetSource], vw: f32, vh: f32) -> Self {
        let ast = parse_stylesheets(sheets.iter().map(|s| s.css.as_str()), vw, vh);
        let index = CompiledRuleIndex::build(&ast);
        Self { ast, index }
    }

    pub fn font_faces(&self) -> &[FontFaceDecl] {
        &self.ast.font_faces
    }

    pub fn property_initials(&self) -> &HashMap<String, String> {
        &self.ast.property_initials
    }
}

thread_local! {
    static COMPILED_CACHE: RefCell<CompiledStylesCache> = RefCell::new(CompiledStylesCache {
        map: HashMap::new(),
        order: VecDeque::new(),
    });
}

struct CompiledStylesCache {
    map: HashMap<u64, Arc<CompiledStyles>>,
    order: VecDeque<u64>,
}

fn hash_stylesheets(sheets: &[StyleSheetSource], vw: f32, vh: f32) -> u64 {
    let mut h = DefaultHasher::new();
    vw.to_bits().hash(&mut h);
    vh.to_bits().hash(&mut h);
    sheets.len().hash(&mut h);
    for s in sheets {
        s.source.hash(&mut h);
        s.css.hash(&mut h);
    }
    h.finish()
}

/// Compiles `sheets` against `(vw, vh)`, reusing an 8-entry thread-local cache across frames.
pub fn compile_stylesheets(sheets: &[StyleSheetSource], vw: f32, vh: f32) -> Arc<CompiledStyles> {
    let key = hash_stylesheets(sheets, vw, vh);
    COMPILED_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some(hit) = cache.map.get(&key) {
            return Arc::clone(hit);
        }
        let compiled = Arc::new(CompiledStyles::compile(sheets, vw, vh));
        if cache.order.len() >= MAX_COMPILED_CACHE_ENTRIES {
            if let Some(oldest) = cache.order.pop_front() {
                cache.map.remove(&oldest);
            }
        }
        cache.order.push_back(key);
        cache.map.insert(key, Arc::clone(&compiled));
        compiled
    })
}

/// Resolves winning stylesheet + inline declarations, `--*` inheritance, `var()`, and `calc()` for a node.
///
/// `resolve_math_for_decl` simplifies `calc()/min()/max()/clamp()` calls using `parent_font_size`
/// first, updating local `font_size` if a `font-size` declaration appears before other properties.
#[allow(clippy::too_many_arguments)]
pub fn resolve_node_declarations(
    compiled: &CompiledStyles,
    tree: &SceneTree,
    dom_meta: &[NodeDomMeta],
    node_id: usize,
    inline_style: &[(String, String)],
    parent_vars: &HashMap<String, String>,
    parent_font_size: f32,
    vw: f32,
    vh: f32,
) -> (Vec<(String, String)>, HashMap<String, String>) {
    let raw_decls =
        compiled
            .index
            .resolve_declarations_for_node(tree, dom_meta, node_id, inline_style);

    let next_vars = build_element_vars(parent_vars, &raw_decls);

    // Resolve `font-size` first so `em` units inside `calc()` on the same node see updated `em`.
    let mut local_fs = parent_font_size;
    for (k, v) in &raw_decls {
        if k == "font-size" {
            if let Some(resolved_v) = resolve_var_references(v, &next_vars) {
                let simplified = simplify_math_in_value(&resolved_v, local_fs, 16.0, vw, vh);
                if let Some(px) = parse_font_size_quick(&simplified, local_fs, vw, vh) {
                    local_fs = px;
                }
            }
        }
    }

    let mut out = Vec::with_capacity(raw_decls.len());
    for (k, v) in raw_decls {
        if k.starts_with("--") {
            continue;
        }
        if let Some(resolved_v) = resolve_var_references(&v, &next_vars) {
            let simplified = simplify_math_in_value(&resolved_v, local_fs, 16.0, vw, vh);
            if !simplified.is_empty() {
                out.push((k, simplified));
            }
        }
    }
    (out, next_vars)
}

fn parse_font_size_quick(s: &str, parent_fs: f32, vw: f32, vh: f32) -> Option<f32> {
    let lower = s.trim().to_ascii_lowercase();
    if let Some(px) = lower.strip_suffix("px") {
        px.trim().parse::<f32>().ok().map(|v| v.max(0.0))
    } else if let Some(rem) = lower.strip_suffix("rem") {
        rem.trim().parse::<f32>().ok().map(|v| (v * 16.0).max(0.0))
    } else if let Some(em) = lower.strip_suffix("em") {
        em.trim()
            .parse::<f32>()
            .ok()
            .map(|v| (v * parent_fs).max(0.0))
    } else if let Some(v) = lower.strip_suffix("vw") {
        v.trim()
            .parse::<f32>()
            .ok()
            .map(|x| (x * 0.01 * vw).max(0.0))
    } else if let Some(v) = lower.strip_suffix("vh") {
        v.trim()
            .parse::<f32>()
            .ok()
            .map(|x| (x * 0.01 * vh).max(0.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use rsmotion_scene::{NodeKind, SceneNode};

    use super::*;

    fn make_tree(nodes: Vec<SceneNode>, css: &str) -> SceneTree {
        SceneTree {
            width: 1920,
            height: 1080,
            frame: 0,
            root: 0,
            nodes,
            fonts: vec![],
            stylesheets: vec![StyleSheetSource {
                source: "inline:0".to_string(),
                css: css.to_string(),
            }],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        }
    }

    #[test]
    fn layer_order_specificity_and_important_follow_cascade_level_5() {
        let css = r#"
            @layer base, components, utilities;
            @layer utilities {
                .bg-\[\#0f172a\] { background-color: #0f172a; }
                .text-imp { color: #ff0000 !important; }
            }
            @layer base {
                div.card { background-color: #111111; color: #00ff00 !important; }
            }
            /* Unlayered rule beats @layer normal even with lower specificity,
               but earlier @layer !important beats later @layer !important and unlayered !important */
            div { background-color: #222222; color: #0000ff !important; }
        "#;
        let tree = make_tree(
            vec![SceneNode {
                id: 0,
                node: NodeKind::Element {
                    tag: "div".into(),
                    classes: vec!["card".into(), "bg-[#0f172a]".into(), "text-imp".into()],
                    inline_style: vec![("color".into(), "#123456".into())],
                    attrs: vec![],
                    children: vec![],
                },
            }],
            css,
        );

        let compiled = CompiledStyles::compile(&tree.stylesheets, 1920.0, 1080.0);
        let meta = precompute_dom_meta(&tree);
        let (decls, _) = resolve_node_declarations(
            &compiled,
            &tree,
            &meta,
            0,
            &[("color".into(), "#123456".into())],
            compiled.property_initials(),
            16.0,
            1920.0,
            1080.0,
        );

        let map: HashMap<String, String> = decls.into_iter().collect();
        // Unlayered `div` beats layered `.bg-[#0f172a]` when both are normal
        assert_eq!(
            map.get("background-color").map(String::as_str),
            Some("#222222")
        );
        // Earlier `@layer base` `!important` beats later `@layer utilities` `!important` & inline normal
        assert_eq!(map.get("color").map(String::as_str), Some("#00ff00"));
    }

    #[test]
    fn selectors_combinators_nth_child_escaped_tailwind_and_vars_calc() {
        let css = r#"
            @property --spacing { syntax: "<length>"; inherits: true; initial-value: 0.25rem; }
            :root { --accent: #38bdf8; }
            @media (min-width: 1280px) {
                .grid-3 { display: grid; gap: calc(var(--spacing) * 6); }
            }
            .grid-3 > .item:nth-child(2) {
                padding: calc(var(--spacing) * 8);
                border-color: var(--accent, #ffffff);
                width: clamp(100px, 50vw, 900px);
            }
            .w-1\/2 { opacity: 0.75; }
            :where(.grid-3 > .item) { z-index: 2; }
            .item:not(:first-child) { letter-spacing: 2px; }
        "#;

        let tree = make_tree(
            vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec!["grid-3".into()],
                        inline_style: vec![],
                        attrs: vec![("id".into(), "root".into())],
                        children: vec![1, 2],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec!["item".into()],
                        inline_style: vec![],
                        attrs: vec![],
                        children: vec![],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec!["item".into(), "w-1/2".into()],
                        inline_style: vec![],
                        attrs: vec![],
                        children: vec![],
                    },
                },
            ],
            css,
        );

        let compiled = CompiledStyles::compile(&tree.stylesheets, 1920.0, 1080.0);
        let meta = precompute_dom_meta(&tree);

        let (root_decls, root_vars) = resolve_node_declarations(
            &compiled,
            &tree,
            &meta,
            0,
            &[],
            compiled.property_initials(),
            16.0,
            1920.0,
            1080.0,
        );
        let root_map: HashMap<String, String> = root_decls.into_iter().collect();
        assert_eq!(root_map.get("display").map(String::as_str), Some("grid"));
        assert_eq!(root_map.get("gap").map(String::as_str), Some("24px"));

        let (child2_decls, _) = resolve_node_declarations(
            &compiled,
            &tree,
            &meta,
            2,
            &[],
            &root_vars,
            16.0,
            1920.0,
            1080.0,
        );
        let c2_map: HashMap<String, String> = child2_decls.into_iter().collect();
        assert_eq!(c2_map.get("padding").map(String::as_str), Some("32px"));
        assert_eq!(
            c2_map.get("border-color").map(String::as_str),
            Some("#38bdf8")
        );
        assert_eq!(c2_map.get("width").map(String::as_str), Some("900px"));
        assert_eq!(c2_map.get("opacity").map(String::as_str), Some("0.75"));
        assert_eq!(c2_map.get("z-index").map(String::as_str), Some("2"));
        assert_eq!(
            c2_map.get("letter-spacing").map(String::as_str),
            Some("2px")
        );
    }

    #[test]
    fn font_face_and_var_cycles_do_not_panic() {
        let css = r#"
            @font-face {
                font-family: "TestFont";
                font-weight: 700;
                font-style: normal;
                src: url("/fonts/test.ttf") format("truetype");
            }
            .cyclic {
                --a: var(--b);
                --b: var(--a);
                color: var(--a, #abcdef);
            }
        "#;
        let tree = make_tree(
            vec![SceneNode {
                id: 0,
                node: NodeKind::Element {
                    tag: "div".into(),
                    classes: vec!["cyclic".into()],
                    inline_style: vec![],
                    attrs: vec![],
                    children: vec![],
                },
            }],
            css,
        );
        let compiled = CompiledStyles::compile(&tree.stylesheets, 1920.0, 1080.0);
        assert_eq!(compiled.font_faces().len(), 1);
        assert_eq!(compiled.font_faces()[0].family, "TestFont");
        assert_eq!(compiled.font_faces()[0].src_url, "/fonts/test.ttf");

        let meta = precompute_dom_meta(&tree);
        let (decls, _) = resolve_node_declarations(
            &compiled,
            &tree,
            &meta,
            0,
            &[],
            compiled.property_initials(),
            16.0,
            1920.0,
            1080.0,
        );
        let map: HashMap<String, String> = decls.into_iter().collect();
        assert_eq!(map.get("color").map(String::as_str), Some("#abcdef"));
    }

    #[test]
    fn preserves_distinct_font_weights_sharing_variable_font_url() {
        let css = r#"
            @font-face {
                font-family: 'Inter';
                font-style: normal;
                font-weight: 300;
                src: url(https://fonts.gstatic.com/s/inter/v20/inter-latin.woff2) format('woff2');
                unicode-range: U+0000-00FF;
            }
            @font-face {
                font-family: 'Inter';
                font-style: normal;
                font-weight: 700;
                src: url(https://fonts.gstatic.com/s/inter/v20/inter-latin.woff2) format('woff2');
                unicode-range: U+0000-00FF;
            }
        "#;
        let tree = make_tree(vec![], css);
        let compiled = CompiledStyles::compile(&tree.stylesheets, 1200.0, 627.0);
        assert_eq!(compiled.font_faces().len(), 2);
        assert_eq!(compiled.font_faces()[0].weight.as_deref(), Some("300"));
        assert_eq!(compiled.font_faces()[1].weight.as_deref(), Some("700"));
    }
}
