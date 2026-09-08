use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A nonempty half-open interval on an episode's integer-microsecond timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TimeRange {
    pub start_us: i64,
    pub end_us: i64,
}

impl TimeRange {
    pub fn new(start_us: i64, end_us: i64) -> Result<Self> {
        ensure!(
            start_us >= 0 && end_us > start_us,
            "invalid episode interval"
        );
        Ok(Self { start_us, end_us })
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        Self::new(
            self.start_us.max(other.start_us),
            self.end_us.min(other.end_us),
        )
        .ok()
    }

    /// Convert a model's clip-relative interval exactly once to episode time.
    pub fn from_clip(clip_start_us: i64, relative: Self) -> Result<Self> {
        ensure!(clip_start_us >= 0, "negative clip origin");
        Self::new(
            clip_start_us
                .checked_add(relative.start_us)
                .ok_or_else(|| anyhow::anyhow!("time overflow"))?,
            clip_start_us
                .checked_add(relative.end_us)
                .ok_or_else(|| anyhow::anyhow!("time overflow"))?,
        )
    }
}

impl<'de> Deserialize<'de> for TimeRange {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            start_us: i64,
            end_us: i64,
        }
        let raw = Raw::deserialize(deserializer)?;
        Self::new(raw.start_us, raw.end_us).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Episode {
    #[serde(rename = "$cerul")]
    pub schema: String,
    pub episode_id: String,
    pub dataset_id: String,
    pub local_id: String,
    pub streams: Vec<Stream>,
    pub time: Timeline,
    pub task: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    pub format: String,
    pub root: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Timeline {
    pub reference: String,
    #[serde(default)]
    pub mappings: std::collections::BTreeMap<String, TimeMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimeMapping {
    pub a: f64,
    pub b_us: i64,
    pub status: MappingStatus,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    Calibrated,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stream {
    Video {
        id: String,
        primary: bool,
        path: std::path::PathBuf,
        sha256: String,
        range_us: [i64; 2],
        probe: crate::media::Probe,
    },
    Parquet {
        id: String,
        path: std::path::PathBuf,
        columns: Vec<String>,
        row_range: [u64; 2],
    },
}
impl Stream {
    pub fn id(&self) -> &str {
        match self {
            Self::Video { id, .. } | Self::Parquet { id, .. } => id,
        }
    }
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Video { path, .. } | Self::Parquet { path, .. } => path,
        }
    }
}
impl Episode {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == "episode/1", "unsupported episode schema");
        let safe_id = |id: &str| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        };
        ensure!(
            safe_id(&self.dataset_id) && safe_id(&self.local_id),
            "invalid episode identity"
        );
        ensure!(
            self.episode_id == format!("{}/{}", self.dataset_id, self.local_id),
            "inconsistent episode identity"
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut primary = 0;
        for stream in &self.streams {
            ensure!(
                (safe_id(stream.id())
                    || (!matches!(stream.id(), "." | "..")
                        && !stream.id().is_empty()
                        && stream
                            .id()
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))))
                    && ids.insert(stream.id()),
                "invalid or duplicate stream ID"
            );
            ensure!(
                !stream.path().as_os_str().is_empty()
                    && stream
                        .path()
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "stream path must be relative to source root"
            );
            match stream {
                Stream::Video {
                    id,
                    primary: is_primary,
                    sha256,
                    range_us,
                    probe,
                    ..
                } => {
                    ensure!(
                        sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                        "invalid stream hash"
                    );
                    ensure!(
                        range_us[1] > range_us[0] && range_us[1].checked_sub(range_us[0]).is_some(),
                        "invalid source range"
                    );
                    ensure!(
                        probe.width > 0 && probe.height > 0 && probe.duration_us > 0,
                        "invalid media probe"
                    );
                    if *is_primary {
                        primary += 1;
                        ensure!(
                            *id == self.time.reference,
                            "primary stream must be time reference"
                        );
                    }
                }
                Stream::Parquet { row_range, .. } => {
                    ensure!(row_range[1] > row_range[0], "invalid parquet row range")
                }
            }
        }
        ensure!(
            primary == 1,
            "episode must have exactly one primary video stream"
        );
        for (id, mapping) in &self.time.mappings {
            ensure!(
                id != &self.time.reference
                    && self
                        .streams
                        .iter()
                        .any(|s| matches!(s,Stream::Video{id:stream_id,..} if stream_id==id)),
                "mapping must reference a nonprimary video stream"
            );
            ensure!(
                mapping.a.is_finite() && mapping.a > 0.,
                "invalid time mapping scale"
            );
        }
        Ok(())
    }
    pub fn video(&self, id: &str) -> Result<&Stream> {
        self.streams
            .iter()
            .find(|s| matches!(s,Stream::Video{id:stream_id,..} if stream_id==id))
            .ok_or_else(|| anyhow::anyhow!("unknown video stream {id}"))
    }
    pub fn source_to_episode(&self, id: &str, source_us: i64) -> Result<i64> {
        let Stream::Video { range_us, .. } = self.video(id)? else {
            unreachable!()
        };
        let local = source_us
            .checked_sub(range_us[0])
            .ok_or_else(|| anyhow::anyhow!("time overflow"))?;
        if id == self.time.reference {
            return Ok(local);
        }
        let mapping = self
            .time
            .mappings
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("stream has no time mapping"))?;
        ensure!(
            mapping.status != MappingStatus::Unknown,
            "stream time alignment is unknown"
        );
        let mapped = (local as f64 * mapping.a + mapping.b_us as f64).round();
        ensure!(
            mapped.is_finite() && mapped >= i64::MIN as f64 && mapped < i64::MAX as f64,
            "time mapping overflow"
        );
        Ok(mapped as i64)
    }
    pub fn episode_to_source(&self, id: &str, episode_us: i64) -> Result<i64> {
        let Stream::Video { range_us, .. } = self.video(id)? else {
            unreachable!()
        };
        let local = if id == self.time.reference {
            episode_us
        } else {
            let mapping = self
                .time
                .mappings
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("stream has no time mapping"))?;
            ensure!(
                mapping.status != MappingStatus::Unknown && mapping.a.is_finite() && mapping.a > 0.,
                "stream time alignment is unknown"
            );
            let mapped = ((episode_us as f64 - mapping.b_us as f64) / mapping.a).round();
            ensure!(
                mapped.is_finite() && mapped >= i64::MIN as f64 && mapped < i64::MAX as f64,
                "time mapping overflow"
            );
            mapped as i64
        };
        local
            .checked_add(range_us[0])
            .ok_or_else(|| anyhow::anyhow!("time overflow"))
    }
    pub fn duration_us(&self) -> Result<i64> {
        let Stream::Video { range_us, .. } = self.video(&self.time.reference)? else {
            unreachable!()
        };
        range_us[1]
            .checked_sub(range_us[0])
            .ok_or_else(|| anyhow::anyhow!("duration overflow"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_persisted_interval_is_rejected() {
        assert!(serde_json::from_str::<TimeRange>(r#"{"start_us":10,"end_us":5}"#).is_err());
    }
    #[test]
    fn short_event_selects_overlapping_chunk_but_not_adjacent_chunk() {
        let event = TimeRange::new(12_000_000, 13_000_000).unwrap();
        assert_eq!(
            TimeRange::new(0, 30_000_000).unwrap().intersection(event),
            Some(event)
        );
        assert_eq!(
            TimeRange::new(13_000_000, 30_000_000)
                .unwrap()
                .intersection(event),
            None
        );
    }
    #[test]
    fn clip_time_does_not_include_source_file_origin() {
        let relative = TimeRange::new(5_000_000, 6_000_000).unwrap();
        // An episode may start at source PTS 120s, but its first clip starts at episode 0.
        assert_eq!(TimeRange::from_clip(0, relative).unwrap(), relative);
        assert!(TimeRange::from_clip(i64::MAX, relative).is_err());
    }
}
