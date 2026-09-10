use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn cli(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cerul"))
        .current_dir(directory)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", directory)
        .args(args)
        .output()
        .unwrap()
}
fn final_json(output: &Output) -> Value {
    let text = std::str::from_utf8(&output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    serde_json::from_str(text).unwrap()
}
fn video(directory: &Path) {
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x64:rate=2:duration=2",
            "-c:v",
            "libx264",
        ])
        .arg(directory.join("sample.mp4"))
        .output()
        .unwrap();
    assert!(output.status.success());
}
#[test]
fn json_argument_errors_and_dry_run_are_machine_readable_and_do_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let invalid = cli(
        dir.path(),
        &["--json", "index", "sample.mp4", "--chunk", "bad"],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(final_json(&invalid)["error"]["code"], "invalid_arguments");
    assert!(invalid.stderr.is_empty());
    video(dir.path());
    let dry = cli(dir.path(), &["--json", "index", "sample.mp4", "--dry-run"]);
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stdout)
    );
    assert_eq!(final_json(&dry)["dry_run"], true);
    assert!(!dir.path().join(".cerul").exists());
    assert!(!dir.path().join("sample.mp4.cerul").exists());
    assert!(dry.stderr.is_empty());
}
#[test]
fn offline_index_exits_partial_with_ndjson_events_and_endpoint_notice_once() {
    let dir = tempfile::tempdir().unwrap();
    video(dir.path());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "embedding.base_url=\"http://{}/\"",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let output = cli(
        dir.path(),
        &[
            "--json",
            "index",
            "sample.mp4",
            "--no-audio",
            "--no-ocr",
            "--set",
            &endpoint,
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(final_json(&output)["partial"], true);
    let events: Vec<Value> = std::str::from_utf8(&output.stderr)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["msg"]
                .as_str()
                .is_some_and(|s| s.starts_with("Using Gemini")))
            .count(),
        1
    );
    let status = cli(dir.path(), &["--json", "status"]);
    assert!(status.status.success());
    let value = final_json(&status);
    assert_eq!(value["episodes"][0]["embeddings"][0]["complete"], false);
    assert!(status.stderr.is_empty());
}

#[test]
fn removal_dry_run_and_explicit_execution_keep_authoritative_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join(".cerul/cache");
    let sidecar = dir.path().join(".cerul/sidecars/keep");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::create_dir_all(&sidecar).unwrap();
    std::fs::write(cache.join("proxy.mp4"), b"cache").unwrap();
    std::fs::write(sidecar.join("transcript.jsonl"), b"annotation").unwrap();
    let output = cli(dir.path(), &["--json", "remove", "--cache", "--dry-run"]);
    assert!(output.status.success());
    assert_eq!(final_json(&output)["items"][0]["action"], "remove_cache");
    assert!(cache.exists());
    let output = cli(dir.path(), &["--json", "remove", "--cache"]);
    assert!(output.status.success());
    assert_eq!(final_json(&output)["dry_run"], false);
    assert!(!cache.exists());
    assert!(sidecar.join("transcript.jsonl").exists());
    // Sidecars the registry does not know about are never touched, and asking to
    // forget a path with no registered episodes is a no-op rather than a failure.
    let untouched = cli(dir.path(), &["--json", "remove", ".", "--yes"]);
    assert!(untouched.status.success());
    assert!(sidecar.join("transcript.jsonl").exists());
}

#[test]
fn search_filter_and_text_modes_return_json_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let output = cli(
        dir.path(),
        &["--json", "search", "--filter", "episode=missing"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(final_json(&output)["hits"].as_array().unwrap().is_empty());
    assert!(output.stderr.is_empty());
    let output = cli(dir.path(), &["--json", "search", "--text", "ECONNREFUSED"]);
    assert!(output.status.success());
    assert!(final_json(&output)["hits"].as_array().unwrap().is_empty());
    let rejected = cli(dir.path(), &["--json", "search", "--count", "anything"]);
    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(
        final_json(&rejected)["error"]["code"],
        "invalid_configuration"
    );
    let wrong_space = cli(dir.path(), &["--json", "search", "cup"]);
    assert_eq!(wrong_space.status.code(), Some(3));
    assert_eq!(
        final_json(&wrong_space)["error"]["code"],
        "missing_capability"
    );
}

#[test]
fn annotation_help_teaches_the_workflow_without_loading_configuration() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cerul.toml"), "invalid = [").unwrap();
    for args in [vec!["annotate"], vec!["annotate", "--help"]] {
        let output = cli(dir.path(), &args);
        let text = if args.len() == 1 {
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            String::from_utf8_lossy(&output.stderr)
        } else {
            assert!(output.status.success());
            String::from_utf8_lossy(&output.stdout)
        };
        assert!(text.contains("subtask,event,interaction,state"), "{text}");
        assert!(text.find("Examples:").unwrap() < text.find("Options:").unwrap());
        assert!(text.contains("--semantic --only 0"), "{text}");
        assert!(text.contains("no index step needed"), "{text}");
        assert!(text.contains("semantic.<type>.jsonl"), "{text}");
        assert!(!text.contains("following required arguments"), "{text}");
    }
    let output = cli(dir.path(), &["--json", "annotate"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(final_json(&output)["error"]["code"], "invalid_arguments");
    assert!(output.stderr.is_empty());
    let output = cli(dir.path(), &["annotate", "--semantci"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
    assert!(!dir.path().join(".cerul").exists());
}

#[test]
fn annotate_default_plan_and_m2_rejection_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    video(dir.path());
    let output = cli(
        dir.path(),
        &[
            "--json",
            "annotate",
            "sample.mp4",
            "--semantic",
            "--dry-run",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result = final_json(&output);
    let mut names: Vec<_> = result["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["annotation"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["semantic.flag", "semantic.subtask", "semantic.task"]
    );
    assert!(!dir.path().join(".cerul").exists());
    assert!(!dir.path().join("sample.mp4.cerul").exists());
    let selected = cli(
        dir.path(),
        &[
            "--json",
            "annotate",
            "sample.mp4",
            "--semantic",
            "subtask,event,interaction,state",
            "--dry-run",
        ],
    );
    assert!(selected.status.success());
    let result = final_json(&selected);
    let mut names: Vec<_> = result["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["annotation"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "semantic.event",
            "semantic.interaction",
            "semantic.state",
            "semantic.subtask"
        ]
    );
    assert!(!dir.path().join(".cerul").exists());
    assert!(!dir.path().join("sample.mp4.cerul").exists());
    let output = cli(dir.path(), &["--json", "annotate", "sample.mp4", "--world"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(final_json(&output)["error"]["code"], "missing_capability");
}

#[test]
fn provider_status_dry_run_remains_offline_and_does_not_create_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let output = cli(
        dir.path(),
        &["--json", "--dry-run", "status", "--providers"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = final_json(&output);
    assert!(result["providers"].as_object().unwrap().is_empty());
    for name in ["embedding", "vision", "transcription", "perception"] {
        assert!(result["capabilities"][name].is_null());
    }
    assert!(!dir.path().join(".cerul").exists());
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn ctrl_c_stops_media_subprocess_and_exits_cancelled() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::Stdio,
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    // The stub ffprobe below stands in for real inspection, so the input only
    // has to exist for the CLI to reach it.
    fs::write(dir.path().join("sample.mp4"), b"").unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let marker = dir.path().join("media-started");
    // Replace only this child's ffprobe. exec keeps one direct process for reaping.
    let probe = bin.join("ffprobe");
    fs::write(
        &probe,
        "#!/bin/sh\nprintf '%s' \"$$\" > \"$CERUL_TEST_MARKER\"\nexec /bin/sleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_cerul"))
        .current_dir(dir.path())
        .env_clear()
        .env("HOME", dir.path())
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("CERUL_TEST_MARKER", &marker)
        .args(["--json", "index", "sample.mp4"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !marker.exists() {
        if start.elapsed() > Duration::from_secs(60) || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "media subprocess did not start: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let start = Instant::now();
    assert!(
        Command::new("/bin/kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(3) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("CLI did not promptly cancel media work");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert_eq!(final_json(&output)["error"]["code"], "cancelled");
    let media_pid = fs::read_to_string(marker).unwrap();
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", media_pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success(),
        "media child survived CLI cancellation"
    );
    assert!(!dir.path().join(".cerul").exists());
}

#[test]
fn search_rejects_invalid_kind_and_missing_save_tools_before_workspace_writes() {
    let dir = tempfile::tempdir().unwrap();
    for (args, code) in [
        (vec!["--json", "search", "--filter", "kind=video"], 2),
        (
            vec!["--json", "search", "cup", "--text", "--save", "clips"],
            3,
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cerul"))
            .current_dir(dir.path())
            .env_clear()
            .env("HOME", dir.path())
            .env("PATH", "")
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(final_json(&output).get("error").is_some());
        assert!(!dir.path().join(".cerul").exists());
    }
}

#[test]
fn invalid_query_image_is_rejected_before_workspace_or_provider_setup() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("query.gif");
    std::fs::write(&image, b"GIF89a").unwrap();
    let output = cli(dir.path(), &["--json", "search", "--image", "query.gif"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(final_json(&output).get("error").is_some());
    assert!(!dir.path().join(".cerul").exists());
}

#[test]
fn invalid_ontology_is_a_configuration_error_without_workspace_writes() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("empty.txt", ""),
        ("broken.json", "[\"reach\", "),
        ("object.json", "{}"),
    ] {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    for name in ["empty.txt", "broken.json", "object.json", "missing.txt"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cerul"))
            .current_dir(dir.path())
            .env_clear()
            .env("HOME", dir.path())
            .env("PATH", "")
            .args([
                "--json",
                "annotate",
                "missing.mp4",
                "--semantic",
                "--ontology",
                name,
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(final_json(&output).get("error").is_some());
        assert!(!dir.path().join(".cerul").exists());
    }
}

#[test]
fn offline_commands_ignore_unusable_saved_credentials() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".cerul")).unwrap();
    std::fs::write(dir.path().join(".cerul/credentials.json"), b"invalid json").unwrap();
    let status = cli(dir.path(), &["--json", "status"]);
    assert!(status.status.success());
    video(dir.path());
    let dry = cli(dir.path(), &["--json", "--dry-run", "index", "sample.mp4"]);
    assert!(dry.status.success());
    assert_eq!(final_json(&dry)["dry_run"], true);
}

#[test]
fn missing_key_keeps_local_processing_and_environment_bypasses_corrupt_saved_keys() {
    let dir = tempfile::tempdir().unwrap();
    video(dir.path());
    let output = cli(
        dir.path(),
        &["--json", "index", "sample.mp4", "--no-audio", "--no-ocr"],
    );
    assert_eq!(output.status.code(), Some(6), "{:?}", output);
    std::fs::create_dir_all(dir.path().join(".cerul")).unwrap();
    std::fs::write(dir.path().join(".cerul/credentials.json"), "invalid").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cerul"))
        .current_dir(dir.path())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path())
        .env("GEMINI_API_KEY", "test-key")
        .args([
            "--json",
            "--set",
            "embedding.base_url=\"http://127.0.0.1:9/v1\"",
            "index",
            "sample.mp4",
            "--no-audio",
            "--no-ocr",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6), "{:?}", output);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("credential file"));
}

#[test]
fn human_mode_renders_text_and_json_mode_stays_machine_readable() {
    let dir = tempfile::tempdir().unwrap();
    // A bare invocation is the start page, not a JSON dump.
    let home = cli(dir.path(), &[]);
    assert!(home.status.success());
    let text = String::from_utf8_lossy(&home.stdout);
    assert!(text.starts_with("cerul 0."), "{text}");
    assert!(text.contains("Get started"), "{text}");
    assert!(
        text.contains("cerul annotate ./video.mp4 --semantic"),
        "{text}"
    );
    assert!(text.contains("cerul auth set"), "{text}");
    assert!(text.contains("cerul index ./video.mp4"), "{text}");
    assert!(text.contains("cerul auth set"), "{text}");
    assert!(serde_json::from_str::<Value>(&text).is_err());
    assert!(home.stderr.is_empty());

    let status = cli(dir.path(), &["status"]);
    assert!(status.status.success());
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(text.contains("no videos yet"), "{text}");
    assert!(text.contains("key not set"), "{text}");
    assert!(text.contains("not checked"), "{text}");

    let auth = cli(dir.path(), &["auth"]);
    assert!(auth.status.success());
    assert!(String::from_utf8_lossy(&auth.stdout).contains("no key configured"));
    let auth_json = cli(dir.path(), &["--json", "auth"]);
    let value = final_json(&auth_json);
    assert_eq!(value["saved"], false);
    assert_eq!(value["env"], "GEMINI_API_KEY");
    // Saving a key needs a person at a terminal; scripts get a clear failure.
    let set = cli(dir.path(), &["auth", "set"]);
    assert_eq!(set.status.code(), Some(2));
    assert!(set.stdout.is_empty());
    let text = String::from_utf8_lossy(&set.stderr);
    assert!(text.starts_with('\u{2717}'), "{text}");
    assert!(text.contains("GEMINI_API_KEY"), "{text}");
    assert!(
        text.contains("exit 2 \u{b7} invalid arguments or configuration"),
        "{text}"
    );
    let removed = cli(dir.path(), &["auth", "remove"]);
    assert!(removed.status.success());
    assert!(String::from_utf8_lossy(&removed.stdout).contains("No saved Gemini key"));

    // Failures go to stderr with a hint; stdout stays empty for people too.
    let missing = cli(dir.path(), &["index", "missing.mp4"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    let text = String::from_utf8_lossy(&missing.stderr);
    assert!(text.starts_with('\u{2717}'), "{text}");
    assert!(
        text.contains("no such file or directory: missing.mp4"),
        "{text}"
    );
    assert!(text.contains("cerul status"), "{text}");

    // A refusal for a missing selection names a command that works.
    let empty = cli(dir.path(), &["remove"]);
    assert_eq!(empty.status.code(), Some(2));
    let text = String::from_utf8_lossy(&empty.stderr);
    assert!(text.contains("cerul remove ./video.mp4"), "{text}");

    // Argument errors keep clap's own formatting and exit code.
    let bad = cli(dir.path(), &["search", "--limit", "many"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty());

    let search = cli(dir.path(), &["search", "--text", "nothing"]);
    assert!(search.status.success());
    let text = String::from_utf8_lossy(&search.stdout);
    assert!(text.contains("No matches for \"nothing\""), "{text}");
    assert!(text.contains("cerul status"), "{text}");

    let quiet = cli(dir.path(), &["--quiet", "status"]);
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty());
}

#[test]
fn unquoted_multi_word_query_gets_a_quoting_hint() {
    let dir = tempfile::tempdir().unwrap();
    let output = cli(dir.path(), &["search", "one", "people"]);
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("cerul search \"one people\""), "{text}");
    let json = cli(dir.path(), &["--json", "search", "one", "people"]);
    assert_eq!(final_json(&json)["error"]["code"], "invalid_configuration");
}

#[test]
fn open_needs_a_recent_search_and_reports_what_it_would_play() {
    let dir = tempfile::tempdir().unwrap();
    let cold = cli(dir.path(), &["open"]);
    assert_eq!(cold.status.code(), Some(2));
    let text = String::from_utf8_lossy(&cold.stderr);
    assert!(text.contains("no recent search"), "{text}");

    let media = dir.path().join("demo.mp4");
    std::fs::write(&media, b"").unwrap();
    let cache = dir.path().join(".cerul").join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        cache.join("last-search.json"),
        serde_json::to_vec(&serde_json::json!({
            "query": "a cup",
            "hits": [{"media": media, "start_us": 12_000_000i64, "end_us": 19_000_000i64}]
        }))
        .unwrap(),
    )
    .unwrap();
    let opened = cli(dir.path(), &["--json", "open", "1", "--dry-run"]);
    let value = final_json(&opened);
    assert_eq!(value["opened"], false);
    assert_eq!(value["start_us"], 12_000_000i64);
    let human = cli(dir.path(), &["open", "--dry-run"]);
    assert!(human.status.success());
    assert!(
        String::from_utf8_lossy(&human.stdout).contains("would open demo.mp4 at 00:12"),
        "{}",
        String::from_utf8_lossy(&human.stdout)
    );
    // Numbers outside the last search are refused, not silently clamped.
    let missing = cli(dir.path(), &["open", "3"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("returned 1 result"),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
}

#[test]
fn completions_are_printed_verbatim_and_removal_of_an_unindexed_video_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let script = cli(dir.path(), &["completions", "zsh"]);
    assert!(script.status.success());
    let text = String::from_utf8_lossy(&script.stdout);
    assert!(
        text.starts_with("#compdef cerul"),
        "{}",
        &text[..40.min(text.len())]
    );
    assert!(
        text.contains("index"),
        "the script must cover the subcommands"
    );
    assert!(script.stderr.is_empty());
    for shell in ["bash", "fish", "powershell", "elvish"] {
        assert!(cli(dir.path(), &["completions", shell]).status.success());
    }
    assert_eq!(
        cli(dir.path(), &["completions", "tcsh"]).status.code(),
        Some(2)
    );

    // Removing a video that was never indexed says so instead of failing.
    let media = dir.path().join("demo.mp4");
    std::fs::write(&media, b"").unwrap();
    let quiet = cli(dir.path(), &["remove", media.to_str().unwrap(), "--yes"]);
    assert!(quiet.status.success());
    assert!(
        String::from_utf8_lossy(&quiet.stdout).contains("is not indexed"),
        "{}",
        String::from_utf8_lossy(&quiet.stdout)
    );
    // A path that does not exist is still an error, before anything is planned.
    let missing = cli(dir.path(), &["remove", "absent.mp4", "--yes"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("no such file or directory"),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
}

#[test]
fn the_skill_installs_where_agents_look_and_never_replaces_a_hand_written_file() {
    let dir = tempfile::tempdir().unwrap();
    let printed = cli(dir.path(), &["skill", "--print"]);
    assert!(printed.status.success());
    assert!(printed.stderr.is_empty());
    let text = String::from_utf8(printed.stdout.clone()).unwrap();
    assert!(text.starts_with("---\nname: cerul\n"), "{text}");
    assert!(text.contains("generated-by: cerul"), "{text}");
    // The reference is generated, so every command has to appear in it.
    for command in [
        "cerul index",
        "cerul annotate",
        "cerul search",
        "cerul status",
    ] {
        assert!(
            text.contains(&format!("### {command}")),
            "{command} missing"
        );
    }
    assert!(text.contains("--timeline"), "{text}");

    let installed = cli(dir.path(), &["skill", "--install", "claude"]);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let path = dir.path().join(".claude/skills/cerul/SKILL.md");
    assert_eq!(std::fs::read(&path).unwrap(), printed.stdout);
    // Installing again replaces Cerul's own copy without asking.
    assert!(
        cli(dir.path(), &["skill", "--install", "claude"])
            .status
            .success()
    );

    let elsewhere = dir.path().join("skills");
    assert!(
        cli(
            dir.path(),
            &["skill", "--dir", elsewhere.to_str().unwrap(), "--dry-run"]
        )
        .status
        .success()
    );
    assert!(!elsewhere.exists(), "a dry run writes nothing");

    std::fs::write(&path, "---\nname: cerul\n---\nmine\n").unwrap();
    let refused = cli(dir.path(), &["--json", "skill", "--install", "claude"]);
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(
        final_json(&refused)["error"]["code"],
        "invalid_configuration"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "---\nname: cerul\n---\nmine\n"
    );

    // Editing an installed skill and reinstalling must not lose the edit, even
    // though the file still carries the line that says cerul wrote it.
    let mut edited = String::from_utf8(printed.stdout.clone()).unwrap();
    edited.push_str("\nMy own note.\n");
    std::fs::write(&path, &edited).unwrap();
    let kept = cli(dir.path(), &["skill", "--install", "claude"]);
    assert_eq!(kept.status.code(), Some(2));
    // An edit to a file an older build wrote has to be protected too: an
    // upgrade is exactly when that file is about to be replaced.
    let older = edited.replace("generated-by: cerul 0.", "generated-by: cerul 0.0.1-0.");
    std::fs::write(&path, &older).unwrap();
    let upgrade = cli(dir.path(), &["skill", "--install", "claude"]);
    assert_eq!(upgrade.status.code(), Some(2));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), older);
    std::fs::write(&path, &edited).unwrap();
    assert!(
        String::from_utf8_lossy(&kept.stderr).contains("changed after cerul wrote it"),
        "{}",
        String::from_utf8_lossy(&kept.stderr)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    let forced = cli(dir.path(), &["skill", "--install", "claude", "--force"]);
    assert!(forced.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), printed.stdout);
}

#[test]
fn printing_and_redirecting_the_skill_need_no_home_directory() {
    let dir = tempfile::tempdir().unwrap();
    let homeless = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cerul"))
            .current_dir(dir.path())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .args(args)
            .output()
            .unwrap()
    };
    let printed = homeless(&["skill", "--print"]);
    assert!(
        printed.status.success(),
        "{}",
        String::from_utf8_lossy(&printed.stderr)
    );
    assert!(String::from_utf8_lossy(&printed.stdout).starts_with("---\nname: cerul\n"));
    let elsewhere = dir.path().join("skills");
    let written = homeless(&["skill", "--dir", elsewhere.to_str().unwrap()]);
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    assert_eq!(
        std::fs::read(elsewhere.join("cerul/SKILL.md")).unwrap(),
        printed.stdout
    );
    // Only an agent's own directory needs to know where home is.
    let agent = homeless(&["--json", "skill", "--install", "claude"]);
    assert_eq!(agent.status.code(), Some(2));
    assert!(
        final_json(&agent)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--dir")
    );
    // Two destinations cannot both be honoured, so they cannot both be given.
    let both = homeless(&[
        "skill",
        "--install",
        "codex",
        "--dir",
        elsewhere.to_str().unwrap(),
    ]);
    assert_eq!(both.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&both.stderr).contains("cannot be used with"),
        "{}",
        String::from_utf8_lossy(&both.stderr)
    );
}

#[test]
fn the_timeline_reads_local_files_only_and_rejects_a_type_that_does_not_exist() {
    let dir = tempfile::tempdir().unwrap();
    let empty = cli(dir.path(), &["--json", "status", "--timeline"]);
    assert!(empty.status.success());
    assert_eq!(final_json(&empty)["episodes"], serde_json::json!([]));
    assert!(empty.stderr.is_empty());
    // Reading sidecars must not bring a workspace into existence.
    assert!(!dir.path().join(".cerul").exists());

    let unknown = cli(
        dir.path(),
        &["--json", "status", "--timeline", "--type", "pose"],
    );
    assert_eq!(unknown.status.code(), Some(2));
    let message = final_json(&unknown)["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("pose"), "{message}");
    assert!(message.contains("subtask"), "{message}");

    // A local read and an endpoint probe are different requests.
    let both = cli(
        dir.path(),
        &["--json", "status", "--timeline", "--providers"],
    );
    assert_eq!(both.status.code(), Some(2));
    // --type and --limit belong to the timeline, not to the summary.
    let stray = cli(dir.path(), &["--json", "status", "--type", "event"]);
    assert_eq!(stray.status.code(), Some(2));
    assert_eq!(final_json(&stray)["error"]["code"], "invalid_arguments");
}

#[test]
fn a_partial_annotation_carries_the_command_that_continues_it() {
    let dir = tempfile::tempdir().unwrap();
    video(dir.path());
    let media = dir.path().join("sample.mp4");
    // Port 9 discards connections, so the vision endpoint fails after the local
    // work succeeds: the run is partial, which is what carries a retry.
    let output = Command::new(env!("CARGO_BIN_EXE_cerul"))
        .current_dir(dir.path())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path())
        .env("GEMINI_API_KEY", "test-key")
        .args([
            "--json",
            "annotate",
            media.to_str().unwrap(),
            "--semantic",
            "subtask",
            "--jobs",
            "1",
            "--set",
            "vision.base_url=\"http://127.0.0.1:9\"",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6));
    let value = final_json(&output);
    assert_eq!(value["partial"], true);
    // The result describes its own recovery, in the schema, not beside it.
    let argv: Vec<String> = value["retry"]["argv"]
        .as_array()
        .expect("a partial run offers a retry")
        .iter()
        .map(|argument| argument.as_str().unwrap().to_owned())
        .collect();
    // The program is repeated as it was invoked: a path was used because the
    // binary is not on PATH, and shortening it would break the copied command.
    assert_eq!(argv[0], env!("CARGO_BIN_EXE_cerul"));
    assert_eq!(value["retry"]["reason"], "incomplete");
    // Every choice survives, or running it again would not be the same run.
    for argument in [
        media.to_str().unwrap(),
        "--semantic",
        "subtask",
        "vision.base_url=\"http://127.0.0.1:9\"",
    ] {
        assert!(argv.iter().any(|value| value == argument), "{argv:?}");
    }
    assert!(!argv.iter().any(|value| value == "--recompute"), "{argv:?}");
    // A source that is only a file name cannot say which input it came from,
    // and two directories can hold the same name.
    let source = value["modules"][0]["source"].as_str().unwrap();
    assert!(source.ends_with("/sample.mp4"), "{source}");
    assert!(std::path::Path::new(source).is_absolute(), "{source}");
}
