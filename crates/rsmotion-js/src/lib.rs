pub mod error;
pub mod ops;
pub mod runtime;

pub use error::{JsHostError, Result};
pub use ops::{resolve_bundle_asset, HostState};
pub use runtime::V8BundleHost;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use rsmotion_scene::NodeKind;
    use serde_json::json;

    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn fixtures_bundle_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/build")
    }

    fn collect_text(tree: &rsmotion_scene::SceneTree) -> Vec<String> {
        tree.nodes
            .iter()
            .filter_map(|n| match &n.node {
                NodeKind::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    fn boot_and_mount(comp_id: &str) -> (V8BundleHost, rsmotion_scene::Tier1Composition) {
        let mut host = V8BundleHost::new(&fixtures_bundle_dir(), &json!({}), 10000).unwrap();
        let comps = host.get_compositions().unwrap();
        let comp = comps
            .into_iter()
            .find(|c| c.config.id == comp_id)
            .unwrap_or_else(|| panic!("missing composition {comp_id}"));
        host.mount_composition(&comp).unwrap();
        (host, comp)
    }

    #[test]
    fn lists_all_18_compositions_in_v8() {
        let _guard = TEST_LOCK.lock().unwrap();
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_test_writer()
            .try_init();
        let mut host = V8BundleHost::new(&fixtures_bundle_dir(), &json!({}), 10000).unwrap();
        let comps = host.get_compositions().unwrap();
        let ids: Vec<&str> = comps.iter().map(|c| c.config.id.as_str()).collect();
        assert_eq!(comps.len(), 18, "ids: {ids:?}");
        for expected in [
            "text-basic",
            "text-animated",
            "captions",
            "image-slideshow",
            "transforms-opacity",
            "gradients-shadows",
            "svg-shapes",
            "chart-bars",
            "video-overlay",
            "audio-mix",
            "fonts-custom",
            "css-tailwind-grid",
            "css-effects-3d",
            "unsupported-3d",
            "canvas-2d",
            "lottie-native",
            "audio-viz",
            "unsupported-webgl",
        ] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
    }

    #[test]
    fn text_basic_and_fonts_custom_and_images_and_svgs_snapshot() {
        let _guard = TEST_LOCK.lock().unwrap();
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_test_writer()
            .try_init();

        // 1. text-basic seeks multiple frames on one mounted host cleanly
        let (mut host, comp) = boot_and_mount("text-basic");
        for frame in [0, 30, 59] {
            let tree = host
                .render_frame_tree(
                    &comp.config.id,
                    comp.config.width,
                    comp.config.height,
                    frame,
                )
                .unwrap();
            assert!(tree.unsupported_features.is_empty());
            let texts = collect_text(&tree).join("");
            assert!(texts.contains("rsmotion Renderer"), "texts={texts}");
            assert!(
                texts.contains(&format!("Frame {frame}")),
                "frame={frame} texts={texts}"
            );
            assert!(
                tree.nodes.iter().any(|n| matches!(
                    &n.node,
                    NodeKind::Element { inline_style, .. }
                        if inline_style.iter().any(|(k, _)| k == "background-color")
                )),
                "expected non-empty inline_style with background-color on text-basic"
            );
        }

        // 2. fonts-custom registers SpaceGroteskLocal
        let (mut host, comp) = boot_and_mount("fonts-custom");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 0)
            .unwrap();
        assert!(
            tree.fonts
                .iter()
                .any(|f| f.family == "SpaceGroteskLocal" && f.resolved_path.is_some()),
            "fonts={:?}",
            tree.fonts
        );

        // 3. image-slideshow frame 30 has two resolved <img> nodes during crossfade
        let (mut host, comp) = boot_and_mount("image-slideshow");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        let img_count = tree
            .nodes
            .iter()
            .filter(|n| matches!(&n.node, NodeKind::Element { tag, .. } if tag == "img"))
            .count();
        assert_eq!(
            img_count, 2,
            "expected 2 <img> elements at frame 30 crossfade"
        );

        // 4. chart-bars preserves SVG case & inherits font-family
        let (mut host, comp) = boot_and_mount("chart-bars");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        let svg_info = tree
            .nodes
            .iter()
            .find_map(|n| match &n.node {
                NodeKind::Svg {
                    xml,
                    width_attr,
                    view_box,
                    ..
                } => Some((xml.clone(), *width_attr, *view_box)),
                _ => None,
            })
            .expect("chart-bars must emit Svg node");
        let (svg_xml, svg_w, svg_vb) = svg_info;
        assert_eq!(svg_w, Some(1500.0), "width_attr={svg_w:?}");
        assert_eq!(
            svg_vb,
            Some((0.0, 0.0, 1500.0, 820.0)),
            "view_box={svg_vb:?}"
        );
        assert!(
            svg_xml.contains("viewBox=\"0 0 1500 820\""),
            "xml={svg_xml}"
        );
        assert!(
            svg_xml.contains("font-family=\"DejaVu Sans, sans-serif\""),
            "xml={svg_xml}"
        );

        // 5. svg-shapes preserves camelCase tags like linearGradient & clipPath
        let (mut host, comp) = boot_and_mount("svg-shapes");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        let svg_xml = tree
            .nodes
            .iter()
            .find_map(|n| match &n.node {
                NodeKind::Svg { xml, .. } => Some(xml.clone()),
                _ => None,
            })
            .expect("svg-shapes must emit Svg node");
        assert!(svg_xml.contains("<linearGradient"), "xml={svg_xml}");
        assert!(svg_xml.contains("<clipPath"), "xml={svg_xml}");

        // 6. unsupported-3d now runs natively on Tier 1 (Canvas 2D + backdrop-filter + mix-blend-mode)
        let (mut host, comp) = boot_and_mount("unsupported-3d");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        assert!(
            tree.unsupported_features.is_empty(),
            "unsupported-3d feats={:?}",
            tree.unsupported_features
        );
        assert!(
            tree.nodes
                .iter()
                .any(|n| matches!(&n.node, NodeKind::Canvas2d { .. })),
            "unsupported-3d must emit Canvas2d node"
        );

        // 7. canvas-2d emits Canvas2d node cleanly on Tier 1
        let (mut host, comp) = boot_and_mount("canvas-2d");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        assert!(
            tree.unsupported_features.is_empty(),
            "canvas-2d feats={:?}",
            tree.unsupported_features
        );
        assert!(
            tree.nodes
                .iter()
                .any(|n| matches!(&n.node, NodeKind::Canvas2d { .. })),
            "canvas-2d must emit Canvas2d node"
        );

        // 8. lottie-native emits Lottie node & animation asset cleanly on Tier 1
        let (mut host, comp) = boot_and_mount("lottie-native");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        assert!(
            tree.unsupported_features.is_empty(),
            "lottie-native feats={:?}",
            tree.unsupported_features
        );
        assert!(
            tree.nodes
                .iter()
                .any(|n| matches!(&n.node, NodeKind::Lottie { .. })),
            "lottie-native must emit Lottie node"
        );
        assert!(
            !tree.lottie_assets.is_empty(),
            "lottie-native must populate lottie_assets"
        );

        // 9. audio-viz decodes audio via media-utils cleanly on Tier 1
        let (mut host, comp) = boot_and_mount("audio-viz");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        assert!(
            tree.unsupported_features.is_empty(),
            "audio-viz feats={:?}",
            tree.unsupported_features
        );
        assert!(
            tree.nodes
                .iter()
                .any(|n| matches!(&n.node, NodeKind::Svg { xml, .. } if xml.contains("<path"))),
            "audio-viz must emit waveform SVG path"
        );

        // 10. unsupported-webgl still routes to Tier 2 on canvas-webgl
        let (mut host, comp) = boot_and_mount("unsupported-webgl");
        let tree = host
            .render_frame_tree(&comp.config.id, comp.config.width, comp.config.height, 30)
            .unwrap();
        let feats: Vec<&str> = tree
            .unsupported_features
            .iter()
            .map(|u| u.feature.as_str())
            .collect();
        assert_eq!(feats, vec!["canvas-webgl"], "feats={feats:?}");

        // 7. video-overlay emits rsmotion-video://frame <img> and video audio_assets
        let (mut host, comp) = boot_and_mount("video-overlay");
        for frame in [0, 30] {
            let tree = host
                .render_frame_tree(
                    &comp.config.id,
                    comp.config.width,
                    comp.config.height,
                    frame,
                )
                .unwrap();
            assert!(
                tree.unsupported_features.is_empty(),
                "unexpected unsupported: {:?}",
                tree.unsupported_features
            );
            let has_video_frame_img = tree.nodes.iter().any(|n| {
                matches!(
                    &n.node,
                    NodeKind::Element { tag, attrs, .. }
                        if tag == "img"
                            && attrs.iter().any(|(k, v)| k == "src" && v.starts_with("rsmotion-video://frame?"))
                )
            });
            assert!(
                has_video_frame_img,
                "video-overlay frame {frame} missing rsmotion-video://frame <img>"
            );
            assert!(
                tree.audio_assets
                    .iter()
                    .any(|a| a.src.contains("sample-video.mp4")),
                "video-overlay frame {frame} audio_assets={:?}",
                tree.audio_assets
            );
        }

        // 8. audio-mix emits audio_assets with 0.2 -> 1.0 -> 0.2 gain envelope
        let (mut host, comp) = boot_and_mount("audio-mix");
        for (frame, expected_vol) in [(0u32, 0.2_f64), (30, 1.0), (59, 0.2)] {
            let tree = host
                .render_frame_tree(
                    &comp.config.id,
                    comp.config.width,
                    comp.config.height,
                    frame,
                )
                .unwrap();
            assert!(tree.unsupported_features.is_empty());
            assert_eq!(
                tree.audio_assets.len(),
                1,
                "audio-mix frame {frame} audio_assets={:?}",
                tree.audio_assets
            );
            assert!(
                (tree.audio_assets[0].volume - expected_vol).abs() < 0.05,
                "frame {frame} vol={} expected={expected_vol}",
                tree.audio_assets[0].volume
            );
        }
    }
}
