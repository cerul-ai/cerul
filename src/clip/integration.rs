//! The whole command against mocked endpoints: transcription, brief,
//! proposals, cached export, and partial failure.
use super::*;
use crate::providers::tests::scripted_server;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn synthetic(path: &Path) {
    crate::media::run(
        crate::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=10:duration=40",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=16000:duration=40",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(path),
    )
    .unwrap();
}

fn gemini(value: Value) -> (u16, Value) {
    (
        200,
        json!({"candidates":[{"content":{"parts":[{"text":value.to_string()}]}}]}),
    )
}

/// Answers by request shape, so the order of calls is checked by the counts.
fn respond(request: &Value, counts: &mut BTreeMap<&'static str, usize>) -> (u16, Value) {
    let parts = request["contents"][0]["parts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(audio) = parts.iter().find_map(|p| p["inlineData"]["data"].as_str()) {
        if audio.len() < 100_000 {
            *counts.entry("probe").or_default() += 1;
            return gemini(json!({"segments":[]}));
        }
        *counts.entry("transcribe").or_default() += 1;
        return gemini(json!({"segments":[
            {"start":1.0,"end":5.0,"text":"We cut the price from 29 to 15. Renewals went from 60 to 74 percent.","lang":"en"},
            {"start":6.0,"end":12.0,"text":"Let me tell you a story about our first customer.","lang":"en"},
            {"start":12.0,"end":18.0,"text":"They paid us in cash on day one and never left.","lang":"en"},
            {"start":20.0,"end":26.0,"text":"The lesson is simple: ship before you are ready.","lang":"en"},
            {"start":27.0,"end":34.0,"text":"Second lesson, price for the value you create, not your costs.","lang":"en"}
        ]}));
    }
    let prompt = parts[0]["text"].as_str().unwrap_or_default().to_owned();
    if prompt.contains("editorial brief") {
        *counts.entry("brief").or_default() += 1;
        return gemini(
            json!({"summary":"A founder recounts pricing changes and early customers.","content_type":"interview",
            "topics":["pricing","customers"],
            "chapters":[{"title":"Pricing","start_id":"u00001","end_id":"u00002"},{"title":"Customers","start_id":"u00003","end_id":"u00006"}],
            "suggested_angles":[{"label":"pricing lessons","query":"advice about pricing"}]}),
        );
    }
    if prompt.contains("Angle: Moments where concrete figures") {
        *counts.entry("numbers").or_default() += 1;
        return gemini(json!({"moments":[
            {"start_id":"u00001","end_id":"u00002","quote":"we cut the price from 29 to 15","reason":"Pricing numbers.","title":"Price cut"},
            {"start_id":"u00001","end_id":"u00002","quote":"we doubled the price","reason":"Made up.","title":"Fake"}]}));
    }
    if prompt.contains("Angle: A told anecdote") {
        *counts.entry("stories").or_default() += 1;
        return gemini(json!({"moments":[
            {"start_id":"u00003","end_id":"u00004","quote":"They paid us in cash on day one","reason":"An anecdote.","title":"First customer"},
            {"start_id":"u00005","end_id":"u00006","quote":"ship before you are ready","reason":"A lesson.","title":"Ship early"}]}));
    }
    *counts.entry("unexpected").or_default() += 1;
    (500, json!({"error":"unexpected request"}))
}

fn config(base: &str) -> Config {
    let mut config = Config::default();
    config.transcription.model = "gemini-3.8-flash".into();
    config.transcription.base_url = base.into();
    config.transcription.enabled = Some(true);
    config.vision.base_url = base.into();
    config
}

async fn with_key<T>(config: &Config, future: impl std::future::Future<Output = T>) -> T {
    let scope = crate::providers::credential_scope(&config.transcription);
    crate::providers::with_credentials(
        BTreeMap::from([(scope, "fixture-key-never-real".to_owned())]),
        future,
    )
    .await
}

#[tokio::test]
async fn propose_then_export_reuses_the_cached_run_and_reports_partial_failure() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("talk.mp4");
    synthetic(&source);
    let workspace = dir.path().join("ws");
    let counts = std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new()));
    let seen = counts.clone();
    // probe + one 40 s window + brief + two angles.
    let (base, server) = scripted_server(5, true, move |request| {
        respond(request, &mut seen.lock().unwrap())
    });
    let config = config(&base);
    let options = Options {
        angles: vec!["numbers".into(), "stories".into()],
        limit: 5,
        min_us: 3_000_000,
        max_us: 20_000_000,
        out: dir.path().join("clips"),
        jobs: 1,
        ..Options::default()
    };
    let mut events = Vec::new();
    let report = with_key(
        &config,
        run(
            &source,
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |event| events.push(event),
        ),
    )
    .await
    .unwrap();
    assert!(!report.cached);
    assert_eq!(
        report
            .proposals
            .iter()
            .map(|p| (
                p.number,
                p.angle.as_str(),
                p.chapter.as_deref(),
                p.start_us,
                p.end_us
            ))
            .collect::<Vec<_>>(),
        vec![
            (1, "numbers", Some("Pricing"), 700_000, 5_300_000),
            (2, "stories", Some("Customers"), 5_700_000, 18_300_000),
            (3, "stories", Some("Customers"), 19_700_000, 34_300_000),
        ]
    );
    assert_eq!(report.dropped.len(), 1);
    assert_eq!(
        report.dropped[0].reason,
        propose::DropReason::QuoteNotVerbatim
    );
    assert_eq!(report.brief.as_ref().unwrap().chapters.len(), 2);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Progress { station, .. } if station == "transcript"))
    );
    assert!(events.iter().any(
        |e| matches!(e, Event::Progress { station, done: 2, total: 2, .. } if station == "propose")
    ));
    let sidecar = &report.sidecar;
    assert!(sidecar.join("transcript.jsonl").is_file());
    assert!(!sidecar.join("embeddings").exists(), "no embeddings");
    assert!(
        !sidecar.join("screen_text.jsonl").exists(),
        "no OCR unless asked"
    );
    assert!(sidecar.join(".internal/clip/last-proposals.json").is_file());
    assert_eq!(server.join().unwrap().len(), 5);
    {
        let counts = counts.lock().unwrap();
        assert_eq!(counts.get("probe"), Some(&1));
        assert_eq!(counts.get("transcribe"), Some(&1));
        assert_eq!(counts.get("brief"), Some(&1));
        assert_eq!(counts.get("numbers"), Some(&1));
        assert_eq!(counts.get("stories"), Some(&1));
        assert_eq!(counts.get("unexpected"), None);
    }

    // The server is gone: the same angles replay from the cache.
    let again = with_key(
        &config,
        run(
            &source,
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        ),
    )
    .await
    .unwrap();
    assert!(again.cached);
    assert_eq!(again.proposals, report.proposals);

    // Export two of them; the second is made to fail by a directory squatting
    // on its file name, which must not stop the first.
    let blocked = options.out.join(format!(
        "{}.mp4",
        export::base_name(&source, 5_700_000, 18_300_000)
    ));
    std::fs::create_dir_all(&blocked).unwrap();
    let export_options = Options {
        export: Some(Selection::Numbers(vec![1, 2])),
        aspect: Aspect::Portrait,
        ..options.clone()
    };
    let mut events = Vec::new();
    let exported = with_key(
        &config,
        run(
            &source,
            &workspace,
            &config,
            &export_options,
            CancellationToken::new(),
            &mut |event| events.push(event),
        ),
    )
    .await
    .unwrap();
    assert!(exported.partial);
    assert!(exported.cached);
    assert_eq!(exported.exports.len(), 2);
    let first = &exported.exports[0];
    let video = first.video.as_ref().unwrap();
    assert!(video.is_file());
    assert!(first.subtitles.as_ref().unwrap().is_file());
    let record = export::read_sidecar(first.sidecar.as_ref().unwrap()).unwrap();
    assert_eq!(record.clip.start_us, 700_000);
    assert_eq!(record.quote, "we cut the price from 29 to 15");
    assert_eq!(record.angle, "numbers");
    assert_eq!(record.aspect, Aspect::Portrait);
    assert_eq!(record.models.transcript.name, "gemini-3.8-flash");
    assert_eq!(record.models.selection.name, config.vision.model);
    assert!(!record.captions.cues.is_empty());
    let probe = crate::media::probe(video).unwrap();
    assert_eq!((probe.width, probe.height), export::PORTRAIT);
    assert!(
        (4_500_000..=4_700_000).contains(&probe.duration_us),
        "{probe:?}"
    );
    assert!(exported.exports[1].video.is_none());
    assert!(exported.exports[1].error.is_some());
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Published { annotation, .. } if annotation == "clip"))
            .count(),
        1
    );

    // Export arguments are checked against the cached run.
    for (bad, message) in [
        (
            Options {
                export: Some(Selection::Numbers(vec![9])),
                ..options.clone()
            },
            "proposal 9 does not exist",
        ),
        (
            Options {
                export: Some(Selection::All),
                angles: vec!["claims".into()],
                ..options.clone()
            },
            "made for angles numbers, stories",
        ),
    ] {
        let error = with_key(
            &config,
            run(
                &source,
                &workspace,
                &config,
                &bad,
                CancellationToken::new(),
                &mut |_| {},
            ),
        )
        .await
        .unwrap_err();
        assert!(error.downcast_ref::<UsageError>().is_some(), "{error:#}");
        assert!(error.to_string().contains(message), "{error:#}");
    }
}
