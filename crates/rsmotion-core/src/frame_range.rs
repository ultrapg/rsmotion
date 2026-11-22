use std::str::FromStr;

use crate::error::{Result, RsmotionError};

/// Inclusive frame range `[start, end]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRange {
    pub start: u32,
    pub end: u32,
}

impl FrameRange {
    /// Creates a validated inclusive range `[start, end]`.
    pub fn new(start: u32, end: u32) -> Result<Self> {
        if start > end {
            return Err(RsmotionError::InvalidFrameRange(format!(
                "{start}-{end} (start > end)"
            )));
        }
        Ok(Self { start, end })
    }

    /// Full range `0..=(duration_in_frames - 1)`.
    pub fn full(duration_in_frames: u32) -> Result<Self> {
        if duration_in_frames == 0 {
            return Err(RsmotionError::InvalidFrameRange(
                "duration_in_frames must be > 0".to_string(),
            ));
        }
        Ok(Self {
            start: 0,
            end: duration_in_frames - 1,
        })
    }

    /// Validates bounds against `duration_in_frames` and returns stepped frame indices.
    pub fn resolve_frames(self, duration_in_frames: u32, every_nth_frame: u32) -> Result<Vec<u32>> {
        if every_nth_frame == 0 {
            return Err(RsmotionError::InvalidEveryNthFrame(every_nth_frame));
        }
        if self.end >= duration_in_frames {
            return Err(RsmotionError::FrameOutOfBounds {
                frame: self.end,
                duration_in_frames,
            });
        }

        let mut frames =
            Vec::with_capacity(((self.end - self.start) / every_nth_frame + 1) as usize);
        let mut cur = self.start;
        while cur <= self.end {
            frames.push(cur);
            match cur.checked_add(every_nth_frame) {
                Some(next) => cur = next,
                None => break,
            }
        }
        Ok(frames)
    }

    /// Number of frames covered (without stepping).
    pub fn len(self) -> u32 {
        self.end - self.start + 1
    }

    pub fn is_empty(self) -> bool {
        false
    }
}

impl FromStr for FrameRange {
    type Err = RsmotionError;

    fn from_str(s: &str) -> Result<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(RsmotionError::InvalidFrameRange(s.to_string()));
        }

        if let Some((left, right)) = trimmed.split_once('-') {
            let start: u32 = left
                .trim()
                .parse()
                .map_err(|_| RsmotionError::InvalidFrameRange(s.to_string()))?;
            let end: u32 = right
                .trim()
                .parse()
                .map_err(|_| RsmotionError::InvalidFrameRange(s.to_string()))?;
            Self::new(start, end)
        } else {
            let single: u32 = trimmed
                .parse()
                .map_err(|_| RsmotionError::InvalidFrameRange(s.to_string()))?;
            Self::new(single, single)
        }
    }
}
