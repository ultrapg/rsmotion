pub mod error;
pub mod probe;
pub mod static_scan;

pub use error::{AnalyzeError, Result};
pub use probe::{
    analyze_bundle, analyze_bundle_with_offline, analyze_composition_for_dispatch,
    analyze_composition_for_dispatch_with_offline, format_composition_explanation, sample_frames,
    BundleAnalysisReport, CompositionAnalysis, PreparedTier1Host, SingleAnalysisOutcome,
};
pub use static_scan::{
    resolve_local_bundle_dir, scan_bundle_static, StaticBundleReport, StaticSignal,
};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn fixtures_bundle_str() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/build")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn static_scan_finds_bundle_hints_without_external_css() {
        let report = scan_bundle_static(&fixtures_bundle_str());
        assert!(!report.is_remote_url);
        assert!(report.bundle_dir.is_some());
        assert!(!report.script_files.is_empty());
        assert!(report.external_stylesheets.is_empty());
        let feats: Vec<&str> = report.signals.iter().map(|s| s.feature.as_str()).collect();
        assert!(feats.contains(&"backdrop-filter"), "signals={feats:?}");
        assert!(feats.contains(&"mix-blend-mode"), "signals={feats:?}");
        assert!(feats.contains(&"canvas-context"), "signals={feats:?}");
    }

    #[test]
    fn analyzes_all_18_fixtures_accurately() {
        let _guard = TEST_LOCK.lock().unwrap();
        let report = analyze_bundle(&fixtures_bundle_str(), None, &json!({}), 15_000).unwrap();
        assert_eq!(report.compositions.len(), 18);

        let expected_tier1 = [
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
        ];

        for id in expected_tier1 {
            let item = report
                .compositions
                .iter()
                .find(|c| c.composition_id == id)
                .unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(
                item.chosen_tier, 1,
                "{id} routed to tier {} with unsupported {:?}: {:?}",
                item.chosen_tier, item.unsupported_features, item.reasons
            );
            assert!(item.unsupported_features.is_empty());
        }

        let webgl = report
            .compositions
            .iter()
            .find(|c| c.composition_id == "unsupported-webgl")
            .expect("missing unsupported-webgl");
        assert_eq!(webgl.chosen_tier, 2);
        assert_eq!(
            webgl.unsupported_features,
            vec!["canvas-webgl:canvas".to_string()],
            "webgl={:?}",
            webgl.unsupported_features
        );
    }

    #[test]
    fn remote_url_routes_to_tier2_and_video_overlay_reuses_host() {
        let _guard = TEST_LOCK.lock().unwrap();
        let remote = analyze_composition_for_dispatch(
            "http://127.0.0.1:9999",
            "text-basic",
            &json!({}),
            10_000,
            None,
            None,
        )
        .unwrap();
        assert!(remote.static_scan.is_remote_url);
        assert_eq!(remote.analysis.chosen_tier, 2);
        assert!(remote.prepared_tier1.is_none());

        let outcome = analyze_composition_for_dispatch(
            &fixtures_bundle_str(),
            "video-overlay",
            &json!({}),
            15_000,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            outcome.analysis.chosen_tier, 1,
            "video-overlay analysis={:?}",
            outcome.analysis
        );
        assert!(outcome.analysis.has_video);
        assert!(outcome.analysis.has_audio);
        assert!(outcome.prepared_tier1.is_some());
    }
}
