//! Caption cues from utterance timing, and the SRT written beside each clip.
use super::utterances::{Kind, Utterance, proportional};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_LINE_CHARS: usize = 32;
pub const MAX_LINES: usize = 2;

/// One caption, timed relative to the clip start; lines are joined by `\n`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cue {
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
}

/// Characters that stand alone without spaces: Han, kana, Hangul and CJK punctuation.
fn cjk(c: char) -> bool {
    crate::text::han(c)
        || matches!(c as u32, 0x3000..=0x30ff | 0x31f0..=0x31ff | 0xac00..=0xd7af | 0xff00..=0xffef)
}

/// Wrap text into lines of at most `max` characters. Latin text breaks at
/// spaces; CJK text breaks between characters; an overlong word is split.
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    // (token, preceded by whitespace)
    let mut tokens: Vec<(String, bool)> = Vec::new();
    let mut word = String::new();
    let mut space = false;
    let flush = |word: &mut String, tokens: &mut Vec<(String, bool)>, space: &mut bool| {
        if !word.is_empty() {
            tokens.push((std::mem::take(word), *space));
            *space = false;
        }
    };
    for c in text.chars() {
        if c.is_whitespace() {
            flush(&mut word, &mut tokens, &mut space);
            space = true;
        } else if cjk(c) {
            flush(&mut word, &mut tokens, &mut space);
            tokens.push((c.to_string(), space));
            space = false;
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut tokens, &mut space);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut length = 0usize;
    for (token, spaced) in tokens {
        let pieces: Vec<String> = if token.chars().count() > max {
            token
                .chars()
                .collect::<Vec<_>>()
                .chunks(max)
                .map(|chunk| chunk.iter().collect())
                .collect()
        } else {
            vec![token]
        };
        for (index, piece) in pieces.into_iter().enumerate() {
            let count = piece.chars().count();
            let gap = usize::from(!current.is_empty() && spaced && index == 0);
            if !current.is_empty() && length + gap + count > max {
                lines.push(std::mem::take(&mut current));
                length = 0;
            }
            if !current.is_empty() && spaced && index == 0 {
                current.push(' ');
                length += 1;
            }
            current.push_str(&piece);
            length += count;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Cues for the speech utterances inside `[clip_start_us, clip_end_us)`, at
/// most two lines of 32 characters each, timed proportionally inside their
/// utterance and expressed relative to the clip start.
pub fn cues(units: &[Utterance], clip_start_us: i64, clip_end_us: i64) -> Result<Vec<Cue>> {
    let mut cues = Vec::new();
    for unit in units {
        if unit.kind != Kind::Speech || unit.end_us <= clip_start_us || unit.start_us >= clip_end_us
        {
            continue;
        }
        let lines = wrap(&unit.text, MAX_LINE_CHARS);
        if lines.is_empty() {
            continue;
        }
        let groups: Vec<String> = lines
            .chunks(MAX_LINES)
            .map(|group| group.join("\n"))
            .collect();
        for (text, (start_us, end_us)) in
            groups
                .iter()
                .zip(proportional(unit.start_us, unit.end_us, &groups)?)
        {
            let start = start_us.max(clip_start_us) - clip_start_us;
            let end = end_us.min(clip_end_us) - clip_start_us;
            if end > start {
                cues.push(Cue {
                    start_us: start,
                    end_us: end,
                    text: text.clone(),
                });
            }
        }
    }
    Ok(cues)
}

fn srt_time(us: i64) -> String {
    let ms = us.max(0) / 1_000;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}

/// SubRip text for the cues, numbered from one.
pub fn srt(cues: &[Cue]) -> String {
    let mut text = String::new();
    for (index, cue) in cues.iter().enumerate() {
        text.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            index + 1,
            srt_time(cue.start_us),
            srt_time(cue.end_us),
            cue.text
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_breaks_latin_at_spaces_and_cjk_between_characters() {
        assert_eq!(
            wrap("we cut the price from 29 to 15 and renewals went up", 32),
            vec!["we cut the price from 29 to 15", "and renewals went up"]
        );
        assert_eq!(
            wrap(
                "价格从二十九降到十五，续费率从六十升到七十四，大家都很高兴。",
                10
            ),
            vec![
                "价格从二十九降到十五",
                "，续费率从六十升到七",
                "十四，大家都很高兴。"
            ]
        );
        assert_eq!(
            wrap("supercalifragilistic", 5),
            vec!["super", "calif", "ragil", "istic"]
        );
        assert_eq!(
            wrap("混合 mixed 文本 text", 32),
            vec!["混合 mixed 文本 text"]
        );
        assert!(wrap("   ", 32).is_empty());
    }

    #[test]
    fn cues_group_two_lines_time_them_inside_the_utterance_and_clip_to_the_range() {
        let unit = |start: i64, end: i64, text: &str| Utterance {
            id: "u00001".into(),
            kind: Kind::Speech,
            start_us: start,
            end_us: end,
            text: text.into(),
        };
        let mut screen = unit(0, 1_000_000, "ERROR");
        screen.kind = Kind::Screen;
        let units = vec![
            unit(
                9_000_000,
                13_000_000,
                "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen",
            ),
            unit(13_000_000, 15_000_000, "short"),
            screen,
            unit(30_000_000, 31_000_000, "after the clip"),
        ];
        let cues = cues(&units, 10_000_000, 20_000_000).unwrap();
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].start_us, 0);
        assert_eq!(cues[0].text.lines().count(), 2);
        assert!(cues[0].text.lines().all(|l| l.chars().count() <= 32));
        assert_eq!(cues[1].end_us, 3_000_000);
        assert_eq!(
            cues[2],
            Cue {
                start_us: 3_000_000,
                end_us: 5_000_000,
                text: "short".into()
            }
        );
        assert_eq!(
            srt(&cues[2..]),
            "1\n00:00:03,000 --> 00:00:05,000\nshort\n\n"
        );
        assert_eq!(srt_time(3_723_456_000), "01:02:03,456");
    }
}
