use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rsmotion_media::resolve_or_fetch_asset;
use rsmotion_scene::LottieSource;
use skia_safe::resources::{helpers, ResourceProvider};
use skia_safe::skottie::{Animation, Builder};
use skia_safe::{Canvas, ClipOp, Data, FontMgr, Rect, Typeface};
use tracing::warn;

struct BundleResourceProvider {
    bundle_dir: PathBuf,
    assets_path: Option<String>,
    offline: bool,
    font_mgr: FontMgr,
}

impl ResourceProvider for BundleResourceProvider {
    fn load(&self, resource_path: &str, resource_name: &str) -> Option<Data> {
        let trimmed_path = resource_path.trim();
        let trimmed_name = resource_name.trim();
        if trimmed_name.starts_with("data:") {
            if let helpers::ResourceKind::Base64(data) =
                helpers::identify_resource_kind("", trimmed_name)
            {
                return Some(data);
            }
        }

        let candidates = [
            if trimmed_path.is_empty() {
                trimmed_name.to_string()
            } else {
                format!("{}/{trimmed_name}", trimmed_path.trim_end_matches('/'))
            },
            if let Some(ap) = &self.assets_path {
                format!("{}/{trimmed_name}", ap.trim_end_matches('/'))
            } else {
                trimmed_name.to_string()
            },
            trimmed_name.to_string(),
        ];

        for cand in candidates {
            if let Some(path) = resolve_or_fetch_asset(&self.bundle_dir, &cand, self.offline) {
                if let Ok(bytes) = std::fs::read(&path) {
                    return Some(Data::new_copy(&bytes));
                }
            }
        }
        None
    }

    fn load_typeface(&self, name: &str, url: &str) -> Option<Typeface> {
        helpers::load_typeface(self, &self.font_mgr, name, url)
    }

    fn font_mgr(&self) -> FontMgr {
        self.font_mgr.clone()
    }
}

/// Caches parsed and resource-bound `skia_safe::skottie::Animation` instances across frames.
#[derive(Default)]
pub struct LottieCache {
    animations: HashMap<String, Option<Animation>>,
}

impl LottieCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync_assets(
        &mut self,
        assets: &[LottieSource],
        bundle_dir: &Path,
        offline: bool,
        font_mgr: FontMgr,
    ) {
        for asset in assets {
            if self.animations.contains_key(&asset.id) {
                continue;
            }
            let anim = match std::fs::read_to_string(&asset.resolved_path) {
                Ok(json) => {
                    let provider = BundleResourceProvider {
                        bundle_dir: bundle_dir.to_path_buf(),
                        assets_path: asset.assets_path.clone(),
                        offline,
                        font_mgr: font_mgr.clone(),
                    };
                    let built = Builder::new()
                        .set_font_manager(font_mgr.clone())
                        .set_resource_provider(provider)
                        .make(&json);
                    if built.is_none() {
                        warn!("Skottie failed to parse Lottie animation {}", asset.id);
                    }
                    built
                }
                Err(e) => {
                    warn!(
                        "failed to read cached Lottie JSON {}: {e}",
                        asset.resolved_path.display()
                    );
                    None
                }
            };
            self.animations.insert(asset.id.clone(), anim);
        }
    }

    pub fn render(
        &self,
        canvas: &Canvas,
        animation_id: &str,
        frame: f64,
        dst_rect: Rect,
        preserve_aspect_ratio: Option<&str>,
    ) {
        let Some(Some(anim)) = self.animations.get(animation_id) else {
            return;
        };
        if dst_rect.width() <= 0.0 || dst_rect.height() <= 0.0 {
            return;
        }
        let in_pt = f64::from(anim.in_point());
        let out_pt = f64::from(anim.out_point());
        let seek_pos = (in_pt + frame.max(0.0)).min(out_pt.max(in_pt));
        anim.seek_frame(seek_pos);

        let intrinsic = anim.size();
        let iw = intrinsic.width.max(1.0);
        let ih = intrinsic.height.max(1.0);
        let (fitted, clip_to_box) =
            compute_preserve_aspect_rect(dst_rect, iw, ih, preserve_aspect_ratio);

        canvas.save();
        if clip_to_box {
            canvas.clip_rect(dst_rect, ClipOp::Intersect, true);
        }
        anim.render(canvas, Some(fitted));
        canvas.restore();
    }
}

fn compute_preserve_aspect_rect(
    dst: Rect,
    iw: f32,
    ih: f32,
    preserve_aspect_ratio: Option<&str>,
) -> (Rect, bool) {
    let raw = preserve_aspect_ratio.unwrap_or("xMidYMid meet").trim();
    if raw.eq_ignore_ascii_case("none") {
        return (dst, false);
    }

    let mut align = "xMidYMid";
    let mut meet_or_slice = "meet";
    for tok in raw.split_whitespace() {
        if tok.eq_ignore_ascii_case("slice") {
            meet_or_slice = "slice";
        } else if tok.eq_ignore_ascii_case("meet") {
            meet_or_slice = "meet";
        } else if tok.eq_ignore_ascii_case("none") {
            return (dst, false);
        } else if tok.starts_with('x') || tok.starts_with('X') {
            align = tok;
        }
    }

    let sx = dst.width() / iw;
    let sy = dst.height() / ih;
    let scale = if meet_or_slice == "slice" {
        sx.max(sy)
    } else {
        sx.min(sy)
    };
    let fw = iw * scale;
    let fh = ih * scale;

    let lower = align.to_ascii_lowercase();
    let fx = if lower.contains("xmin") {
        dst.left
    } else if lower.contains("xmax") {
        dst.right - fw
    } else {
        dst.left + (dst.width() - fw) * 0.5
    };
    let fy = if lower.contains("ymin") {
        dst.top
    } else if lower.contains("ymax") {
        dst.bottom - fh
    } else {
        dst.top + (dst.height() - fh) * 0.5
    };

    (Rect::from_xywh(fx, fy, fw, fh), meet_or_slice == "slice")
}
