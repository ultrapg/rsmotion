pub mod color4;
pub mod effects_parse;
pub mod engine;
pub mod error;
pub mod grid_parse;
pub mod style;

pub use engine::{compute_layout, LayoutContent, LayoutMeasurer, LayoutNode, LayoutTree};
pub use error::{LayoutError, Result};
pub use style::{
    parse_color, BackgroundImage, BorderSide, BoxShadow, ClipRadius, ColorStop, ComputedStyle,
    CssBlendMode, CssClipPath, CssColor, CssFilter, CssMaskImage, FontSlantMode, LengthPercent,
    LineHeight, LinearGradient, ObjectFit, RadialGradient, TextAlignMode, TextDecorationMode,
    TextShadow, TextTransformMode, TransformOp, VisibilityMode, WhiteSpaceMode,
};

#[cfg(test)]
mod tests {
    use rsmotion_scene::{NodeKind, SceneNode, SceneTree};

    use super::*;

    struct DummyMeasurer;

    impl LayoutMeasurer for DummyMeasurer {
        fn measure_text(
            &self,
            text: &str,
            style: &ComputedStyle,
            _max_width: Option<f32>,
        ) -> (f32, f32) {
            (
                text.len() as f32 * style.font_size * 0.6,
                style.font_size * 1.2,
            )
        }
    }

    #[test]
    fn parses_colors_gradients_transforms_and_shorthands() {
        assert_eq!(
            parse_color("#0f172a", CssColor::BLACK),
            Some(CssColor::rgba(15, 23, 42, 255))
        );
        assert_eq!(
            parse_color("rgba(15, 23, 42, 0.5)", CssColor::BLACK),
            Some(CssColor::rgba(15, 23, 42, 128))
        );
        assert_eq!(
            parse_color("hsl(120, 100%, 50%)", CssColor::BLACK),
            Some(CssColor::rgba(0, 255, 0, 255))
        );

        let st = ComputedStyle::for_element(
            None,
            "div",
            &[],
            &[
                (
                    "background".into(),
                    "linear-gradient(150deg, #ec4899 0%, #8b5cf6 50%, #06b6d4 100%)".into(),
                ),
                (
                    "transform".into(),
                    "translate(100px, -20px) rotate(45deg) scale(1.2) skewX(-12deg)".into(),
                ),
                ("padding".into(), "28px 44px".into()),
                ("border".into(), "6px solid #52525b".into()),
                (
                    "box-shadow".into(),
                    "0px 28px 60px rgba(0, 0, 0, 0.65)".into(),
                ),
            ],
            1920.0,
            1080.0,
        );
        assert!(
            matches!(st.background_image, BackgroundImage::Linear(ref g) if (g.angle_deg - 150.0).abs() < 1e-3 && g.stops.len() == 3)
        );
        assert_eq!(st.transforms.len(), 4);
        assert_eq!(st.border_top.width, 6.0);
        assert_eq!(st.box_shadows.len(), 1);
    }

    #[test]
    fn merges_jsx_text_runs_and_centers_flexbox() {
        let scene = SceneTree {
            width: 1920,
            height: 1080,
            frame: 30,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("display".into(), "flex".into()),
                            ("justify-content".into(), "center".into()),
                            ("align-items".into(), "center".into()),
                            ("font-family".into(), "\"DejaVu Sans\", sans-serif".into()),
                        ],
                        attrs: vec![],
                        children: vec![1],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "p".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("margin".into(), "0".into()),
                            ("padding".into(), "10px 20px".into()),
                            ("font-size".into(), "40px".into()),
                        ],
                        attrs: vec![],
                        children: vec![2, 3],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Text("Frame ".into()),
                },
                SceneNode {
                    id: 3,
                    node: NodeKind::Text("30".into()),
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let layout = compute_layout(&scene, &DummyMeasurer).unwrap();
        assert_eq!(layout.nodes.len(), 2);
        let root_children = &layout.nodes[layout.root].children;
        assert_eq!(root_children.len(), 1);
        let p = &layout.nodes[root_children[0]];
        assert_eq!(p.content, LayoutContent::Text("Frame 30".to_string()));
        assert_eq!(p.padding, [10.0, 20.0, 10.0, 20.0]);
        // Centered horizontally & vertically inside 1920x1080
        assert!((p.x + p.width * 0.5 - 960.0).abs() <= 1.0);
        assert!((p.y + p.height * 0.5 - 540.0).abs() <= 1.0);
    }

    #[test]
    fn parses_oklch_color_mix_effects_3d_and_computes_css_grid() {
        let sky = parse_color("oklch(0.685 0.169 237.323)", CssColor::BLACK).unwrap();
        assert!(sky.b > sky.r && sky.a == 255, "sky={sky:?}");
        let translucent = parse_color(
            "color-mix(in oklab, #38bdf8 50%, transparent)",
            CssColor::BLACK,
        )
        .unwrap();
        assert_eq!(translucent.a, 128, "translucent={translucent:?}");

        let eff = ComputedStyle::for_element(
            None,
            "div",
            &[],
            &[
                (
                    "filter".into(),
                    "blur(4px) brightness(1.2) drop-shadow(2px 4px 6px rgba(0, 0, 0, 0.5))".into(),
                ),
                ("backdrop-filter".into(), "blur(12px) saturate(1.4)".into()),
                ("mix-blend-mode".into(), "difference".into()),
                (
                    "clip-path".into(),
                    "polygon(0% 0%, 100% 0%, 80% 100%, 0% 100%)".into(),
                ),
                (
                    "mask-image".into(),
                    "linear-gradient(to bottom, #000000 0%, transparent 100%)".into(),
                ),
                ("background-clip".into(), "text".into()),
                ("perspective".into(), "900px".into()),
                (
                    "transform".into(),
                    "perspective(800px) rotateY(25deg) translateZ(40px)".into(),
                ),
            ],
            1920.0,
            1080.0,
        );
        assert_eq!(eff.filters.len(), 3);
        assert_eq!(eff.backdrop_filters.len(), 2);
        assert_eq!(eff.mix_blend_mode, CssBlendMode::Difference);
        assert!(matches!(eff.clip_path, CssClipPath::Polygon { .. }));
        assert!(matches!(eff.mask_image, CssMaskImage::Linear(_)));
        assert!(eff.background_clip_text);
        assert_eq!(eff.perspective, Some(900.0));
        assert_eq!(eff.transforms.len(), 3);

        let scene = SceneTree {
            width: 900,
            height: 400,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec!["grid-wrap".into()],
                        inline_style: vec![],
                        attrs: vec![],
                        children: vec![1, 2],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec!["span-two".into()],
                        inline_style: vec![("height".into(), "100px".into())],
                        attrs: vec![],
                        children: vec![],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![("height".into(), "100px".into())],
                        attrs: vec![],
                        children: vec![],
                    },
                },
            ],
            fonts: vec![],
            stylesheets: vec![rsmotion_scene::StyleSheetSource {
                source: "inline:0".into(),
                css: r#"
                    :root { --g: 30px; }
                    .grid-wrap {
                        display: grid;
                        grid-template-columns: repeat(3, minmax(0, 1fr));
                        gap: var(--g);
                    }
                    .span-two { grid-column: span 2; }
                "#
                .into(),
            }],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let layout = compute_layout(&scene, &DummyMeasurer).unwrap();
        let kids = &layout.nodes[layout.root].children;
        assert_eq!(kids.len(), 2);
        let c1 = &layout.nodes[kids[0]];
        let c2 = &layout.nodes[kids[1]];
        // 3 columns across 900px with 2x30px gap -> col_w = 280; span-2 = 280*2 + 30 = 590px; col3 starts at 620px
        assert!((c1.width - 590.0).abs() <= 1.0, "c1.width={}", c1.width);
        assert!((c2.width - 280.0).abs() <= 1.0, "c2.width={}", c2.width);
        assert!((c2.x - 620.0).abs() <= 1.0, "c2.x={}", c2.x);

        let text_fill = ComputedStyle::for_element(
            None,
            "span",
            &[],
            &[("-webkit-text-fill-color".into(), "transparent".into())],
            1920.0,
            1080.0,
        );
        assert_eq!(text_fill.color, CssColor::TRANSPARENT);
    }

    struct FixedImageMeasurer(u32, u32);

    impl LayoutMeasurer for FixedImageMeasurer {
        fn measure_text(
            &self,
            text: &str,
            style: &ComputedStyle,
            _max_width: Option<f32>,
        ) -> (f32, f32) {
            (
                text.len() as f32 * style.font_size * 0.6,
                style.font_size * 1.2,
            )
        }

        fn measure_image(&self, _src: &str) -> Option<(u32, u32)> {
            Some((self.0, self.1))
        }
    }

    #[test]
    fn replaced_image_transfers_max_height_across_aspect_ratio_in_flex_row() {
        let scene = SceneTree {
            width: 984,
            height: 300,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("display".into(), "flex".into()),
                            ("flex-direction".into(), "row".into()),
                            ("width".into(), "984px".into()),
                        ],
                        attrs: vec![],
                        children: vec![1, 2],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "img".into(),
                        classes: vec![],
                        inline_style: vec![("max-height".into(), "250px".into())],
                        attrs: vec![("src".into(), "cover.jpg".into())],
                        children: vec![],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("flex-grow".into(), "1".into()),
                            ("height".into(), "100px".into()),
                        ],
                        attrs: vec![],
                        children: vec![],
                    },
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let layout = compute_layout(&scene, &FixedImageMeasurer(640, 640)).unwrap();
        let kids = &layout.nodes[layout.root].children;
        let img = &layout.nodes[kids[0]];
        let title = &layout.nodes[kids[1]];
        assert!((img.width - 250.0).abs() <= 0.5, "img.width={}", img.width);
        assert!(
            (img.height - 250.0).abs() <= 0.5,
            "img.height={}",
            img.height
        );
        assert!((title.x - 250.0).abs() <= 0.5, "title.x={}", title.x);
    }

    #[test]
    fn replaced_image_with_explicit_width_and_height_ignores_intrinsic_aspect_ratio() {
        let scene = SceneTree {
            width: 1920,
            height: 1080,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("width".into(), "1920px".into()),
                            ("height".into(), "1080px".into()),
                        ],
                        attrs: vec![],
                        children: vec![1],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "img".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("width".into(), "1920px".into()),
                            ("height".into(), "1080px".into()),
                        ],
                        attrs: vec![("src".into(), "square.jpg".into())],
                        children: vec![],
                    },
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let layout = compute_layout(&scene, &FixedImageMeasurer(640, 640)).unwrap();
        let img = &layout.nodes[layout.nodes[layout.root].children[0]];
        assert!((img.width - 1920.0).abs() <= 0.5, "img.width={}", img.width);
        assert!(
            (img.height - 1080.0).abs() <= 0.5,
            "img.height={}",
            img.height
        );
    }
}
