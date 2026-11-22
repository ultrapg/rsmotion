use serde::{Deserialize, Serialize};

use crate::codec::Codec;

/// Composition configuration matching Remotion's `VideoConfig`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoConfig {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_in_frames: u32,
    #[serde(default)]
    pub props: serde_json::Value,
    #[serde(default)]
    pub default_props: serde_json::Value,
    #[serde(default)]
    pub default_codec: Option<Codec>,
}

impl VideoConfig {
    /// Duration of one frame in seconds.
    pub fn frame_duration_secs(&self) -> f64 {
        1.0 / self.fps
    }

    /// Total duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        f64::from(self.duration_in_frames) / self.fps
    }
}
