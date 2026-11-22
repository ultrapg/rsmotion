use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use rsmotion_core::FrameRange;
use rsmotion_js::{resolve_bundle_asset, V8BundleHost};
use rsmotion_media::{fetch_remote_asset, is_remote_http_url, CHROME_WOFF2_UA};
use rsmotion_scene::{NodeKind, SceneTree, Tier1Composition};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AnalyzeError, Result};
use crate::static_scan::{scan_bundle_static, StaticBundleReport};

fn unresolved_external_stylesheets(
    bundle_dir: &Path,
    hrefs: &[String],
    offline: bool,
) -> Vec<String> {
    let Ok(canon_bundle) = std::fs::canonicalize(bundle_dir) else {
        return hrefs.to_vec();
    };
    hrefs
        .iter()
        .filter(|href| {
            let trimmed = href.trim();
            if is_remote_http_url(trimmed) {
                return fetch_remote_asset(trimmed, offline, Some(CHROME_WOFF2_UA)).is_err();
            }
            resolve_bundle_asset(bundle_dir, trimmed).is_none_or(|p| !p.starts_with(&canon_bundle))
        })
        .cloned()
        .collect()
}

/// Per-composition dispatch decision produced by the static + V8 dynamic analyzer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionAnalysis {
    pub composition_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_in_frames: u32,
    pub chosen_tier: u8,
    pub tier_label: String,
    pub expected_fps_range: String,
    pub sampled_frames: Vec<u32>,
    pub unsupported_features: Vec<String>,
    pub has_video: bool,
    pub has_audio: bool,
    pub has_svg: bool,
    pub reasons: Vec<String>,
    pub probe_ms: f64,
}

/// Complete analysis of all (or filtered) compositions inside a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleAnalysisReport {
    pub serve_url: String,
    pub static_scan: StaticBundleReport,
    pub compositions: Vec<CompositionAnalysis>,
    pub total_ms: f64,
}

/// Already-booted and mounted V8 runtime that can immediately proceed to Tier 1 rendering.
pub struct PreparedTier1Host {
    pub host: V8BundleHost,
    pub composition: Tier1Composition,
}

/// Outcome of analyzing one composition before automatic tier dispatch.
pub struct SingleAnalysisOutcome {
    pub static_scan: StaticBundleReport,
    pub analysis: CompositionAnalysis,
    pub prepared_tier1: Option<PreparedTier1Host>,
}

/// Selects representative frames (`start`, `mid`, `end` or an explicit still frame) to probe.
pub fn sample_frames(
    duration_in_frames: u32,
    frame_range: Option<FrameRange>,
    explicit_frame: Option<u32>,
) -> Vec<u32> {
    let last_valid = duration_in_frames.saturating_sub(1);
    if let Some(frame) = explicit_frame {
        return vec![frame.min(last_valid)];
    }

    let (start, end) = match frame_range {
        Some(r) => (r.start.min(last_valid), r.end.min(last_valid)),
        None => (0, last_valid),
    };
    let mid = start + (end.saturating_sub(start)) / 2;

    let mut set = BTreeSet::new();
    set.insert(start);
    set.insert(mid);
    set.insert(end);
    set.into_iter().collect()
}

fn expected_fps_for(tier: u8, has_video: bool, has_svg: bool) -> String {
    match tier {
        1 if has_video => "15–25 fps (Tier 1 GPU + native video)".to_string(),
        1 if has_svg => "19–42 fps (Tier 1 GPU + Skia SVG)".to_string(),
        1 => "25–40 fps (Tier 1 GPU)".to_string(),
        2 => "8–13 fps (Tier 2 Chrome CDP)".to_string(),
        _ => "5–12 fps (Tier 3 @remotion/renderer)".to_string(),
    }
}

fn inspect_scene_capabilities(scene: &SceneTree) -> (bool, bool, bool) {
    let mut has_video = false;
    let mut has_svg = false;
    for node in &scene.nodes {
        match &node.node {
            NodeKind::Svg { .. } => has_svg = true,
            NodeKind::Element { tag, attrs, .. } => {
                if tag == "video"
                    || (tag == "img"
                        && attrs
                            .iter()
                            .any(|(k, v)| k == "src" && v.starts_with("rsmotion-video://")))
                {
                    has_video = true;
                }
            }
            NodeKind::Text(_) | NodeKind::Canvas2d { .. } | NodeKind::Lottie { .. } => {}
        }
    }
    let has_audio = !scene.audio_assets.is_empty();
    (has_video, has_audio, has_svg)
}

#[allow(clippy::too_many_arguments)]
fn build_fallback_analysis(
    composition_id: String,
    width: u32,
    height: u32,
    fps: f64,
    duration_in_frames: u32,
    sampled_frames: Vec<u32>,
    unsupported_features: Vec<String>,
    has_video: bool,
    has_audio: bool,
    has_svg: bool,
    reasons: Vec<String>,
    started: Instant,
) -> CompositionAnalysis {
    CompositionAnalysis {
        composition_id,
        width,
        height,
        fps,
        duration_in_frames,
        chosen_tier: 2,
        tier_label: "tier2-cdp".to_string(),
        expected_fps_range: expected_fps_for(2, has_video, has_svg),
        sampled_frames,
        unsupported_features,
        has_video,
        has_audio,
        has_svg,
        reasons,
        probe_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}

fn probe_mounted_composition(
    host: &mut V8BundleHost,
    comp: &Tier1Composition,
    frames: &[u32],
    started: Instant,
) -> CompositionAnalysis {
    let mut unsupported_set = BTreeSet::new();
    let mut has_video = false;
    let mut has_audio = false;
    let mut has_svg = false;

    for &frame_idx in frames {
        match host.render_frame_tree(
            &comp.config.id,
            comp.config.width,
            comp.config.height,
            frame_idx,
        ) {
            Ok(scene) => {
                for u in &scene.unsupported_features {
                    unsupported_set.insert(format!("{}:{}", u.feature, u.tag));
                }
                let (vid, aud, svg) = inspect_scene_capabilities(&scene);
                has_video |= vid;
                has_audio |= aud;
                has_svg |= svg;
            }
            Err(err) => {
                return build_fallback_analysis(
                    comp.config.id.clone(),
                    comp.config.width,
                    comp.config.height,
                    comp.config.fps,
                    comp.config.duration_in_frames,
                    frames.to_vec(),
                    unsupported_set.into_iter().collect(),
                    has_video,
                    has_audio,
                    has_svg,
                    vec![format!(
                        "Tier 1 V8 probe failed on frame {frame_idx}: {err}"
                    )],
                    started,
                );
            }
        }
    }

    let unsupported_features: Vec<String> = unsupported_set.into_iter().collect();
    if unsupported_features.is_empty() {
        let mut reasons = vec![format!(
            "all sampled frames {frames:?} supported natively in Tier 1 (V8 + Taffy + GPU Skia)"
        )];
        if has_video {
            reasons.push("native FFmpeg frame decoder enabled (rsmotion-media)".to_string());
        }
        if has_audio {
            reasons.push("native multi-track audio mixer enabled (AAC)".to_string());
        }
        if has_svg {
            reasons.push("native Skia SVG DOM rasterizer enabled".to_string());
        }
        CompositionAnalysis {
            composition_id: comp.config.id.clone(),
            width: comp.config.width,
            height: comp.config.height,
            fps: comp.config.fps,
            duration_in_frames: comp.config.duration_in_frames,
            chosen_tier: 1,
            tier_label: "tier1-native".to_string(),
            expected_fps_range: expected_fps_for(1, has_video, has_svg),
            sampled_frames: frames.to_vec(),
            unsupported_features,
            has_video,
            has_audio,
            has_svg,
            reasons,
            probe_ms: started.elapsed().as_secs_f64() * 1000.0,
        }
    } else {
        let reasons = vec![format!(
            "unsupported Tier 1 features detected on sampled frames {frames:?}: {}",
            unsupported_features.join(", ")
        )];
        build_fallback_analysis(
            comp.config.id.clone(),
            comp.config.width,
            comp.config.height,
            comp.config.fps,
            comp.config.duration_in_frames,
            frames.to_vec(),
            unsupported_features,
            has_video,
            has_audio,
            has_svg,
            reasons,
            started,
        )
    }
}

/// Analyzes one composition and, when Tier 1 is chosen, keeps the mounted `V8BundleHost` for reuse.
pub fn analyze_composition_for_dispatch(
    serve_url: &str,
    composition_id: &str,
    input_props: &Value,
    timeout_ms: u64,
    frame_range: Option<FrameRange>,
    explicit_frame: Option<u32>,
) -> Result<SingleAnalysisOutcome> {
    analyze_composition_for_dispatch_with_offline(
        serve_url,
        composition_id,
        input_props,
        timeout_ms,
        frame_range,
        explicit_frame,
        false,
    )
}

/// Analyzes one composition with explicit offline cache policy (`offline = true` forbids remote fetches).
pub fn analyze_composition_for_dispatch_with_offline(
    serve_url: &str,
    composition_id: &str,
    input_props: &Value,
    timeout_ms: u64,
    frame_range: Option<FrameRange>,
    explicit_frame: Option<u32>,
    offline: bool,
) -> Result<SingleAnalysisOutcome> {
    let t0 = Instant::now();
    let static_scan = scan_bundle_static(serve_url);

    if static_scan.is_remote_url {
        return Ok(SingleAnalysisOutcome {
            analysis: build_fallback_analysis(
                composition_id.to_string(),
                0,
                0,
                30.0,
                0,
                Vec::new(),
                vec!["remote-serve-url".to_string()],
                false,
                false,
                false,
                vec!["remote HTTP serveUrl requires Tier 2 browser host".to_string()],
                t0,
            ),
            static_scan,
            prepared_tier1: None,
        });
    }

    let Some(bundle_dir) = static_scan.bundle_dir.clone() else {
        return Ok(SingleAnalysisOutcome {
            analysis: build_fallback_analysis(
                composition_id.to_string(),
                0,
                0,
                30.0,
                0,
                Vec::new(),
                vec!["missing-bundle".to_string()],
                false,
                false,
                false,
                vec![format!("missing local bundle directory at {serve_url}")],
                t0,
            ),
            static_scan,
            prepared_tier1: None,
        });
    };

    let unresolved_sheets =
        unresolved_external_stylesheets(&bundle_dir, &static_scan.external_stylesheets, offline);
    if !unresolved_sheets.is_empty() {
        return Ok(SingleAnalysisOutcome {
            analysis: build_fallback_analysis(
                composition_id.to_string(),
                0,
                0,
                30.0,
                0,
                Vec::new(),
                vec!["external-stylesheet".to_string()],
                false,
                false,
                false,
                vec![format!(
                    "remote/missing <link rel=\"stylesheet\"> requires Tier 2 browser CSS engine: {}",
                    unresolved_sheets.join(", ")
                )],
                t0,
            ),
            static_scan,
            prepared_tier1: None,
        });
    }

    let mut host =
        match V8BundleHost::new_with_options(&bundle_dir, input_props, timeout_ms, offline) {
            Ok(h) => h,
            Err(err) => {
                return Ok(SingleAnalysisOutcome {
                    analysis: build_fallback_analysis(
                        composition_id.to_string(),
                        0,
                        0,
                        30.0,
                        0,
                        Vec::new(),
                        vec!["v8-boot-error".to_string()],
                        false,
                        false,
                        false,
                        vec![format!("Tier 1 V8 boot failed, routing to Tier 2: {err}")],
                        t0,
                    ),
                    static_scan,
                    prepared_tier1: None,
                });
            }
        };

    let comps = match host.get_compositions() {
        Ok(c) => c,
        Err(err) => {
            return Ok(SingleAnalysisOutcome {
                analysis: build_fallback_analysis(
                    composition_id.to_string(),
                    0,
                    0,
                    30.0,
                    0,
                    Vec::new(),
                    vec!["v8-eval-error".to_string()],
                    false,
                    false,
                    false,
                    vec![format!(
                        "Tier 1 V8 getStaticCompositions() failed, routing to Tier 2: {err}"
                    )],
                    t0,
                ),
                static_scan,
                prepared_tier1: None,
            });
        }
    };

    let comp = comps
        .into_iter()
        .find(|c| c.config.id == composition_id)
        .ok_or_else(|| AnalyzeError::CompositionNotFound(composition_id.to_string()))?;

    let frames = sample_frames(comp.config.duration_in_frames, frame_range, explicit_frame);
    if let Err(err) = host.mount_composition(&comp) {
        return Ok(SingleAnalysisOutcome {
            analysis: build_fallback_analysis(
                comp.config.id.clone(),
                comp.config.width,
                comp.config.height,
                comp.config.fps,
                comp.config.duration_in_frames,
                frames,
                vec!["v8-mount-error".to_string()],
                false,
                false,
                false,
                vec![format!("Tier 1 mount failed, routing to Tier 2: {err}")],
                t0,
            ),
            static_scan,
            prepared_tier1: None,
        });
    }

    let analysis = probe_mounted_composition(&mut host, &comp, &frames, t0);
    let prepared_tier1 = (analysis.chosen_tier == 1).then_some(PreparedTier1Host {
        host,
        composition: comp,
    });

    Ok(SingleAnalysisOutcome {
        static_scan,
        analysis,
        prepared_tier1,
    })
}

/// Analyzes every composition (or one filtered composition) in `serve_url`.
pub fn analyze_bundle(
    serve_url: &str,
    composition_filter: Option<&str>,
    input_props: &Value,
    timeout_ms: u64,
) -> Result<BundleAnalysisReport> {
    analyze_bundle_with_offline(
        serve_url,
        composition_filter,
        input_props,
        timeout_ms,
        false,
    )
}

/// Analyzes every composition (or one filtered composition) in `serve_url` with explicit offline policy.
pub fn analyze_bundle_with_offline(
    serve_url: &str,
    composition_filter: Option<&str>,
    input_props: &Value,
    timeout_ms: u64,
    offline: bool,
) -> Result<BundleAnalysisReport> {
    let t0 = Instant::now();
    let static_scan = scan_bundle_static(serve_url);

    let Some(bundle_dir) = static_scan.bundle_dir.clone() else {
        return Err(AnalyzeError::BundleInvalid(format!(
            "rsmotion explain requires a local bundle directory (got `{serve_url}`)"
        )));
    };

    let mut list_host =
        V8BundleHost::new_with_options(&bundle_dir, input_props, timeout_ms, offline)?;
    let all_comps = list_host.get_compositions()?;
    drop(list_host);

    let selected: Vec<Tier1Composition> = match composition_filter {
        Some(target) => {
            let found = all_comps
                .into_iter()
                .filter(|c| c.config.id == target)
                .collect::<Vec<_>>();
            if found.is_empty() {
                return Err(AnalyzeError::CompositionNotFound(target.to_string()));
            }
            found
        }
        None => all_comps,
    };

    let unresolved_sheets =
        unresolved_external_stylesheets(&bundle_dir, &static_scan.external_stylesheets, offline);
    let mut results = Vec::with_capacity(selected.len());
    for comp in selected {
        let t_comp = Instant::now();
        let frames = sample_frames(comp.config.duration_in_frames, None, None);

        if !unresolved_sheets.is_empty() {
            results.push(build_fallback_analysis(
                comp.config.id,
                comp.config.width,
                comp.config.height,
                comp.config.fps,
                comp.config.duration_in_frames,
                frames,
                vec!["external-stylesheet".to_string()],
                false,
                false,
                false,
                vec![format!(
                    "remote/missing <link rel=\"stylesheet\"> requires Tier 2 browser CSS engine: {}",
                    unresolved_sheets.join(", ")
                )],
                t_comp,
            ));
            continue;
        }

        let mut host =
            V8BundleHost::new_with_options(&bundle_dir, input_props, timeout_ms, offline)?;
        host.mount_composition(&comp)?;
        results.push(probe_mounted_composition(&mut host, &comp, &frames, t_comp));
    }

    Ok(BundleAnalysisReport {
        serve_url: serve_url.to_string(),
        static_scan,
        compositions: results,
        total_ms: t0.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Formats a human-readable explanation for `--explain` and `rsmotion explain`.
pub fn format_composition_explanation(
    analysis: &CompositionAnalysis,
    static_scan: &StaticBundleReport,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "[explain] composition={} -> tier={} ({}) | expected={} | probe={:.1}ms",
        analysis.composition_id,
        analysis.chosen_tier,
        analysis.tier_label,
        analysis.expected_fps_range,
        analysis.probe_ms
    ));
    if !analysis.sampled_frames.is_empty() {
        lines.push(format!(
            "[explain] sampled frames={:?} | video={} | audio={} | svg={}",
            analysis.sampled_frames, analysis.has_video, analysis.has_audio, analysis.has_svg
        ));
    }
    if !analysis.unsupported_features.is_empty() {
        lines.push(format!(
            "[explain] unsupported in Tier 1: {}",
            analysis.unsupported_features.join(", ")
        ));
    }
    for reason in &analysis.reasons {
        lines.push(format!("[explain] reason: {reason}"));
    }
    if !static_scan.signals.is_empty() {
        let hints: Vec<String> = static_scan
            .signals
            .iter()
            .map(|s| format!("{}@{}", s.feature, s.source))
            .collect();
        lines.push(format!("[explain] bundle hints: {}", hints.join(", ")));
    }
    lines.join("\n")
}
