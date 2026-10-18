//! Sentence-level units derived from transcript and screen-text records.
//!
//! Utterances are the vocabulary the selection model works with: every moment it
//! proposes is a span of utterance ids, so every boundary it can name is a
//! boundary that exists in the sidecar. Deriving them is deterministic and
//! local; the checkpoint only saves the work on repeated runs.
use crate::{
    annotations::AnnotationFile,
    storage::{self, Checkpoints},
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped when splitting or timing rules change, so stale checkpoints are ignored.
pub const RECIPE: &str = "clip-utterances/2";

/// A speech phrase that ends without sentence punctuation continues into the
/// next phrase when the gap between the two is at most this long. Recognizers
/// group words into short phrases that often end mid-sentence; the sentence,
/// not the phrase, is the unit a clip should start or end on.
pub const MERGE_GAP_US: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Speech,
    Screen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Utterance {
    /// `u00001…` for speech, `s00001…` for screen text.
    pub id: String,
    pub kind: Kind,
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
}

fn terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '。' | '！' | '？')
}
fn closing(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '”' | '’' | ')' | ']' | '」' | '』' | '）' | '】' | '》'
    )
}

/// Split text at sentence punctuation, keeping the punctuation with its
/// sentence. Latin terminators end a sentence only before whitespace or the
/// end of the text, so `3.5` and `v2.0` stay whole; CJK terminators always do.
pub fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut push = |current: &mut String| {
        let piece = current.trim();
        if !piece.is_empty() {
            pieces.push(piece.to_owned());
        }
        current.clear();
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        current.push(c);
        if !terminator(c) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && (terminator(chars[j]) || closing(chars[j])) {
            current.push(chars[j]);
            j += 1;
        }
        let ascii = c.is_ascii();
        let boundary = !ascii || j >= chars.len() || chars[j].is_whitespace();
        if boundary {
            push(&mut current);
        }
        i = j;
    }
    push(&mut current);
    pieces
}

/// Distribute a record's interval over its sentences by character count.
/// Returns half-open interval boundaries, strictly increasing, ending at `end_us`.
pub fn proportional(start_us: i64, end_us: i64, pieces: &[String]) -> Result<Vec<(i64, i64)>> {
    ensure!(end_us > start_us, "empty utterance interval");
    ensure!(!pieces.is_empty(), "no sentences to time");
    let total: i64 = pieces
        .iter()
        .map(|p| p.chars().filter(|c| !c.is_whitespace()).count() as i64)
        .sum::<i64>()
        .max(1);
    let duration = end_us - start_us;
    let mut bounds = Vec::with_capacity(pieces.len());
    let mut consumed = 0i64;
    let mut previous = start_us;
    for (index, piece) in pieces.iter().enumerate() {
        consumed += piece.chars().filter(|c| !c.is_whitespace()).count() as i64;
        let end = if index + 1 == pieces.len() {
            end_us
        } else {
            // i128 keeps hour-long records exact without overflow.
            let end =
                start_us + (i128::from(duration) * i128::from(consumed) / i128::from(total)) as i64;
            end.clamp(previous + 1, end_us)
        };
        ensure!(end > previous, "utterance timing collapsed");
        bounds.push((previous, end));
        previous = end;
    }
    Ok(bounds)
}

/// Whether the text ends a sentence: its last character, ignoring closing
/// quotes and brackets, is a terminator.
fn ends_sentence(text: &str) -> bool {
    text.chars()
        .rev()
        .find(|c| !closing(*c))
        .is_some_and(terminator)
}

/// Join a continued sentence across two phrases: a space between Latin words
/// or digits, nothing between CJK characters.
fn join(left: &str, right: &str) -> String {
    let latin = left
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        || right
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
    if latin {
        format!("{left} {right}")
    } else {
        format!("{left}{right}")
    }
}

fn record_text(record: &crate::annotations::Record) -> Result<&str> {
    record
        .fields
        .get("text")
        .and_then(Value::as_str)
        .context("annotation record has no text")
}

/// Derive ordered utterances from the current transcript and screen-text records.
/// Records whose pieces cannot each receive at least one microsecond fall back
/// to a single utterance for the whole record rather than inventing time. A
/// speech record that ends mid-sentence continues into the next record when
/// the gap is at most [`MERGE_GAP_US`]; the merged utterance keeps the start
/// of the first fragment and the end of the last, including the gap.
pub fn derive(
    transcript: Option<&AnnotationFile>,
    screen: Option<&AnnotationFile>,
) -> Result<Vec<Utterance>> {
    let mut units = Vec::new();
    for (file, kind, prefix) in [(transcript, Kind::Speech, 'u'), (screen, Kind::Screen, 's')] {
        let Some(file) = file else { continue };
        let mut records: Vec<_> = file.records.iter().collect();
        records.sort_by_key(|r| (r.start_us, r.end_us));
        // Timed pieces in order; an unfinished sentence waits in `open` for the
        // next record. Only a record's last piece can be unfinished.
        let mut timed: Vec<(i64, i64, String)> = Vec::new();
        let mut open: Option<(i64, i64, String)> = None;
        for record in records {
            let text = record_text(record)?;
            let mut pieces = sentences(text);
            if pieces.is_empty() {
                continue;
            }
            if record.end_us - record.start_us < pieces.len() as i64 {
                pieces = vec![pieces.join(" ")];
            }
            for (piece, (start_us, end_us)) in
                pieces
                    .iter()
                    .zip(proportional(record.start_us, record.end_us, &pieces)?)
            {
                let piece = match open.take() {
                    Some((open_start, open_end, open_text))
                        if start_us - open_end <= MERGE_GAP_US =>
                    {
                        (open_start, end_us, join(&open_text, piece))
                    }
                    Some(previous) => {
                        timed.push(previous);
                        (start_us, end_us, piece.clone())
                    }
                    None => (start_us, end_us, piece.clone()),
                };
                if kind == Kind::Speech && !ends_sentence(&piece.2) {
                    open = Some(piece);
                } else {
                    timed.push(piece);
                }
            }
        }
        timed.extend(open.take());
        for (index, (start_us, end_us, text)) in timed.into_iter().enumerate() {
            units.push(Utterance {
                id: format!("{prefix}{:05}", index + 1),
                kind,
                start_us,
                end_us,
                text,
            });
        }
    }
    Ok(units)
}

/// Identity of a derivation: the transcript station key (content, model and
/// window parameters) plus the screen-text key when screen text is present.
pub fn key(transcript: Option<&AnnotationFile>, screen: Option<&AnnotationFile>) -> Result<String> {
    storage::cache_key(&(
        RECIPE,
        transcript.map(|f| &f.header.input_hash),
        screen.map(|f| &f.header.input_hash),
    ))
}

/// Reuse a saved derivation when its key matches; derive and save otherwise.
/// `persist = false` (dry runs) derives without writing.
pub fn load_or_derive(
    checkpoints: &Checkpoints,
    key: &str,
    transcript: Option<&AnnotationFile>,
    screen: Option<&AnnotationFile>,
    persist: bool,
) -> Result<Vec<Utterance>> {
    if let Some(saved) = checkpoints.load::<Vec<Utterance>>(key)? {
        return Ok(saved);
    }
    let units = derive(transcript, screen)?;
    if persist {
        checkpoints.save(key, &units)?;
    }
    Ok(units)
}

/// `mm:ss`, or `h:mm:ss` past an hour, for the list the model reads.
pub fn label(us: i64) -> String {
    let seconds = us.max(0) / 1_000_000;
    let (h, m, s) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// One line per utterance: `id [mm:ss] text`.
pub fn listing(units: &[Utterance]) -> String {
    let mut text = String::new();
    for unit in units {
        text.push_str(&unit.id);
        text.push_str(" [");
        text.push_str(&label(unit.start_us));
        text.push_str("] ");
        text.push_str(&unit.text.split_whitespace().collect::<Vec<_>>().join(" "));
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{Header, Model, Record};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn file(name: &str, records: Vec<(i64, i64, &str)>) -> AnnotationFile {
        AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: name.into(),
                episode: "ep/0".into(),
                stream: "primary".into(),
                model: Model {
                    kind: "gemini".into(),
                    name: "m".into(),
                    base_url: None,
                },
                params: json!({}),
                created: String::new(),
                cerul_version: String::new(),
                input_hash: format!("{name}-key"),
                record_schema: format!("{name}/1"),
            },
            records: records
                .into_iter()
                .enumerate()
                .map(|(i, (start_us, end_us, text))| Record {
                    id: i.to_string(),
                    start_us,
                    end_us,
                    confidence: None,
                    fields: BTreeMap::from([("text".into(), json!(text))]),
                })
                .collect(),
        }
    }

    #[test]
    fn splits_latin_and_cjk_sentences_without_breaking_numbers() {
        assert_eq!(
            sentences("We cut the price from 29.5 to 15. Renewals went up! Really? Yes..."),
            vec![
                "We cut the price from 29.5 to 15.",
                "Renewals went up!",
                "Really?",
                "Yes..."
            ]
        );
        assert_eq!(
            sentences("价格从29降到15。续费率上升了！真的吗？是的"),
            vec!["价格从29降到15。", "续费率上升了！", "真的吗？", "是的"]
        );
        assert_eq!(
            sentences("He said \"stop.\" Then left."),
            vec!["He said \"stop.\"", "Then left."]
        );
        assert_eq!(sentences("   "), Vec::<String>::new());
        assert_eq!(
            sentences("no punctuation at all"),
            vec!["no punctuation at all"]
        );
    }

    #[test]
    fn proportional_timing_follows_character_counts_and_stays_half_open() {
        let pieces: Vec<String> = ["aaaa", "bb", "cccc"].map(str::to_owned).into();
        assert_eq!(
            proportional(1_000_000, 2_000_000, &pieces).unwrap(),
            vec![
                (1_000_000, 1_400_000),
                (1_400_000, 1_600_000),
                (1_600_000, 2_000_000)
            ]
        );
        // Whitespace does not count; CJK characters do.
        let cjk: Vec<String> = ["中文 句子。", "第二句"].map(str::to_owned).into();
        assert_eq!(
            proportional(0, 700, &cjk).unwrap(),
            vec![(0, 437), (437, 700)]
        );
        // Tiny intervals never produce an empty piece.
        let many: Vec<String> = ["a", "b", "c"].map(str::to_owned).into();
        let bounds = proportional(10, 13, &many).unwrap();
        assert!(bounds.iter().all(|(s, e)| e > s));
        assert_eq!(bounds.last().unwrap().1, 13);
        assert!(proportional(5, 5, &many).is_err());
    }

    #[test]
    fn unfinished_phrases_continue_into_the_next_record_within_a_short_gap() {
        let transcript = file(
            "transcript",
            vec![
                (0, 4_000_000, "我们收到了信号。父亲宣布进行无线电禁"),
                (4_000_000, 8_000_000, "默和灯火管制。然后"),
                // 1.5 s away: the open fragment stays on its own.
                (9_500_000, 11_000_000, "继续。"),
                (13_000_000, 15_000_000, "Prices rose to 3"),
                (15_000_000, 17_000_000, "percent. Done"),
            ],
        );
        let units = derive(Some(&transcript), None).unwrap();
        assert_eq!(
            units
                .iter()
                .map(|u| (u.id.as_str(), u.start_us, u.end_us, u.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("u00001", 0, 1_777_777, "我们收到了信号。"),
                (
                    "u00002",
                    1_777_777,
                    7_111_111,
                    "父亲宣布进行无线电禁默和灯火管制。"
                ),
                ("u00003", 7_111_111, 8_000_000, "然后"),
                ("u00004", 9_500_000, 11_000_000, "继续。"),
                (
                    "u00005",
                    13_000_000,
                    16_333_333,
                    "Prices rose to 3 percent."
                ),
                ("u00006", 16_333_333, 17_000_000, "Done"),
            ]
        );
        // Screen text never merges across records.
        let screen = file(
            "screen_text",
            vec![(0, 1_000_000, "Slide 1"), (1_000_000, 2_000_000, "Slide 2")],
        );
        let units = derive(None, Some(&screen)).unwrap();
        assert_eq!(units.len(), 2);
        assert!(ends_sentence("Really?!”"));
        assert!(!ends_sentence("v2.0"));
        assert_eq!(join("第五", "日。"), "第五日。");
        assert_eq!(join("大概", "70 天。"), "大概 70 天。");
    }

    #[test]
    fn derivation_numbers_speech_and_screen_units_separately_and_checkpoints() {
        let transcript = file(
            "transcript",
            vec![
                (0, 4_000_000, "First sentence. Second one!"),
                (4_000_000, 6_000_000, "  "),
                (6_000_000, 8_000_000, "Third"),
            ],
        );
        let screen = file("screen_text", vec![(1_000_000, 3_000_000, "ERROR 500")]);
        let units = derive(Some(&transcript), Some(&screen)).unwrap();
        assert_eq!(
            units
                .iter()
                .map(|u| (u.id.as_str(), u.start_us, u.end_us, u.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("u00001", 0, 2_333_333, "First sentence."),
                ("u00002", 2_333_333, 4_000_000, "Second one!"),
                ("u00003", 6_000_000, 8_000_000, "Third"),
                ("s00001", 1_000_000, 3_000_000, "ERROR 500"),
            ]
        );
        assert_eq!(units[0].kind, Kind::Speech);
        assert_eq!(units[3].kind, Kind::Screen);
        assert_eq!(
            listing(&units[..2]),
            "u00001 [00:00] First sentence.\nu00002 [00:02] Second one!\n"
        );
        assert_eq!(label(3_723_000_000), "1:02:03");

        let dir = tempfile::tempdir().unwrap();
        let checkpoints = Checkpoints::new(dir.path());
        let key = key(Some(&transcript), Some(&screen)).unwrap();
        assert_ne!(key, super::key(Some(&transcript), None).unwrap());
        let dry =
            load_or_derive(&checkpoints, &key, Some(&transcript), Some(&screen), false).unwrap();
        assert_eq!(dry, units);
        assert!(
            !dir.path().join("staging").exists(),
            "dry derivation writes nothing"
        );
        let saved =
            load_or_derive(&checkpoints, &key, Some(&transcript), Some(&screen), true).unwrap();
        assert_eq!(saved, units);
        // A later call reads the checkpoint even without the source files.
        assert_eq!(
            load_or_derive(&checkpoints, &key, None, None, true).unwrap(),
            units
        );
    }
}
