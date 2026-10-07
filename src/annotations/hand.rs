//! Read-only hand record compatibility. No model weights or inference.
use super::Record;
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const NAME: &str = "grounding.hand";
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hand {
    pub track_id: u64,
    pub handedness: Side,
    /// Classification confidence for the side, not keypoint accuracy.
    pub handedness_score: f32,
    /// Model hand-presence confidence; per-joint visibility is unavailable.
    pub confidence: f32,
    /// MediaPipe order: wrist, thumb, index, middle, ring, little finger.
    /// Display-image normalized XY. Out-of-frame/nonfinite predictions are null.
    pub keypoints: [Option<[f32; 2]>; 21],
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub frame_width: u32,
    pub frame_height: u32,
    /// Empty means no accepted detection in this observed frame.
    pub hands: Vec<Hand>,
}
impl Frame {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frame_width > 0 && self.frame_height > 0 && self.hands.len() <= 2,
            "invalid hand frame"
        );
        let mut ids = BTreeSet::new();
        for hand in &self.hands {
            ensure!(
                hand.track_id > 0 && ids.insert(hand.track_id),
                "invalid hand track ID"
            );
            ensure!(
                [hand.confidence, hand.handedness_score]
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid hand confidence"
            );
            ensure!(
                hand.keypoints
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid hand keypoint"
            );
        }
        Ok(())
    }
    pub fn from_record(record: &Record) -> Result<Self> {
        let frame: Self = serde_json::from_value(serde_json::to_value(&record.fields)?)?;
        frame.validate()?;
        Ok(frame)
    }
}
