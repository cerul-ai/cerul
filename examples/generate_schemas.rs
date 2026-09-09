use anyhow::Result;
use schemars::{JsonSchema, Schema};
use std::{fs, path::Path};
fn schema<T: JsonSchema>() -> Schema {
    schemars::schema_for!(T)
}
fn main() -> Result<()> {
    let check = std::env::args().any(|argument| argument == "--check");
    let schemas = [
        ("episode", schema::<cerul::episode::Episode>()),
        ("status", schema::<cerul::status::Status>()),
        (
            "annotate-result",
            schema::<cerul::annotate::pipeline::Report>(),
        ),
        (
            "semantic-task",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::Task>>(),
        ),
        (
            "semantic-subtask",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::Subtask>>(),
        ),
        (
            "semantic-event",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::Event>>(),
        ),
        (
            "semantic-interaction",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::Interaction>>(),
        ),
        (
            "semantic-state",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::State>>(),
        ),
        (
            "semantic-flag",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::Flag>>(),
        ),
        (
            "semantic-progress",
            schema::<cerul::annotate::schema::Window<cerul::annotate::schema::Progress>>(),
        ),
        ("search-result", schema::<cerul::search::Report>()),
        ("remove-result", schema::<cerul::clean::Report>()),
        ("indexed-record", schema::<cerul::index::records::Row>()),
        ("index-result", schema::<cerul::index::pipeline::Report>()),
        ("annotation", schema::<cerul::annotations::AnnotationFile>()),
        ("annotation-header", schema::<cerul::annotations::Header>()),
        ("annotation-record", schema::<cerul::annotations::Record>()),
        ("event", schema::<cerul::events::Event>()),
        (
            "embedding-row",
            schema::<cerul::index::vectors::VectorRow>(),
        ),
        ("config", schema::<cerul::config::Config>()),
    ];
    if !check {
        fs::create_dir_all("schemas")?;
    }
    for (name, mut schema) in schemas {
        schema.insert(
            "$id".into(),
            serde_json::Value::String(format!("https://cerul.ai/schemas/{name}/1")),
        );
        let path = Path::new("schemas").join(format!("{name}.json"));
        let expected = format!("{}\n", serde_json::to_string_pretty(&schema)?);
        if check {
            anyhow::ensure!(
                fs::read_to_string(&path)? == expected,
                "stale schema {}",
                path.display()
            );
        } else {
            fs::write(path, expected)?;
        }
    }
    Ok(())
}
