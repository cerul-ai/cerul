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
                .is_some_and(|s| s.starts_with("Sending model requests")))
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
fn clean_dry_run_and_explicit_execution_keep_authoritative_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join(".cerul/cache");
    let sidecar = dir.path().join(".cerul/sidecars/keep");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::create_dir_all(&sidecar).unwrap();
    std::fs::write(cache.join("proxy.mp4"), b"cache").unwrap();
    std::fs::write(sidecar.join("transcript.jsonl"), b"annotation").unwrap();
    let output = cli(dir.path(), &["--json", "clean", "--cache", "--dry-run"]);
    assert!(output.status.success());
    assert_eq!(final_json(&output)["items"][0]["action"], "remove_cache");
    assert!(cache.exists());
    let output = cli(dir.path(), &["--json", "clean", "--cache"]);
    assert!(output.status.success());
    assert_eq!(final_json(&output)["dry_run"], false);
    assert!(!cache.exists());
    assert!(sidecar.join("transcript.jsonl").exists());
    let rejected = cli(dir.path(), &["--json", "clean", "--sidecars", "."]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(sidecar.exists());
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
