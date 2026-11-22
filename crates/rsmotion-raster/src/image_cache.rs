use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rsmotion_layout::{ComputedStyle, LayoutMeasurer};
use rsmotion_media::{resolve_or_fetch_asset, VideoDecoderPool};
use skia_safe::{Data, Image};

use crate::font_cache::FontCache;

/// Resolves and caches decoded `skia_safe::Image` bitmaps across frames.
pub struct ImageCache {
    bundle_dir: PathBuf,
    offline: bool,
    images: HashMap<String, Option<Image>>,
    dims: HashMap<String, Option<(u32, u32)>>,
}

impl ImageCache {
    pub fn new(bundle_dir: &Path) -> Self {
        Self {
            bundle_dir: bundle_dir.to_path_buf(),
            offline: false,
            images: HashMap::new(),
            dims: HashMap::new(),
        }
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    pub fn resolve_asset_path(&self, raw_src: &str) -> Option<PathBuf> {
        resolve_or_fetch_asset(&self.bundle_dir, raw_src, self.offline)
    }

    pub fn get_cached(&self, src: &str) -> Option<&Image> {
        self.images.get(src).and_then(|opt| opt.as_ref())
    }

    pub fn get_or_load(&mut self, src: &str) -> Option<&Image> {
        if !self.images.contains_key(src) {
            let loaded = self.resolve_asset_path(src).and_then(|path| {
                let bytes = std::fs::read(&path).ok()?;
                let img = Image::from_encoded(Data::new_copy(&bytes))?;
                let d = img.dimensions();
                self.dims
                    .insert(src.to_string(), Some((d.width as u32, d.height as u32)));
                Some(img)
            });
            self.images.insert(src.to_string(), loaded);
        }
        self.images.get(src).and_then(|opt| opt.as_ref())
    }

    pub fn dimensions(&self, src: &str) -> Option<(u32, u32)> {
        if let Some(d) = self.dims.get(src) {
            return *d;
        }
        let path = self.resolve_asset_path(src)?;
        image::image_dimensions(&path).ok()
    }
}

/// Adapter passing Skia text, image, and video measurement callbacks into `rsmotion-layout`.
pub struct SceneMeasurer<'a> {
    pub fonts: &'a FontCache,
    pub images: &'a ImageCache,
    pub videos: &'a VideoDecoderPool,
}

impl LayoutMeasurer for SceneMeasurer<'_> {
    fn measure_text(
        &self,
        text: &str,
        style: &ComputedStyle,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        self.fonts.measure_text_box(text, style, max_width)
    }

    fn measure_image(&self, src: &str) -> Option<(u32, u32)> {
        self.images.dimensions(src)
    }

    fn measure_video(&self, src: &str) -> Option<(u32, u32)> {
        self.videos.cached_dimensions(src)
    }
}
