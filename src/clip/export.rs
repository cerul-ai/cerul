//! Rendering selected proposals: filter graph, burned-in strips, loudness,
//! atomic publication, and the provenance sidecar and SRT beside each clip.
use super::{
    Aspect,
    captions::{self, Cue},
    propose::Proposal,
    raster::{self, Fonts},
    utterances::Utterance,
};
use crate::{
    annotations::Model,
    episode::{Episode, Stream},
    media::{
        self,
        extract::{Graph, SourceRange},
    },
    storage,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LOUDNORM: &str = "loudnorm=I=-16:TP=-1.5:LRA=11";
pub const FADE_US: i64 = 100_000;
pub const PORTRAIT: (u32, u32) = (1080, 1920);

/// The public contract for anything that later reads a Cerul clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Sidecar {
    #[serde(rename = "$cerul")]
    pub schema: String,
    pub cerul_version: String,
    pub created: String,
    pub source: Source,
    pub clip: Range,
    pub utterances: Vec<String>,
    pub quote: String,
    pub title: String,
    pub reason: String,
    pub angle: String,
    pub query: Option<String>,
    pub aspect: Aspect,
    pub captions: Captions,
    pub models: Models,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    /// File name of the source video, not its full path.
    pub path: String,
    pub sha256: String,
    pub episode: String,
    pub stream: String,
    pub duration_us: i64,
}
/// Episode microseconds, half-open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Range {
    pub start_us: i64,
    pub end_us: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Captions {
    pub burned: bool,
    /// Cue times are relative to the clip start.
    pub cues: Vec<Cue>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Models {
    pub transcript: Model,
    pub selection: Model,
}

/// One strip to composite: the input index it will have, its top edge, and
/// the clip-relative window it is visible in (`None` for the whole clip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub input: usize,
    pub y: u32,
    pub window: Option<(i64, i64)>,
}

/// Frame size after the aspect step; source frames are made even for yuv420p.
pub fn output_size(aspect: Aspect, width: u32, height: u32) -> (u32, u32) {
    match aspect {
        Aspect::Source => (width / 2 * 2, height / 2 * 2),
        Aspect::Portrait => PORTRAIT,
    }
}

/// The complete `-filter_complex` string: aspect, overlays with timeline
/// enables, pixel format, and the audio chain when the source has audio.
pub fn filter_graph(aspect: Aspect, overlays: &[Overlay], duration_us: i64, audio: bool) -> String {
    let mut chain = match aspect {
        Aspect::Source => "[0:v]scale=trunc(iw/2)*2:trunc(ih/2)*2,setsar=1[v0]".to_owned(),
        Aspect::Portrait => format!(
            "[0:v]crop=min(iw\\,ih*9/16):ih,scale={}:{},setsar=1[v0]",
            PORTRAIT.0, PORTRAIT.1
        ),
    };
    let mut label = 0;
    for overlay in overlays {
        let enable = match overlay.window {
            Some((start, end)) => format!(
                ":enable='between(t,{},{})'",
                media::seconds(start),
                media::seconds(end)
            ),
            None => String::new(),
        };
        chain.push_str(&format!(
            ";[v{label}][{}:v]overlay=x=0:y={}:format=auto{enable}[v{}]",
            overlay.input,
            overlay.y,
            label + 1
        ));
        label += 1;
    }
    chain.push_str(&format!(";[v{label}]format=yuv420p[vout]"));
    if audio {
        let fade = media::seconds(FADE_US);
        let out_start = media::seconds((duration_us - FADE_US).max(0));
        chain.push_str(&format!(
            ";[0:a]afade=t=in:st=0:d={fade},afade=t=out:st={out_start}:d={fade},{LOUDNORM},aresample=48000[aout]"
        ));
    }
    chain
}

/// `30m30s500` style, sortable within an episode and safe in file names.
pub fn time_name(us: i64) -> String {
    let ms = us.max(0) / 1_000;
    let (h, m, s, frac) = (
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000,
    );
    if h > 0 {
        format!("{h}h{m:02}m{s:02}s{frac:03}")
    } else {
        format!("{m:02}m{s:02}s{frac:03}")
    }
}

/// Base name of a clip's three files.
pub fn base_name(source: &Path, start_us: i64, end_us: i64) -> String {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    format!("{stem}-{}-{}", time_name(start_us), time_name(end_us))
}

/// The credit line under a clip unless `--credit-text` replaces it.
pub fn credit_line(source: &Path, start_us: i64, end_us: i64) -> String {
    format!(
        "{} · {}–{}",
        source
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        super::utterances::label(start_us),
        super::utterances::label(end_us)
    )
}

pub struct Job<'a> {
    pub episode: &'a Episode,
    pub stream: &'a str,
    pub source: &'a Path,
    pub proposal: &'a Proposal,
    pub units: &'a [Utterance],
    pub out: &'a Path,
    pub aspect: Aspect,
    pub captions: bool,
    pub credit: bool,
    pub credit_text: Option<&'a str>,
    pub transcript_model: Model,
    pub selection_model: Model,
}

/// The three files one clip publishes, in publication order.
#[derive(Debug, Clone)]
pub struct Published {
    pub video: PathBuf,
    pub subtitles: PathBuf,
    pub sidecar: PathBuf,
}

/// Render one clip. The MP4 is published first through a temporary file, then
/// the SRT, then the JSON sidecar, so a sidecar's presence means the clip is
/// complete.
pub fn render(job: &Job<'_>, fonts: &mut Fonts) -> Result<Published> {
    let proposal = job.proposal;
    let Stream::Video { probe, sha256, .. } = job.episode.video(job.stream)? else {
        unreachable!()
    };
    ensure!(
        proposal.end_us > proposal.start_us,
        "proposal has an empty range"
    );
    let duration_us = proposal.end_us - proposal.start_us;
    let (width, height) = output_size(job.aspect, probe.width, probe.height);
    ensure!(
        width >= 64 && height >= 64,
        "source frame is too small to clip"
    );
    let cues = captions::cues(
        job.units,
        &proposal.utterances,
        proposal.start_us,
        proposal.end_us,
    )?;
    let temporary = tempfile::tempdir()?;
    let mut inputs = Vec::new();
    let mut overlays = Vec::new();
    let margin = (height as f32 * 0.04) as u32;
    let caption_px = raster::caption_px(width, height);
    let mut caption_bottom = height - margin;
    if job.credit {
        let text = job
            .credit_text
            .map(str::to_owned)
            .unwrap_or_else(|| credit_line(job.source, proposal.start_us, proposal.end_us));
        fonts.prepare(&text);
        let strip = raster::strip(fonts, &[text], width, caption_px * 0.55)?;
        let path = temporary.path().join("credit.png");
        std::fs::write(&path, &strip.png)?;
        overlays.push(Overlay {
            input: inputs.len() + 1,
            y: height - margin - strip.height,
            window: None,
        });
        inputs.push(path);
        caption_bottom = height - margin - strip.height - margin / 2;
    }
    if job.captions {
        for (index, cue) in cues.iter().enumerate() {
            media::check_cancellation()?;
            fonts.prepare(&cue.text);
            let lines: Vec<String> = cue.text.lines().map(str::to_owned).collect();
            let strip = raster::strip(fonts, &lines, width, caption_px)?;
            let path = temporary.path().join(format!("cue-{index:04}.png"));
            std::fs::write(&path, &strip.png)?;
            overlays.push(Overlay {
                input: inputs.len() + 1,
                y: caption_bottom.saturating_sub(strip.height),
                window: Some((cue.start_us, cue.end_us)),
            });
            inputs.push(path);
        }
    }
    let graph = filter_graph(job.aspect, &overlays, duration_us, probe.has_audio);
    let base = base_name(job.source, proposal.start_us, proposal.end_us);
    let video = job.out.join(format!("{base}.mp4"));
    let subtitles = job.out.join(format!("{base}.srt"));
    let sidecar = job.out.join(format!("{base}.json"));
    media::extract::video_rendered(
        job.source,
        SourceRange::new(
            job.episode
                .episode_to_source(job.stream, proposal.start_us)?,
            job.episode.episode_to_source(job.stream, proposal.end_us)?,
        )?,
        &video,
        &Graph {
            inputs: &inputs,
            filter_complex: &graph,
            audio: probe.has_audio,
        },
    )?;
    storage::atomic_write(&subtitles, captions::srt(&cues).as_bytes())?;
    let record = Sidecar {
        schema: "clip/1".into(),
        cerul_version: env!("CARGO_PKG_VERSION").into(),
        created: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        source: Source {
            path: job
                .source
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            sha256: sha256.clone(),
            episode: job.episode.episode_id.clone(),
            stream: job.stream.into(),
            duration_us: job.episode.duration_us()?,
        },
        clip: Range {
            start_us: proposal.start_us,
            end_us: proposal.end_us,
        },
        utterances: proposal.utterances.clone(),
        quote: proposal.quote.clone(),
        title: proposal.title.clone(),
        reason: proposal.reason.clone(),
        angle: proposal.angle.clone(),
        query: proposal.query.clone(),
        aspect: job.aspect,
        captions: Captions {
            burned: job.captions,
            cues,
        },
        models: Models {
            transcript: job.transcript_model.clone(),
            selection: job.selection_model.clone(),
        },
    };
    let mut bytes = serde_json::to_vec_pretty(&record)?;
    bytes.push(b'\n');
    storage::atomic_write(&sidecar, &bytes)?;
    Ok(Published {
        video,
        subtitles,
        sidecar,
    })
}

/// Read a sidecar back and check it is a `clip/1` record with a sane range.
pub fn read_sidecar(path: &Path) -> Result<Sidecar> {
    let record: Sidecar = serde_json::from_slice(&std::fs::read(path)?)
        .with_context(|| format!("invalid clip sidecar {}", path.display()))?;
    ensure!(record.schema == "clip/1", "unsupported clip sidecar schema");
    ensure!(
        record.clip.start_us >= 0
            && record.clip.end_us > record.clip.start_us
            && record.clip.end_us <= record.source.duration_us,
        "clip range is outside its source"
    );
    ensure!(
        record.captions.cues.iter().all(|c| c.start_us >= 0
            && c.end_us > c.start_us
            && c.end_us <= record.clip.end_us - record.clip.start_us),
        "caption cues must lie inside the clip"
    );
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_graph_chains_overlays_with_timeline_enables_and_an_audio_chain() {
        let overlays = vec![
            Overlay {
                input: 1,
                y: 1000,
                window: None,
            },
            Overlay {
                input: 2,
                y: 900,
                window: Some((0, 2_400_000)),
            },
        ];
        let graph = filter_graph(Aspect::Portrait, &overlays, 30_000_000, true);
        assert_eq!(
            graph,
            "[0:v]crop=min(iw\\,ih*9/16):ih,scale=1080:1920,setsar=1[v0];[v0][1:v]overlay=x=0:y=1000:format=auto[v1];[v1][2:v]overlay=x=0:y=900:format=auto:enable='between(t,0.000000,2.400000)'[v2];[v2]format=yuv420p[vout];[0:a]afade=t=in:st=0:d=0.100000,afade=t=out:st=29.900000:d=0.100000,loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000[aout]"
        );
        let silent = filter_graph(Aspect::Source, &[], 5_000_000, false);
        assert_eq!(
            silent,
            "[0:v]scale=trunc(iw/2)*2:trunc(ih/2)*2,setsar=1[v0];[v0]format=yuv420p[vout]"
        );
        assert_eq!(output_size(Aspect::Source, 641, 361), (640, 360));
        assert_eq!(output_size(Aspect::Portrait, 1920, 1080), (1080, 1920));
    }

    #[test]
    fn names_and_credit_lines_are_readable_and_sortable() {
        assert_eq!(time_name(1_830_500_000), "30m30s500");
        assert_eq!(time_name(3_723_000_000), "1h02m03s000");
        assert_eq!(
            base_name(Path::new("/v/ep42.mp4"), 1_830_500_000, 1_872_100_000),
            "ep42-30m30s500-31m12s100"
        );
        assert_eq!(
            credit_line(Path::new("/v/ep42.mp4"), 1_830_500_000, 1_872_100_000),
            "ep42.mp4 · 30:30–31:12"
        );
    }

    fn synthetic(path: &Path, audio: bool) {
        let mut command = crate::media::command("ffmpeg");
        command.args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=10:duration=20",
        ]);
        if audio {
            command.args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=16000:duration=20",
                "-c:a",
                "aac",
            ]);
        }
        command.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-shortest"]);
        media::run(command.arg(path)).unwrap();
    }
    fn unit(id: &str, start_us: i64, end_us: i64, text: &str) -> Utterance {
        Utterance {
            id: id.into(),
            kind: super::super::utterances::Kind::Speech,
            start_us,
            end_us,
            text: text.into(),
        }
    }
    fn proposal(start_us: i64, end_us: i64) -> Proposal {
        Proposal {
            number: 1,
            start_us,
            end_us,
            utterances: vec!["u00001".into(), "u00002".into()],
            quote: "we cut the price from 29 to 15".into(),
            title: "Cutting the price".into(),
            reason: "Concrete numbers.".into(),
            angle: "numbers".into(),
            query: None,
            chapter: None,
            preview: None,
        }
    }
    fn model(name: &str) -> Model {
        Model {
            kind: "gemini".into(),
            name: name.into(),
            base_url: None,
        }
    }

    #[test]
    fn rendered_clip_has_the_requested_length_aspect_overlays_and_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("talk.mp4");
        synthetic(&source, true);
        let episode = crate::index::discover::ordinary_episode(&source).unwrap();
        let units = vec![
            unit(
                "u00001",
                1_500_000,
                4_500_000,
                "we cut the price from 29 to 15",
            ),
            unit(
                "u00002",
                4_500_000,
                8_000_000,
                "and renewals went from 60 to 74 percent",
            ),
        ];
        let proposal = proposal(1_000_000, 9_000_000);
        let mut fonts = Fonts::load().unwrap();
        let out = dir.path().join("clips");
        #[allow(clippy::too_many_arguments)]
        fn job<'a>(
            episode: &'a Episode,
            source: &'a Path,
            proposal: &'a Proposal,
            units: &'a [Utterance],
            out: &'a Path,
            aspect: Aspect,
            captions: bool,
            credit: bool,
        ) -> Job<'a> {
            Job {
                episode,
                stream: "primary",
                source,
                proposal,
                units,
                out,
                aspect,
                captions,
                credit,
                credit_text: None,
                transcript_model: model("asr"),
                selection_model: model("text"),
            }
        }
        let portrait = render(
            &job(
                &episode,
                &source,
                &proposal,
                &units,
                &out,
                Aspect::Portrait,
                true,
                true,
            ),
            &mut fonts,
        )
        .unwrap();
        assert_eq!(
            portrait.video.file_name().unwrap().to_str().unwrap(),
            "talk-00m01s000-00m09s000.mp4"
        );
        let probe = media::probe(&portrait.video).unwrap();
        assert!(
            (7_900_000..=8_150_000).contains(&probe.duration_us),
            "{probe:?}"
        );
        assert_eq!((probe.width, probe.height), PORTRAIT);
        assert!(probe.has_audio);
        let sidecar = read_sidecar(&portrait.sidecar).unwrap();
        assert_eq!(
            sidecar.clip,
            Range {
                start_us: 1_000_000,
                end_us: 9_000_000
            }
        );
        assert_eq!(sidecar.captions.cues.len(), 2);
        assert_eq!(sidecar.captions.cues[0].start_us, 500_000);
        assert!(sidecar.captions.burned);
        assert_eq!(sidecar.models.selection.name, "text");
        assert_eq!(sidecar.source.sha256.len(), 64);
        let srt = std::fs::read_to_string(&portrait.subtitles).unwrap();
        assert!(
            srt.starts_with("1\n00:00:00,500 --> 00:00:03,500\n"),
            "{srt}"
        );
        assert_eq!(
            std::fs::read_dir(&out).unwrap().count(),
            3,
            "no temporary file is left beside the clip"
        );

        // Overlays really change the frame: compare a credit-less render.
        let plain = dir.path().join("plain");
        let bare = render(
            &job(
                &episode,
                &source,
                &proposal,
                &units,
                &plain,
                Aspect::Source,
                false,
                false,
            ),
            &mut fonts,
        )
        .unwrap();
        let bare_probe = media::probe(&bare.video).unwrap();
        assert_eq!((bare_probe.width, bare_probe.height), (320, 240));
        let framed = dir.path().join("framed");
        let with = render(
            &job(
                &episode,
                &source,
                &proposal,
                &units,
                &framed,
                Aspect::Source,
                true,
                true,
            ),
            &mut fonts,
        )
        .unwrap();
        let still = |video: &Path, name: &str| {
            let png = dir.path().join(name);
            media::extract::still(video, 2_000_000, &png, 320).unwrap();
            image::open(png).unwrap().to_rgb8()
        };
        let before = still(&bare.video, "bare.png");
        let after = still(&with.video, "with.png");
        // Lossy encoding moves every pixel a little; count real changes only.
        let changed = |a: &image::Rgb<u8>, b: &image::Rgb<u8>| {
            a.0.iter().zip(b.0).any(|(x, y)| x.abs_diff(y) > 48)
        };
        let differing = before
            .pixels()
            .zip(after.pixels())
            .enumerate()
            .filter(|(index, (a, b))| index / 320 > 150 && changed(a, b))
            .count();
        assert!(
            differing > 500,
            "the caption and credit strips were composited: {differing}"
        );
        let top_same = before
            .pixels()
            .zip(after.pixels())
            .take(320 * 40)
            .filter(|(a, b)| changed(a, b))
            .count();
        assert!(
            top_same < 320 * 40 / 10,
            "the top of the frame is untouched: {top_same}"
        );

        // A silent source renders without an audio chain.
        let silent = dir.path().join("silent.mp4");
        synthetic(&silent, false);
        let episode = crate::index::discover::ordinary_episode(&silent).unwrap();
        let job = Job {
            episode: &episode,
            stream: "primary",
            source: &silent,
            proposal: &proposal,
            units: &units,
            out: &dir.path().join("silent-clips"),
            aspect: Aspect::Source,
            captions: true,
            credit: true,
            credit_text: Some("my credit"),
            transcript_model: model("asr"),
            selection_model: model("text"),
        };
        let rendered = render(&job, &mut fonts).unwrap();
        assert!(!media::probe(&rendered.video).unwrap().has_audio);
    }

    #[test]
    fn sidecar_round_trips_and_rejects_cues_outside_the_clip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.json");
        let record: Sidecar = serde_json::from_str(
            r#"{"$cerul":"clip/1","cerul_version":"0.0.19","created":"2026-10-15T09:12:00Z",
            "source":{"path":"ep42.mp4","sha256":"abc","episode":"abc/0","stream":"primary","duration_us":5400000000},
            "clip":{"start_us":1830500000,"end_us":1872100000},"utterances":["u00137","u00138"],
            "quote":"we cut the price","title":"Cutting the price","reason":"Only pricing numbers.",
            "angle":"numbers","query":null,"aspect":"source",
            "captions":{"burned":true,"cues":[{"start_us":0,"end_us":2400000,"text":"we cut the price"}]},
            "models":{"transcript":{"kind":"gemini","name":"a"},"selection":{"kind":"gemini","name":"b"}}}"#,
        )
        .unwrap();
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        let read = read_sidecar(&path).unwrap();
        assert_eq!(read, record);
        assert_eq!(serde_json::to_value(&read).unwrap()["$cerul"], "clip/1");
        let mut bad = record;
        bad.captions.cues[0].end_us = 60_000_000_000;
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(read_sidecar(&path).is_err());
    }
}
