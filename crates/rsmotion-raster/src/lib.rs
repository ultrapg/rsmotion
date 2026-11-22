pub mod canvas2d;
pub mod diff;
mod effects;
pub mod error;
pub mod font_cache;
pub mod gpu_egl;
pub mod image_cache;
pub mod lottie_cache;
pub mod scene_render;
pub mod spike;

pub use diff::{compare_png_files, compare_rgba, write_heatmap_png, DiffReport};
pub use error::{RasterError, Result};
pub use font_cache::FontCache;
pub use gpu_egl::{GpuInfo, HeadlessGpuSurface};
pub use image_cache::{ImageCache, SceneMeasurer};
pub use lottie_cache::LottieCache;
pub use scene_render::{RenderedFrame, RenderedNv12Frame, SceneRenderer};
pub use spike::{run_raster_spike, BackendSpikeMetrics, RasterSpikeReport, SpikeConfig};
