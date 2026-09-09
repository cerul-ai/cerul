//! Typed model output contracts. Times are microseconds relative to the input window.
use crate::annotations::Record;
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! record {
    ($name:ident { $($field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub start_us: i64,
            pub end_us: i64,
            pub confidence: Option<f64>,
            $(pub $field: $kind,)*
        }
    };
}
record!(Task { text: String });
record!(Subtask {
    text: String,
    index: u64
});
record!(Event { verb: String, objects: Vec<String>, actor: Option<String>, outcome: Option<String> });
record!(Interaction { hand: String, object: String, contact: Option<bool> });
record!(State { object: String, attribute: String, before: Option<String>, after: Option<String> });
record!(Flag {
    kind: String,
    note: String
});
record!(Progress {
    value: f64,
    done: bool
});

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window<T> {
    pub records: Vec<T>,
}

fn strict(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                let keys: Vec<_> = properties.keys().cloned().map(Value::String).collect();
                object.insert("required".into(), Value::Array(keys));
                object.insert("additionalProperties".into(), Value::Bool(false));
            }
            object.remove("$schema");
            for child in object.values_mut() {
                strict(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                strict(child);
            }
        }
        _ => {}
    }
}
pub fn response_schema(item: &str) -> Result<Value> {
    let schema = match item {
        "task" => schemars::schema_for!(Window<Task>),
        "subtask" => schemars::schema_for!(Window<Subtask>),
        "event" => schemars::schema_for!(Window<Event>),
        "interaction" => schemars::schema_for!(Window<Interaction>),
        "state" => schemars::schema_for!(Window<State>),
        "flag" => schemars::schema_for!(Window<Flag>),
        "progress" => schemars::schema_for!(Window<Progress>),
        _ => anyhow::bail!("unknown semantic item"),
    };
    let mut value = serde_json::to_value(schema)?;
    strict(&mut value);
    Ok(value)
}
fn decode<T: for<'de> Deserialize<'de> + Serialize>(value: Value) -> Result<Value> {
    Ok(serde_json::to_value(serde_json::from_value::<Window<T>>(
        value,
    )?)?)
}
/// Deserialization validates field types before station-level temporal validation.
pub fn records(item: &str, value: Value) -> Result<Vec<Record>> {
    let normalized = match item {
        "task" => decode::<Task>(value)?,
        "subtask" => decode::<Subtask>(value)?,
        "event" => decode::<Event>(value)?,
        "interaction" => decode::<Interaction>(value)?,
        "state" => decode::<State>(value)?,
        "flag" => decode::<Flag>(value)?,
        "progress" => decode::<Progress>(value)?,
        _ => anyhow::bail!("unknown semantic item"),
    };
    normalized["records"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mut value = value.clone();
            value
                .as_object_mut()
                .unwrap()
                .insert("id".into(), Value::String(format!("{item}-{index}")));
            Ok(serde_json::from_value(value)?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn semantic_schemas_require_typed_payload_and_reject_unadvertised_fields() {
        let valid = json!({"records":[{"start_us":0,"end_us":1000000,"confidence":null,"verb":"grasp","objects":["cup"],"actor":null,"outcome":null}]});
        assert_eq!(
            records("event", valid.clone()).unwrap()[0].fields["verb"],
            "grasp"
        );
        let mut wrong = valid.clone();
        wrong["records"][0]["objects"] = json!("cup");
        assert!(records("event", wrong).is_err());
        let mut wrong = valid;
        wrong["records"][0]["plan"] = json!("invented");
        assert!(records("event", wrong).is_err());
        for item in crate::annotations::SEMANTIC_ITEMS {
            let schema = response_schema(item).unwrap();
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(schema["required"], json!(["records"]));
        }
    }
}
