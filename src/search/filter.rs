use crate::{annotations::AnnotationFile, episode::TimeRange};
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy)]
pub enum Operator {
    Eq,
    NotEq,
    Greater,
    Less,
    Contains,
}
#[derive(Debug, Clone)]
pub struct Filter {
    pub key: String,
    pub operator: Operator,
    pub value: Value,
}
impl Filter {
    pub fn parse(input: &str) -> Result<Self> {
        let Some((index, _)) = input
            .char_indices()
            .find(|(_, c)| matches!(c, '=' | '!' | '>' | '<' | '~'))
        else {
            bail!("filter requires an operator");
        };
        let (key, tail) = input.split_at(index);
        ensure!(!key.is_empty(), "filter key is empty");
        let (operator, raw) = if let Some(raw) = tail.strip_prefix("!=") {
            (Operator::NotEq, raw)
        } else {
            let operator = match &tail[..1] {
                "=" => Operator::Eq,
                ">" => Operator::Greater,
                "<" => Operator::Less,
                "~" => Operator::Contains,
                _ => bail!("invalid filter operator"),
            };
            (operator, &tail[1..])
        };
        ensure!(!raw.is_empty(), "filter value is empty");
        ensure!(
            matches!(key, "episode" | "stream" | "kind")
                || matches!(key.split('.').collect::<Vec<_>>().as_slice(),["semantic",item,field] if crate::annotations::SEMANTIC_ITEMS.contains(item)&&!field.is_empty()),
            "unknown filter key"
        );
        if let Some((annotation, field)) = key.rsplit_once('.') {
            let fields: &[&str] = match annotation {
                "semantic.task" => &["text"],
                "semantic.subtask" => &["text", "index"],
                "semantic.event" => &["verb", "objects", "actor", "outcome"],
                "semantic.interaction" => &["hand", "object", "contact"],
                "semantic.state" => &["object", "attribute", "before", "after"],
                "semantic.flag" => &["kind", "note"],
                "semantic.progress" => &["value", "done"],
                _ => &[],
            };
            ensure!(fields.contains(&field), "unknown annotation filter field");
        }
        let value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.into()));
        match operator {
            Operator::Greater | Operator::Less => {
                ensure!(value.is_number(), "numeric comparison requires a number")
            }
            Operator::Contains => ensure!(value.is_string(), "substring filter requires text"),
            _ => {}
        }
        ensure!(
            !value.is_array() && !value.is_object() && !value.is_null(),
            "filter value must be a scalar"
        );
        Ok(Self {
            key: key.into(),
            operator,
            value,
        })
    }
    pub fn matches(&self, value: Option<&Value>) -> bool {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return false;
        };
        if let Some(values) = value.as_array() {
            return match self.operator {
                Operator::NotEq => values.iter().all(|v| v != &self.value),
                _ => values.iter().any(|v| self.matches(Some(v))),
            };
        }
        match self.operator {
            Operator::Eq => value == &self.value,
            Operator::NotEq => value != &self.value,
            Operator::Contains => value
                .as_str()
                .zip(self.value.as_str())
                .is_some_and(|(a, b)| a.contains(b)),
            Operator::Greater => value
                .as_f64()
                .zip(self.value.as_f64())
                .is_some_and(|(a, b)| a > b),
            Operator::Less => value
                .as_f64()
                .zip(self.value.as_f64())
                .is_some_and(|(a, b)| a < b),
        }
    }
    pub fn annotation(&self) -> Option<(&str, &str)> {
        self.key
            .rsplit_once('.')
            .filter(|(name, _)| name.starts_with("semantic."))
    }
}

#[derive(Debug, Clone)]
pub struct Match {
    pub range: TimeRange,
    pub records: BTreeSet<(String, String)>,
}

/// Filters on fields of the same annotation must match the same record.
/// Different annotation groups join by interval intersection on one stream.
pub fn intervals(
    files: &[AnnotationFile],
    filters: &[Filter],
    episode: &str,
    stream: &str,
    duration_us: i64,
) -> Result<Vec<Match>> {
    for filter in filters {
        let actual = match filter.key.as_str() {
            "episode" => Some(Value::String(episode.into())),
            "stream" => Some(Value::String(stream.into())),
            _ => None,
        };
        if let Some(actual) = actual
            && !filter.matches(Some(&actual))
        {
            return Ok(Vec::new());
        }
    }
    let mut groups: BTreeMap<&str, Vec<(&Filter, &str)>> = BTreeMap::new();
    for filter in filters {
        if let Some((name, field)) = filter.annotation() {
            groups.entry(name).or_default().push((filter, field));
        }
    }
    let mut matches = vec![Match {
        range: TimeRange::new(0, duration_us)?,
        records: BTreeSet::new(),
    }];
    for (name, conditions) in groups {
        let matching_files: Vec<_> = files
            .iter()
            .filter(|f| {
                f.header.name == name && f.header.episode == episode && f.header.stream == stream
            })
            .collect();
        ensure!(
            !matching_files.is_empty(),
            "annotation {name} has not been generated for {episode}/{stream}"
        );
        let candidates = matching_files
            .iter()
            .flat_map(|file| file.records.iter())
            .filter(|record| {
                conditions
                    .iter()
                    .all(|(filter, field)| filter.matches(record.fields.get(*field)))
            })
            .collect::<Vec<_>>();
        let mut joined = Vec::new();
        for previous in &matches {
            for record in &candidates {
                if let Some(range) = previous.range.intersection(record.range()?) {
                    let mut records = previous.records.clone();
                    records.insert((name.into(), record.id.clone()));
                    joined.push(Match { range, records });
                }
            }
        }
        matches = joined;
    }
    Ok(matches)
}

pub fn sql_predicate(episode: &str, stream: &str, matches: &[Match]) -> String {
    let ranges = matches
        .iter()
        .map(|m| {
            format!(
                "(start_us < {} AND end_us > {})",
                m.range.end_us, m.range.start_us
            )
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    if ranges.is_empty() {
        return "false".into();
    }
    format!(
        "(episode = {} AND stream = {} AND ({ranges}))",
        crate::index::lance::literal(episode),
        crate::index::lance::literal(stream)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{Header, Model, Record};
    fn file(name: &str, records: Vec<Record>) -> AnnotationFile {
        AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: name.into(),
                episode: "dataset/12".into(),
                stream: "front".into(),
                model: Model {
                    kind: "test".into(),
                    name: "test".into(),
                    base_url: None,
                },
                params: Value::Null,
                created: String::new(),
                cerul_version: String::new(),
                input_hash: "fixture".into(),
                record_schema: format!("{name}/1"),
            },
            records,
        }
    }
    fn record(id: &str, start: i64, end: i64, fields: Value) -> Record {
        Record {
            id: id.into(),
            start_us: start,
            end_us: end,
            confidence: None,
            fields: serde_json::from_value(fields).unwrap(),
        }
    }
    #[test]
    fn conditions_cannot_match_different_records_of_one_annotation() {
        let file = file(
            "semantic.event",
            vec![
                record(
                    "one",
                    0,
                    10,
                    serde_json::json!({"verb":"grasp","actor":"left"}),
                ),
                record(
                    "two",
                    0,
                    10,
                    serde_json::json!({"verb":"place","actor":"right"}),
                ),
            ],
        );
        let filters = vec![
            Filter::parse("semantic.event.verb=grasp").unwrap(),
            Filter::parse("semantic.event.actor=right").unwrap(),
        ];
        assert!(
            intervals(&[file], &filters, "dataset/12", "front", 100)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn different_annotations_join_only_overlapping_time_and_missing_is_an_error() {
        let event = file(
            "semantic.event",
            vec![record("one", 12, 15, serde_json::json!({"verb":"grasp"}))],
        );
        let subtask = file(
            "semantic.subtask",
            vec![record(
                "two",
                14,
                20,
                serde_json::json!({"text":"Move cup"}),
            )],
        );
        let filters = vec![
            Filter::parse("semantic.event.verb=grasp").unwrap(),
            Filter::parse("semantic.subtask.text~cup").unwrap(),
        ];
        let joined = intervals(
            &[event.clone(), subtask],
            &filters,
            "dataset/12",
            "front",
            100,
        )
        .unwrap();
        assert_eq!(joined[0].range, TimeRange::new(14, 15).unwrap());
        assert_eq!(joined[0].records.len(), 2);
        assert!(intervals(&[event], &filters, "dataset/12", "front", 100).is_err());
        assert!(Filter::parse("semantic.event.verb!grasp").is_err());
    }
}
