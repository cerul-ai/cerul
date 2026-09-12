//! Parameterized, offline fusion candidates. Production search does not select
//! these recipes until representative, held-out evaluation establishes one.
//! No endpoint, index, process arguments, or printing belongs in this module.
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ALGORITHM: &str = "fusion/two-groups-half-coverage/1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Track {
    Video,
    Speech,
    Screen,
    Description,
    Lexical,
}
impl Track {
    fn base(self) -> bool {
        matches!(self, Self::Video | Self::Speech | Self::Screen)
    }
    fn group(self) -> usize {
        match self {
            Self::Video | Self::Description => 0,
            Self::Speech | Self::Screen | Self::Lexical => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub episode: String,
    pub stream: String,
    pub start_us: i64,
    pub end_us: i64,
    pub track: Track,
    pub raw_score: f64,
    /// Original one-based rank, before gating or temporal alignment.
    pub rank: usize,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub annotation: Option<String>,
}
impl Candidate {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty() && !self.episode.is_empty() && !self.stream.is_empty(),
            "incomplete candidate identity"
        );
        ensure!(
            self.start_us >= 0 && self.end_us > self.start_us,
            "invalid candidate interval"
        );
        ensure!(
            self.raw_score.is_finite() && self.rank > 0,
            "invalid candidate score or rank"
        );
        ensure!(
            if self.track == Track::Lexical {
                matches!(
                    self.annotation.as_deref(),
                    Some("transcript" | "screen_text")
                )
            } else {
                self.annotation.is_none()
            },
            "lexical candidates need an original transcript or screen_text annotation"
        );
        Ok(())
    }
    fn anchor(&self) -> Anchor {
        Anchor(
            self.episode.clone(),
            self.stream.clone(),
            self.start_us,
            self.end_us,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    /// Pre-fusion gate in this track's raw units (cosine or BM25).
    pub raw_min: f64,
    /// Positive affine map, clamped to [0, 1]. Relevance, not probability.
    pub scale: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RankRule {
    pub raw_min: f64,
    /// Fixed across all candidates of this query; absent tracks add zero.
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum Recipe {
    /// The three-track baseline over identical intervals, before temporal merge.
    RawMax { threshold: Option<f64> },
    CalibratedMax {
        channels: BTreeMap<Track, Calibration>,
        agreement_bonus: f64,
        agreement_cap: f64,
    },
    GroupedRrf {
        channels: BTreeMap<Track, RankRule>,
        rank_constant: f64,
    },
}
impl Recipe {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::RawMax { threshold } => ensure!(
                threshold.is_none_or(f64::is_finite),
                "invalid cosine threshold"
            ),
            Self::CalibratedMax {
                channels,
                agreement_bonus,
                agreement_cap,
            } => {
                ensure!(
                    !channels.is_empty(),
                    "calibration needs explicit channel rules"
                );
                ensure!(
                    channels.values().all(|c| c.raw_min.is_finite()
                        && c.scale.is_finite()
                        && c.scale > 0.
                        && c.offset.is_finite()),
                    "invalid calibration or missing raw gate"
                );
                ensure!(
                    [agreement_bonus, agreement_cap]
                        .into_iter()
                        .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
                    "agreement must be bounded in [0, 1]"
                );
            }
            Self::GroupedRrf {
                channels,
                rank_constant,
            } => {
                ensure!(!channels.is_empty(), "RRF needs explicit channel rules");
                ensure!(
                    rank_constant.is_finite() && *rank_constant >= 0.,
                    "invalid rank constant"
                );
                ensure!(
                    channels.values().all(|c| c.raw_min.is_finite()
                        && c.weight.is_finite()
                        && c.weight > 0.
                        && c.weight <= 1.),
                    "invalid RRF gate or weight"
                );
            }
        }
        Ok(())
    }
    fn value(&self, c: &Candidate) -> Result<Option<f64>> {
        Ok(match self {
            Self::RawMax { threshold } => (c.track.base()
                && threshold.is_none_or(|min| c.raw_score >= min))
            .then_some(c.raw_score),
            Self::CalibratedMax { channels, .. } => {
                if let Some(rule) = channels
                    .get(&c.track)
                    .filter(|rule| c.raw_score >= rule.raw_min)
                {
                    let value = c.raw_score * rule.scale + rule.offset;
                    ensure!(value.is_finite(), "calibration overflow");
                    (value > 0.).then_some(value.clamp(0., 1.))
                } else {
                    None
                }
            }
            Self::GroupedRrf {
                channels,
                rank_constant,
            } => channels
                .get(&c.track)
                .filter(|rule| c.raw_score >= rule.raw_min)
                .map(|rule| rule.weight / (rank_constant + c.rank as f64)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Anchor(String, String, i64, i64);
impl Anchor {
    /// A short evidence interval may vote for at most one comparable window.
    /// i128 avoids overflow in half-coverage tests and exact IoU comparisons.
    fn overlap(&self, other: &Self) -> Option<(i128, i128)> {
        if self.0 != other.0 || self.1 != other.1 {
            return None;
        }
        let length = i128::from(self.3 - self.2);
        let other_length = i128::from(other.3 - other.2);
        let overlap = i128::from((self.3.min(other.3) - self.2.max(other.2)).max(0));
        (length <= other_length && overlap * 2 >= length && overlap * 2 >= other_length)
            .then_some((overlap, length + other_length - overlap))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    #[serde(flatten)]
    pub candidate: Candidate,
    /// Calibrated relevance or weighted reciprocal rank; raw_score is retained.
    pub value: f64,
    /// Only group winners contribute. Other admitted evidence remains auditable.
    pub contributes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Moment {
    pub episode: String,
    pub stream: String,
    pub start_us: i64,
    pub end_us: i64,
    pub score: f64,
    pub evidence: Vec<Evidence>,
}

/// Fuse bounded, prefiltered candidates. Missing channels need no special case.
/// No temporal union or boundary refinement is inferred from overlapping rows.
pub fn fuse(candidates: &[Candidate], recipe: &Recipe) -> Result<Vec<Moment>> {
    recipe.validate()?;
    let mut identities = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    let mut admitted = Vec::new();
    for c in candidates {
        c.validate()?;
        ensure!(
            identities.insert((&c.episode, &c.stream, c.track, &c.annotation, &c.id)),
            "duplicate candidate identity"
        );
        ensure!(ranks.insert((c.track, c.rank)), "duplicate rank in a track");
        if let Some(value) = recipe.value(c)? {
            admitted.push(Evidence {
                candidate: c.clone(),
                value,
                contributes: false,
            });
        }
    }
    // Stable anchors are formed from admitted base rows before attaching either
    // derived track. Adding descriptions cannot change another row's anchor.
    let anchors: BTreeSet<_> = admitted
        .iter()
        .filter(|e| e.candidate.track.base())
        .map(|e| e.candidate.anchor())
        .collect();
    let mut groups: BTreeMap<Anchor, Vec<Evidence>> = BTreeMap::new();
    for evidence in admitted {
        let c = &evidence.candidate;
        let own = c.anchor();
        let mut target = own.clone();
        if !c.track.base() {
            let mut best: Option<(i128, i128)> = None;
            for anchor in &anchors {
                if let Some((overlap, union)) = own.overlap(anchor)
                    && best.is_none_or(|(o, u)| overlap * u > o * union)
                {
                    target = anchor.clone();
                    best = Some((overlap, union));
                }
            }
        }
        groups.entry(target).or_default().push(evidence);
    }
    let mut moments = Vec::new();
    for (anchor, mut evidence) in groups {
        evidence.sort_by(|a, b| {
            b.value.total_cmp(&a.value).then_with(|| {
                let a = &a.candidate;
                let b = &b.candidate;
                (a.track, a.rank, a.start_us, a.end_us, &a.id)
                    .cmp(&(b.track, b.rank, b.start_us, b.end_us, &b.id))
            })
        });
        let score = if matches!(recipe, Recipe::RawMax { .. }) {
            evidence[0].contributes = true;
            evidence[0].value
        } else {
            let mut winners = [None, None];
            for (i, e) in evidence.iter().enumerate() {
                winners[e.candidate.track.group()].get_or_insert(i);
            }
            let winners: Vec<_> = winners.into_iter().flatten().collect();
            match recipe {
                Recipe::CalibratedMax {
                    agreement_bonus,
                    agreement_cap,
                    ..
                } => {
                    // Evidence is score-sorted; the lowest winning index is
                    // the strongest group, with deterministic identity ties.
                    let strongest_index = *winners.iter().min().expect("nonempty evidence");
                    let strongest = evidence[strongest_index].value;
                    let weakest = winners
                        .iter()
                        .map(|i| evidence[*i].value)
                        .fold(1., f64::min);
                    let bonus = if winners.len() == 2 {
                        (weakest * agreement_bonus).min(*agreement_cap)
                    } else {
                        0.
                    };
                    let score = (strongest + bonus).min(1.);
                    evidence[strongest_index].contributes = true;
                    if score > strongest {
                        for i in winners {
                            evidence[i].contributes = true;
                        }
                    }
                    score
                }
                Recipe::GroupedRrf { .. } => winners
                    .into_iter()
                    .map(|i| {
                        evidence[i].contributes = true;
                        evidence[i].value
                    })
                    .sum(),
                Recipe::RawMax { .. } => unreachable!(),
            }
        };
        moments.push(Moment {
            episode: anchor.0,
            stream: anchor.1,
            start_us: anchor.2,
            end_us: anchor.3,
            score,
            evidence,
        });
    }
    moments.sort_by(|a, b| {
        b.score.total_cmp(&a.score).then_with(|| {
            (&a.episode, &a.stream, a.start_us, a.end_us)
                .cmp(&(&b.episode, &b.stream, b.start_us, b.end_us))
        })
    });
    Ok(moments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(track: Track, rank: usize, start: i64, end: i64, score: f64) -> Candidate {
        Candidate {
            id: format!("{track:?}-{rank}"),
            episode: "clip".into(),
            stream: "primary".into(),
            start_us: start,
            end_us: end,
            track,
            raw_score: score,
            rank,
            text: None,
            annotation: (track == Track::Lexical).then(|| "transcript".into()),
        }
    }
    fn rrf() -> Recipe {
        Recipe::GroupedRrf {
            channels: [
                Track::Video,
                Track::Description,
                Track::Speech,
                Track::Screen,
                Track::Lexical,
            ]
            .into_iter()
            .map(|t| {
                (
                    t,
                    RankRule {
                        raw_min: 0.5,
                        weight: 1.,
                    },
                )
            })
            .collect(),
            rank_constant: 60.,
        }
    }
    fn calibrated() -> Recipe {
        Recipe::CalibratedMax {
            channels: [
                Track::Video,
                Track::Description,
                Track::Speech,
                Track::Screen,
                Track::Lexical,
            ]
            .into_iter()
            .map(|t| {
                (
                    t,
                    Calibration {
                        raw_min: 0.5,
                        scale: 1.,
                        offset: 0.,
                    },
                )
            })
            .collect(),
            agreement_bonus: 0.2,
            agreement_cap: 0.05,
        }
    }

    #[test]
    fn correlated_tracks_and_duplicate_rows_do_not_multiply_votes() -> Result<()> {
        let mut rows = vec![
            row(Track::Video, 1, 0, 30, 0.8),
            row(Track::Speech, 1, 0, 30, 0.8),
        ];
        let original = fuse(&rows, &rrf())?[0].score;
        rows.extend([
            row(Track::Description, 1, 0, 30, 0.9),
            row(Track::Description, 2, 0, 30, 0.9),
            row(Track::Screen, 1, 0, 30, 0.9),
            row(Track::Lexical, 1, 0, 30, 30.),
        ]);
        let moments = fuse(&rows, &rrf())?;
        assert_eq!(moments.len(), 1);
        assert_eq!(moments[0].score, original);
        assert_eq!(moments[0].evidence.len(), 6);
        assert_eq!(
            moments[0].evidence.iter().filter(|e| e.contributes).count(),
            2
        );
        Ok(())
    }

    #[test]
    fn gates_reject_irrelevant_rank_one_without_renumbering_survivors() -> Result<()> {
        let mut recipe = rrf();
        if let Recipe::GroupedRrf { channels, .. } = &mut recipe {
            channels.get_mut(&Track::Lexical).unwrap().raw_min = 12.;
        }
        let rejected = [
            row(Track::Video, 1, 0, 30, 0.49),
            row(Track::Lexical, 1, 0, 30, 10.),
        ];
        assert!(fuse(&rejected, &recipe)?.is_empty());
        let found = fuse(&[row(Track::Video, 2, 0, 30, 0.8)], &recipe)?;
        assert_eq!(found[0].score, 1. / 62.);
        assert_eq!(found[0].evidence[0].candidate.rank, 2);
        Ok(())
    }

    #[test]
    fn descriptions_attach_once_with_exact_iou_and_keep_original_interval() -> Result<()> {
        let rows = [
            row(Track::Video, 1, 0, 30, 0.8),
            row(Track::Video, 2, 10, 40, 0.8),
            row(Track::Description, 1, 5, 35, 0.9),
            row(Track::Description, 2, 0, 90, 0.9),
            row(Track::Description, 3, 14, 16, 0.9),
        ];
        let result = fuse(&rows, &rrf())?;
        assert_eq!(result.len(), 4);
        let attached = result
            .iter()
            .find(|m| m.start_us == 0 && m.end_us == 30)
            .unwrap();
        assert_eq!(attached.evidence.len(), 2); // IoU tie chooses the earlier anchor.
        let original = &attached
            .evidence
            .iter()
            .find(|e| e.candidate.track == Track::Description)
            .unwrap()
            .candidate;
        assert_eq!((original.start_us, original.end_us), (5, 35));
        assert!(result.iter().any(|m| m.start_us == 0 && m.end_us == 90));
        assert!(result.iter().any(|m| m.start_us == 14 && m.end_us == 16));
        let reversed = fuse(&rows.into_iter().rev().collect::<Vec<_>>(), &rrf())?;
        assert_eq!(
            serde_json::to_value(result)?,
            serde_json::to_value(reversed)?
        );
        Ok(())
    }

    #[test]
    fn streams_and_episodes_never_share_anchors() -> Result<()> {
        let video = row(Track::Video, 1, 0, 30, 0.8);
        let mut description = row(Track::Description, 1, 2, 28, 0.8);
        description.stream = "other".into();
        let mut speech = row(Track::Speech, 1, 0, 30, 0.8);
        speech.episode = "other".into();
        assert_eq!(fuse(&[video, description, speech], &rrf())?.len(), 3);
        Ok(())
    }

    #[test]
    fn single_track_survives_and_absence_never_increases_score() -> Result<()> {
        let video = row(Track::Video, 1, 0, 30, 0.8);
        let mut rows = vec![video.clone(), row(Track::Speech, 1, 0, 30, 0.6)];
        let alone = fuse(&[video], &calibrated())?[0].score;
        let together = fuse(&rows, &calibrated())?[0].score;
        assert_eq!(alone, 0.8);
        assert!((together - 0.85).abs() < 1e-12); // capped 0.05 bonus
        rows[1].raw_score = 0.49;
        assert_eq!(fuse(&rows, &calibrated())?[0].score, alone);
        let lexical = fuse(&[row(Track::Lexical, 1, 4, 7, 8.)], &rrf())?;
        assert_eq!((lexical[0].start_us, lexical[0].end_us), (4, 7));
        Ok(())
    }

    #[test]
    fn baseline_ignores_description_and_lexical_scores() -> Result<()> {
        let result = fuse(
            &[
                row(Track::Video, 1, 0, 30, 0.7),
                row(Track::Speech, 1, 0, 30, 0.8),
                row(Track::Description, 1, 0, 30, 0.99),
                row(Track::Lexical, 1, 0, 30, 100.),
            ],
            &Recipe::RawMax { threshold: None },
        )?;
        assert_eq!(result[0].score, 0.8);
        assert_eq!(result[0].evidence.len(), 2);
        assert_eq!(result[0].evidence[0].candidate.track, Track::Speech);
        let roundtrip: Vec<Moment> = serde_json::from_value(serde_json::to_value(&result)?)?;
        assert_eq!(
            roundtrip[0].evidence[0].candidate,
            result[0].evidence[0].candidate
        );
        Ok(())
    }

    #[test]
    fn calibration_changes_scale_without_claiming_unused_votes() -> Result<()> {
        let mut recipe = calibrated();
        if let Recipe::CalibratedMax {
            channels,
            agreement_bonus,
            ..
        } = &mut recipe
        {
            channels.get_mut(&Track::Description).unwrap().offset = -0.3;
            *agreement_bonus = 0.;
        }
        let rows = [
            row(Track::Video, 1, 0, 30, 0.7),
            row(Track::Description, 1, 0, 30, 0.9),
            row(Track::Speech, 1, 0, 30, 0.6),
        ];
        let result = fuse(&rows, &recipe)?;
        assert_eq!(result[0].score, 0.7);
        let voters: Vec<_> = result[0]
            .evidence
            .iter()
            .filter(|e| e.contributes)
            .map(|e| e.candidate.track)
            .collect();
        assert_eq!(voters, [Track::Video]);
        if let Recipe::CalibratedMax { channels, .. } = &mut recipe {
            channels.get_mut(&Track::Lexical).unwrap().scale = f64::MAX;
        }
        assert!(fuse(&[row(Track::Lexical, 1, 0, 30, 100.)], &recipe).is_err());
        Ok(())
    }

    #[test]
    fn raw_scores_roundtrip_exactly_through_diagnostic_json_values() -> Result<()> {
        let original = row(Track::Video, 1, 0, 30, 0.49306201934814453);
        let value: serde_json::Value = serde_json::from_str(&serde_json::to_string(&original)?)?;
        let decoded: Candidate = serde_json::from_value(value)?;
        assert_eq!(decoded.raw_score.to_bits(), original.raw_score.to_bits());
        Ok(())
    }

    #[test]
    fn invalid_recipes_ranks_and_provenance_fail_before_fusion() {
        let mut bad = row(Track::Video, 1, 0, 30, f64::NAN);
        assert!(fuse(&[bad.clone()], &rrf()).is_err());
        bad.raw_score = 0.8;
        let mut duplicate_rank = bad.clone();
        duplicate_rank.id = "other".into();
        assert!(fuse(&[bad, duplicate_rank], &rrf()).is_err());
        let mut lexical = row(Track::Lexical, 1, 0, 30, 10.);
        lexical.annotation = Some("semantic.scene".into());
        assert!(fuse(&[lexical], &rrf()).is_err());
        assert!(
            serde_json::from_str::<Recipe>(
                r#"{"method":"grouped_rrf","channels":{"video":{"weight":1}},"rank_constant":60}"#
            )
            .is_err()
        );
        let mut recipe = calibrated();
        if let Recipe::CalibratedMax { channels, .. } = &mut recipe {
            channels.get_mut(&Track::Video).unwrap().scale = -1.;
        }
        assert!(recipe.validate().is_err());
    }
}
