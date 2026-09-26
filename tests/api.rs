//! End-to-end tests over the real router, driven in-process through
//! `tower::ServiceExt::oneshot`: no socket, and the whole stack still runs.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use utterance::config::Config;
use utterance::routes;
use utterance::state::AppState;
use utterance::store::Store;
use utterance_analysis::voiceprint::SCHEMA_VERSION;

/// A router over a fresh store in a throwaway directory that cleans itself up.
struct TestApp {
    router: Router,
    dir: std::path::PathBuf,
}

impl TestApp {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "utterance-api-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));

        let cfg = Config {
            bind_addr: "127.0.0.1:0".into(),
            data_dir: dir.clone(),
            // API-only: a static dir's fallback would turn a routing mistake
            // into a 200 with an HTML body.
            static_dir: None,
        };
        Self {
            router: routes::router(AppState::new(cfg, Store::open(&dir).expect("open store"))),
            dir,
        }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        // Best-effort: a failed test should report its own reason.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A spoken-ish vowel at 16 kHz mono, gated into bursts so every part of the
/// voiceprint has something to find.
fn wav_fixture(secs: f32) -> Vec<u8> {
    wav_fixture_at(secs, 16_000, 1)
}

/// The same fixture at an arbitrary rate and channel count, tiled from one
/// second (everything divides a second exactly) so long files are cheap.
fn wav_fixture_at(secs: f32, rate: u32, channels: u16) -> Vec<u8> {
    /// Divides one second a whole number of times, so tiles join without a click.
    const F0: f32 = 125.0;
    let formant = |hz: f32, center: f32, bw: f32| 1.0 / (1.0 + ((hz - center) / bw).powi(2));

    // Band-limited to 8 kHz whatever the rate, as a voice nearly is.
    let ceiling = 8_000.0_f32.min(rate as f32 / 2.0);
    let harmonics: Vec<(f32, f32)> = (1..(ceiling / F0).ceil() as u32)
        .map(|k| k as f32 * F0)
        .map(|hz| {
            (
                hz,
                (F0 / hz) * (formant(hz, 730.0, 90.0) + 0.5 * formant(hz, 1090.0, 110.0)),
            )
        })
        .collect();

    let one_second: Vec<f32> = (0..rate as usize)
        .map(|i| {
            let t = i as f32 / rate as f32;
            // Two 300 ms bursts per second, 200 ms apart.
            if (t / 0.5).fract() >= 0.6 {
                return 0.0;
            }
            harmonics
                .iter()
                .map(|&(hz, gain)| gain * (2.0 * std::f32::consts::PI * hz * t).sin())
                .sum::<f32>()
                * 0.4
        })
        .collect();

    let total = (rate as f32 * secs) as usize;
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
        for i in 0..total {
            let s = (one_second[i % one_second.len()].clamp(-1.0, 1.0) * 32_767.0) as i16;
            for _ in 0..channels {
                w.write_sample(s).unwrap();
            }
        }
        w.finalize().unwrap();
    }
    buf.into_inner()
}

/// Send a request to a JSON endpoint and parse the response. Every endpoint here
/// answers JSON, success or error, so unparseable bytes fail the test.
async fn send(app: &TestApp, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.router.clone().oneshot(req).await.expect("router call");
    let status = res.status();
    let bytes = res
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "response body was not JSON ({e}): {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, body)
}

/// Store a take that defines the speaker. In these tests the fixture *is* the
/// voice; `upload_material` is for the tests that care about the difference.
async fn upload(app: &TestApp, label: &str, wav: Vec<u8>) -> (StatusCode, Value) {
    upload_as(app, label, wav, "calibration").await
}

/// Store a take that is only something to render.
async fn upload_material(app: &TestApp, label: &str, wav: Vec<u8>) -> (StatusCode, Value) {
    upload_as(app, label, wav, "material").await
}

async fn upload_as(app: &TestApp, label: &str, wav: Vec<u8>, role: &str) -> (StatusCode, Value) {
    let req = Request::post(format!("/api/recordings?label={label}&role={role}"))
        .body(Body::from(wav))
        .unwrap();
    send(app, req).await
}

#[tokio::test]
async fn health_check_responds() {
    let app = TestApp::new();
    let res = app
        .router
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn uploading_a_recording_returns_its_voiceprint() {
    let app = TestApp::new();
    let (status, body) = upload(&app, "take-1", wav_fixture(2.0)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["meta"]["label"], "take-1");
    assert_eq!(body["voiceprint"]["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(body["voiceprint"]["frame"]["analysisRateHz"], 16_000);

    // A voice-shaped input must come back with populated series.
    let count = body["voiceprint"]["frame"]["count"].as_u64().unwrap();
    assert!(count > 100, "only {count} frames");
    assert_eq!(
        body["voiceprint"]["pitch"]["hz"].as_array().unwrap().len() as u64,
        count
    );
    assert!(
        body["meta"]["voicedFraction"].as_f64().unwrap() > 0.4,
        "voiced fraction was {}",
        body["meta"]["voicedFraction"]
    );

    // Recording quality reaches the summary; this fixture sits below the rail.
    assert_eq!(body["meta"]["clipped"], false);
    assert!(body["meta"]["peak"].as_f64().unwrap() < 0.99);

    assert!(
        body["meta"]["onsetCount"].as_u64().unwrap() >= 3,
        "expected an onset per burst"
    );
}

#[tokio::test]
async fn a_recording_can_be_listed_fetched_and_deleted() {
    let app = TestApp::new();
    let (_, uploaded) = upload(&app, "take-1", wav_fixture(2.0)).await;
    let id = uploaded["meta"]["id"].as_str().unwrap().to_string();

    let (status, list) = send(
        &app,
        Request::get("/api/recordings").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id.as_str());

    let (status, detail) = send(
        &app,
        Request::get(format!("/api/recordings/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["voiceprint"]["schemaVersion"], SCHEMA_VERSION);

    let audio = app
        .router
        .clone()
        .oneshot(
            Request::get(format!("/api/recordings/{id}/audio"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::OK);
    assert_eq!(audio.headers()["content-type"], "audio/wav");
    assert_eq!(
        &audio.into_body().collect().await.unwrap().to_bytes()[..4],
        b"RIFF"
    );

    let (status, _) = send(
        &app,
        Request::delete(format!("/api/recordings/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, list) = send(
        &app,
        Request::get("/api/recordings").body(Body::empty()).unwrap(),
    )
    .await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn re_uploading_the_same_audio_does_not_duplicate_it() {
    let app = TestApp::new();
    let wav = wav_fixture(1.5);
    let (_, first) = upload(&app, "take-1", wav.clone()).await;
    let (_, second) = upload(&app, "take-1-again", wav).await;

    assert_eq!(first["meta"]["id"], second["meta"]["id"]);
    let (_, list) = send(
        &app,
        Request::get("/api/recordings").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn undecodable_audio_is_rejected_with_a_code() {
    let app = TestApp::new();
    let (status, body) = upload(&app, "junk", b"not a wav file at all".to_vec()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "audio_undecodable");
}

#[tokio::test]
async fn a_too_short_recording_is_rejected_with_a_code() {
    let app = TestApp::new();
    let (status, body) = upload(&app, "blip", wav_fixture(0.1)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "audio_too_short");
}

#[tokio::test]
async fn an_empty_body_is_rejected() {
    let app = TestApp::new();
    let (status, body) = upload(&app, "nothing", Vec::new()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
}

#[tokio::test]
async fn an_unknown_recording_is_a_404() {
    let app = TestApp::new();
    let (status, body) = send(
        &app,
        Request::get("/api/recordings/0123456789abcdef")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn a_traversal_id_is_a_404_not_a_file_read() {
    let app = TestApp::new();
    let (status, _) = send(
        &app,
        Request::get("/api/recordings/..%2f..%2fetc%2fpasswd")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_upload_larger_than_the_default_axum_limit_is_accepted() {
    // Past axum's 2 MB default, which a real half-minute take (48 kHz mono,
    // about 2.9 MB) also exceeds. Stereo only keeps the fixture short.
    let app = TestApp::new();
    let wav = wav_fixture_at(12.0, 48_000, 2);
    assert!(
        wav.len() > 2 * 1024 * 1024,
        "fixture is only {} bytes — under the default limit",
        wav.len()
    );

    let (status, body) = upload(&app, "long-take", wav).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["meta"]["durationS"].as_f64().unwrap() > 11.9);
    // Source geometry is reported as recorded; analysis normalises separately.
    assert_eq!(body["meta"]["sampleRateHz"], 48_000);
    assert_eq!(body["voiceprint"]["frame"]["analysisRateHz"], 16_000);
}

/// A take whose vowel moves — roughly *ah* and *ee* alternating — so the
/// profile has a vowel space with width to it.
fn wav_fixture_moving_vowel(secs: f32) -> Vec<u8> {
    const RATE: u32 = 16_000;
    const F0: f32 = 125.0;
    let formant = |hz: f32, center: f32, bw: f32| 1.0 / (1.0 + ((hz - center) / bw).powi(2));

    let total = (RATE as f32 * secs) as usize;
    // Deterministic, so the fixture is the same every run.
    let mut state: u32 = 0x9E37_79B9;
    let mut hiss = (0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            if (t / 0.5).fract() >= 0.6 {
                return 0.0;
            }
            // The last 80 ms of each burst is a fricative, so the consonant path
            // and the knob that controls it have something to act on.
            if (t / 0.5).fract() >= 0.52 {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let white = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let y = white + 1.2 * hiss.0 - 0.72 * hiss.1;
                hiss.1 = hiss.0;
                hiss.0 = y;
                return y * 0.12;
            }

            // Alternate vowels every half second, so both ends of the space
            // survive the profile's percentile trim.
            //
            // Three formants and a source shallower than 1/k: two formants and
            // a 1/k source yield a four-degree scale whose deepest pair is the
            // fourth and fifth, which spans no lattice. The slope stands in for
            // what puts energy in a real voice's upper partials (jitter,
            // shimmer, glottal noise); tuned until the partials look measured.
            let (f1, f2, f3) = if ((t / 0.5) as u32).is_multiple_of(2) {
                (730.0, 1090.0, 2440.0)
            } else {
                (300.0, 2300.0, 3000.0)
            };
            (1..(8_000.0 / F0).ceil() as u32)
                .map(|k| k as f32 * F0)
                .map(|hz| {
                    let gain = (F0 / hz).sqrt()
                        * (formant(hz, f1, 90.0)
                            + 0.8 * formant(hz, f2, 110.0)
                            + 0.6 * formant(hz, f3, 150.0));
                    gain * (2.0 * std::f32::consts::PI * hz * t).sin()
                })
                .sum::<f32>()
                * 0.4
        })
        .collect();

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
        for s in samples {
            w.write_sample((s.clamp(-1.0, 1.0) * 32_767.0) as i16)
                .unwrap();
        }
        w.finalize().unwrap();
    }
    buf.into_inner()
}

/// Fetch a non-JSON endpoint, returning status, content type and body bytes.
async fn fetch(app: &TestApp, path: &str) -> (StatusCode, String, Vec<u8>) {
    let res = app
        .router
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .expect("router call");
    let status = res.status();
    let content_type = res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, content_type, bytes)
}

#[tokio::test]
async fn rendering_a_take_returns_playable_audio() {
    let app = TestApp::new();
    let (status, body) = upload(&app, "calibration", wav_fixture_moving_vowel(8.0)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, content_type, bytes) = fetch(&app, &format!("/api/recordings/{id}/render")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(content_type, "audio/wav");
    assert_eq!(&bytes[0..4], b"RIFF");

    // Eight seconds at 44.1 kHz, 16-bit mono, is about 700 KB.
    assert!(
        bytes.len() > 400_000,
        "rendered only {} bytes — the score was probably empty",
        bytes.len()
    );
}

#[tokio::test]
async fn a_render_is_the_same_every_time() {
    // Byte-identical, or "the mapping changed" cannot be told from "the
    // renderer wandered".
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(8.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (_, _, first) = fetch(&app, &format!("/api/recordings/{id}/render")).await;
    let (_, _, second) = fetch(&app, &format!("/api/recordings/{id}/render")).await;
    assert_eq!(first, second);
}

#[tokio::test]
async fn the_voice_summary_describes_the_derived_scale() {
    let app = TestApp::new();
    upload(&app, "calibration", wav_fixture_moving_vowel(8.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["calibrationLabel"], "calibration");
    assert!(body["tonicHz"].as_f64().unwrap() > 100.0);

    let degrees = body["degrees"].as_array().unwrap();
    assert!(degrees.len() >= 3, "a scale of {} degrees", degrees.len());
    assert_eq!(degrees.first().unwrap()["cents"].as_f64().unwrap(), 0.0);
    assert_eq!(degrees.last().unwrap()["cents"].as_f64().unwrap(), 1200.0);

    // A harmonic source must put a fifth in the scale, whatever else it finds.
    assert!(
        degrees
            .iter()
            .any(|d| (d["cents"].as_f64().unwrap() - 702.0).abs() < 8.0),
        "no fifth in {degrees:?}"
    );

    // An empty palette renders silence; one entry, a colour that never moves.
    let palette = body["palette"].as_array().unwrap();
    assert!(!palette.is_empty(), "no spectra to synthesise from");
    assert!(
        !palette[0].as_array().unwrap().is_empty(),
        "a spectrum with no partials in it"
    );
    assert!(body["detuneCents"].as_f64().unwrap() >= 0.0);
}

#[tokio::test]
async fn asking_for_a_voice_before_recording_anything_explains_itself() {
    let app = TestApp::new();
    let (status, body) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Every new installation starts here, so the message must say what to do.
    let message = body["error"].as_str().unwrap_or_default().to_string()
        + body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("record"),
        "unhelpful message for an empty store: {body}"
    );
}

#[tokio::test]
async fn refuses_to_calibrate_from_material_that_never_held_a_pitch() {
    // No sustained phonation, no harmonic series worth a scale.
    let app = TestApp::new();
    upload(&app, "too-short", wav_fixture_moving_vowel(1.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn calibration_can_be_pointed_at_a_chosen_take() {
    // The automatic choice is a heuristic; a listener must be able to overrule it.
    let app = TestApp::new();
    let (_, first) = upload(&app, "one", wav_fixture_moving_vowel(8.0)).await;
    let (_, second) = upload(&app, "two", wav_fixture_moving_vowel(9.0)).await;
    let chosen = second["meta"]["id"].as_str().unwrap().to_string();
    let other = first["meta"]["id"].as_str().unwrap().to_string();
    assert_ne!(chosen, other);

    let (status, body) = send(
        &app,
        Request::get(format!("/api/voice?calibration={chosen}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["calibrationId"], chosen);
}

#[tokio::test]
async fn an_unknown_calibration_take_is_refused_rather_than_ignored() {
    // Falling back to the automatic choice would render a scale not asked for.
    let app = TestApp::new();
    upload(&app, "calibration", wav_fixture_moving_vowel(8.0)).await;

    let (status, _) = send(
        &app,
        Request::get("/api/voice?calibration=deadbeefdeadbeef")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_short_lively_take_does_not_block_a_usable_one() {
    // Eligibility before preference: a brief take can look rich and then be
    // refused as too short, while a good one goes unexamined.
    let app = TestApp::new();
    upload(&app, "brief", wav_fixture_moving_vowel(1.5)).await;
    upload(&app, "usable", wav_fixture_moving_vowel(9.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["calibrationLabel"], "usable");
}

#[tokio::test]
async fn the_two_mappings_render_differently() {
    // If they rendered the same bytes, the choice would be doing nothing.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, _, field) = fetch(&app, &format!("/api/recordings/{id}/render")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, notes) =
        fetch(&app, &format!("/api/recordings/{id}/render?mapping=notes")).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(&field[0..4], b"RIFF");
    assert_eq!(&notes[0..4], b"RIFF");
    assert_ne!(field, notes, "both mappings rendered identical audio");
}

/// The density at which the fixture's scale stops spanning a plane: the top of
/// the knob's range. Where a real take goes thin depends on the speaker, so the
/// tests check the scale really collapsed.
const DENSITY_TOO_HIGH: &str = "density=0.5";

#[tokio::test]
async fn a_scale_too_thin_for_a_lattice_says_so_rather_than_rendering_silence() {
    // Without the refusal, a scale that points one way renders consonants over
    // silence and answers a perfectly good 200.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, summary) = send(
        &app,
        Request::get(format!("/api/voice?mapping=tonnetz&{DENSITY_TOO_HIGH}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{summary}");

    // Checked, so this cannot quietly test a scale that was fine all along.
    let interior = summary["degrees"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| {
            let cents = d["cents"].as_f64().unwrap();
            cents > 0.0 && cents < 1200.0
        })
        .count();
    assert!(
        interior < 2,
        "the fixture still spans a plane at this density: {summary}"
    );

    // The studio reads the summary before pointing a player anywhere.
    let refusal = summary["refusal"]
        .as_str()
        .unwrap_or_else(|| panic!("no refusal in {summary}"));
    assert!(
        refusal.contains("density"),
        "the refusal does not name the setting that undoes it: {refusal}"
    );

    let (status, _, _) = fetch(
        &app,
        &format!("/api/recordings/{id}/render?mapping=tonnetz&{DENSITY_TOO_HIGH}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a scale that spans no lattice still rendered"
    );
}

#[tokio::test]
async fn only_the_mapping_that_needs_a_plane_is_refused_for_want_of_one() {
    // Only the lattice refuses; every other mapping plays whatever degrees are
    // left, or the knob would seem to have a broken upper half.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    for mapping in ["field", "notes"] {
        let (status, _, audio) = fetch(
            &app,
            &format!("/api/recordings/{id}/render?mapping={mapping}&{DENSITY_TOO_HIGH}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{mapping} was refused a thin scale");
        assert_eq!(&audio[0..4], b"RIFF");

        let (_, summary) = send(
            &app,
            Request::get(format!("/api/voice?mapping={mapping}&{DENSITY_TOO_HIGH}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert!(
            summary["refusal"].is_null(),
            "{mapping} was reported unplayable: {summary}"
        );
    }
}

#[tokio::test]
async fn a_scale_that_spans_a_plane_is_not_reported_as_a_problem() {
    // A warning shown at the default density would train someone to ignore it.
    let app = TestApp::new();
    upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;

    let (status, summary) = send(
        &app,
        Request::get("/api/voice?mapping=tonnetz")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{summary}");
    assert!(
        summary["refusal"].is_null(),
        "the default scale was called unplayable: {summary}"
    );
}

#[tokio::test]
async fn an_unknown_mapping_is_refused_rather_than_ignored() {
    // Falling back to the default would render something not asked for.
    let app = TestApp::new();
    upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let (_, body) = send(
        &app,
        Request::get("/api/recordings").body(Body::empty()).unwrap(),
    )
    .await;
    let id = body[0]["id"].as_str().unwrap().to_string();

    let (status, _, _) = fetch(
        &app,
        &format!("/api/recordings/{id}/render?mapping=orchestral"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn both_mappings_can_sound_together() {
    // Events over a texture is a third thing neither mapping makes alone.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, _, both) = fetch(
        &app,
        &format!("/api/recordings/{id}/render?mapping=field,notes"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, _, field) = fetch(&app, &format!("/api/recordings/{id}/render?mapping=field")).await;
    let (_, _, notes) = fetch(&app, &format!("/api/recordings/{id}/render?mapping=notes")).await;
    assert_ne!(
        both, field,
        "combining changed nothing against the field alone"
    );
    assert_ne!(
        both, notes,
        "combining changed nothing against the notes alone"
    );
}

#[tokio::test]
async fn every_published_knob_changes_what_is_rendered() {
    // Taken from what the API publishes, so a knob in the table but not wired
    // into the render fails here instead of reaching the UI as a dead slider.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, controls) = send(
        &app,
        Request::get("/api/controls").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{controls}");
    let knobs = controls["knobs"].as_array().unwrap();
    assert!(!knobs.is_empty(), "no knobs published at all");

    // Each knob against every mapping it claims to reach, so a false claim fails.
    let every: Vec<String> = controls["mappings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect();

    for knob in knobs {
        let name = knob["name"].as_str().unwrap();
        let value = a_quarter_from_default(knob);
        let claimed: Vec<String> = knob["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect();
        let against = if claimed.is_empty() { &every } else { &claimed };

        for mapping in against {
            assert!(every.contains(mapping), "{name} claims unknown {mapping}");
            let base = format!("/api/recordings/{id}/render?mapping={mapping}");
            let (_, _, plain) = fetch(&app, &base).await;
            let (status, _, altered) = fetch(&app, &format!("{base}&{name}={value}")).await;
            match status {
                StatusCode::OK => assert_ne!(
                    altered, plain,
                    "{name}={value} changed nothing in {mapping}"
                ),
                // A refusal is a change — accepted only when it explains itself.
                // `every_published_mapping_can_be_rendered` fails a mapping that
                // refuses everything.
                StatusCode::UNPROCESSABLE_ENTITY => {
                    let body: Value = serde_json::from_slice(&altered)
                        .unwrap_or_else(|_| panic!("{name}={value}: refusal is not JSON"));
                    assert_eq!(body["code"], "unplayable", "{body}");
                    assert!(
                        body["message"].as_str().unwrap_or_default().contains(name),
                        "{name}={value} was refused without naming {name}: {body}"
                    );
                }
                other => panic!("{name}={value} in {mapping}: {other}"),
            }
        }
    }
}

#[tokio::test]
async fn every_setting_a_slider_can_reach_either_sounds_or_says_why_not() {
    // A published range promises that *every* position means something, and
    // the ends are where nobody drags by hand. Two acceptable answers per
    // position, and no third: it makes sound, or it refuses and says which
    // setting to move.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (_, controls) = send(
        &app,
        Request::get("/api/controls").body(Body::empty()).unwrap(),
    )
    .await;
    let every: Vec<String> = controls["mappings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect();

    // Tallied, so this cannot pass by refusing everything.
    let mut sounded = 0usize;

    for knob in controls["knobs"].as_array().unwrap() {
        let name = knob["name"].as_str().unwrap();
        let claimed: Vec<String> = knob["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect();
        let against = if claimed.is_empty() { &every } else { &claimed };

        for value in ends_and_middle(knob) {
            for mapping in against {
                // The score rather than the render: the same decision without
                // the seconds of synthesis.
                let (status, view) = send(
                    &app,
                    Request::get(format!(
                        "/api/recordings/{id}/score?mapping={mapping}&{name}={value}"
                    ))
                    .body(Body::empty())
                    .unwrap(),
                )
                .await;
                let at = format!("{name}={value} in {mapping}");

                if status == StatusCode::UNPROCESSABLE_ENTITY {
                    assert_eq!(view["code"], "unplayable", "{at}: {view}");
                    assert!(
                        view["message"].as_str().unwrap_or_default().contains(name),
                        "{at} was refused without naming {name}: {view}"
                    );
                    continue;
                }
                assert_eq!(status, StatusCode::OK, "{at}: {view}");
                assert!(sounds(&view), "{at} is silent and does not say why: {view}");
                sounded += 1;
            }
        }
    }

    assert!(
        sounded > 0,
        "every setting on every slider was refused — nothing here makes a sound"
    );
}

/// Whether a score has anything a listener would hear: a texture's gains or a
/// note mapping's events. Consonants do not count — every mapping carries them,
/// so a pitched layer that fell silent would hide behind them.
fn sounds(view: &Value) -> bool {
    let gains = view["gains"].as_array().unwrap();
    let audible = gains.iter().any(|voice| {
        voice
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g.as_f64().unwrap_or(0.0) > 0.0)
    });
    audible || !view["events"].as_array().unwrap().is_empty()
}

/// Both ends of a knob's published range and one position between, on the step
/// grid: the ends are what the range promises, the middle what everyone uses.
fn ends_and_middle(knob: &Value) -> Vec<f32> {
    let (min, max, step) = (
        knob["min"].as_f64().unwrap() as f32,
        knob["max"].as_f64().unwrap() as f32,
        knob["step"].as_f64().unwrap() as f32,
    );
    let on_grid = |raw: f32| (min + ((raw - min) / step).round() * step).clamp(min, max);
    let mut values = vec![min, on_grid(f32::midpoint(min, max)), max];
    values.dedup();
    values
}

/// A value a quarter of the way from a knob's default toward its far end:
/// audible, and somewhere a person would plausibly leave the slider.
fn a_quarter_from_default(knob: &Value) -> f32 {
    let (min, max, step, default) = (
        knob["min"].as_f64().unwrap() as f32,
        knob["max"].as_f64().unwrap() as f32,
        knob["step"].as_f64().unwrap() as f32,
        knob["default"].as_f64().unwrap() as f32,
    );
    let far = if (default - min).abs() > (max - default).abs() {
        min
    } else {
        max
    };
    let raw = default + (far - default) * 0.25;
    // Onto the step grid, since that is the only place the UI can put it.
    (min + ((raw - min) / step).round() * step).clamp(min, max)
}

#[tokio::test]
async fn the_scale_shown_is_the_scale_that_sounds() {
    // The scale shown must be the scale the render plays.
    let app = TestApp::new();
    upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/voice?bind=0")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    for degree in body["degrees"].as_array().unwrap() {
        let cents = degree["cents"].as_f64().unwrap();
        let off = cents - (cents / 100.0).round() * 100.0;
        assert!(
            off.abs() < 1.0,
            "bind=0 reported {cents}¢, which is {off:.1}¢ off equal temperament"
        );
    }
}

#[tokio::test]
async fn every_published_mapping_can_be_rendered() {
    // The UI offers exactly what the render accepts, so a listed mapping cannot 400.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (_, controls) = send(
        &app,
        Request::get("/api/controls").body(Body::empty()).unwrap(),
    )
    .await;
    let mappings = controls["mappings"].as_array().unwrap();
    assert!(!mappings.is_empty(), "no mappings published at all");

    for mapping in mappings {
        let name = mapping["name"].as_str().unwrap();
        let (status, _, audio) =
            fetch(&app, &format!("/api/recordings/{id}/render?mapping={name}")).await;
        assert_eq!(status, StatusCode::OK, "{name} was refused");
        assert!(!audio.is_empty(), "{name} rendered nothing");

        // ...and made tones, not only consonants: a toneless render still has
        // the right length.
        let (_, score) = send(
            &app,
            Request::get(format!("/api/recordings/{id}/score?mapping={name}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let sounded = score["voices"].as_array().map_or(0, std::vec::Vec::len)
            + score["events"].as_array().map_or(0, std::vec::Vec::len);
        assert!(sounded > 0, "{name} sounded no pitched material: {score}");
    }
}

#[tokio::test]
async fn taking_no_knobs_renders_the_defaults() {
    // Every earlier render has to stay comparable with every later one.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (_, _, plain) = fetch(&app, &format!("/api/recordings/{id}/render")).await;
    let (_, _, explicit) = fetch(
        &app,
        &format!("/api/recordings/{id}/render?bind=1&voices=5&spacing=2&drift=0.25&reach=1"),
    )
    .await;
    assert_eq!(plain, explicit);
}

#[tokio::test]
async fn the_score_describes_the_render_it_shares_a_url_with() {
    // The chart must describe the audio at the matching URL.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, plain) = send(
        &app,
        Request::get(format!("/api/recordings/{id}/score"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plain}");

    // Every stream the same length, or they cannot share a time axis.
    let points = plain["colour"].as_array().unwrap().len();
    assert!(points > 0, "no colour stream");
    assert_eq!(plain["breath"].as_array().unwrap().len(), points);
    assert_eq!(plain["level"].as_array().unwrap().len(), points);
    for voice in plain["voices"].as_array().unwrap() {
        assert_eq!(voice.as_array().unwrap().len(), points);
    }
    assert_eq!(
        plain["gains"].as_array().unwrap().len(),
        plain["voices"].as_array().unwrap().len()
    );

    // The time axis must be real, or a click seeks to the wrong second.
    let step = plain["stepS"].as_f64().unwrap();
    let duration = plain["durationS"].as_f64().unwrap();
    let spanned = step * points as f64;
    assert!(
        (spanned - duration).abs() < duration * 0.05,
        "{points} points of {step}s span {spanned}s against a {duration}s take"
    );

    // The knobs must reach it, or both sides are the same picture.
    let (_, bound) = send(
        &app,
        Request::get(format!("/api/recordings/{id}/score?bind=0"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(
        plain["degrees"], bound["degrees"],
        "bind did not reach the score view"
    );
}

#[tokio::test]
async fn the_score_never_exceeds_what_a_chart_can_draw() {
    // Thousands of frames per stream would be megabytes of sub-pixel detail.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(20.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (_, view) = send(
        &app,
        Request::get(format!("/api/recordings/{id}/score"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let points = view["colour"].as_array().unwrap().len();
    assert!(points <= 1200, "{points} points is more than a chart needs");
    // ...and not so few that the shape is gone.
    assert!(points > 100, "only {points} points for a 20-second take");
}

#[tokio::test]
async fn a_note_mapping_reports_its_notes_and_no_streams() {
    // `notes` has no per-frame material: empty series, not invented ones.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    let (status, view) = send(
        &app,
        Request::get(format!("/api/recordings/{id}/score?mapping=notes"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert!(view["colour"].as_array().unwrap().is_empty());
    assert!(view["voices"].as_array().unwrap().is_empty());
    assert!(
        !view["events"].as_array().unwrap().is_empty(),
        "a note mapping with no notes"
    );
}

#[tokio::test]
async fn audio_can_be_seeked_in() {
    // Without `Accept-Ranges` an `<audio>` element cannot seek, and the compare
    // page's jump button does nothing.
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    for path in [
        format!("/api/recordings/{id}/render"),
        format!("/api/recordings/{id}/audio"),
    ] {
        let res = app
            .router
            .clone()
            .oneshot(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            res.headers()
                .get("accept-ranges")
                .map(|v| v.to_str().unwrap()),
            Some("bytes"),
            "{path} never told the browser it could be seeked in"
        );
    }
}

#[tokio::test]
async fn a_range_request_is_answered_with_that_range() {
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();
    let path = format!("/api/recordings/{id}/render");

    let (_, _, whole) = fetch(&app, &path).await;

    let res = app
        .router
        .clone()
        .oneshot(
            Request::get(&path)
                .header("range", "bytes=1000-1999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("bytes 1000-1999/{}", whole.len())
    );

    let part = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(part.len(), 1000);
    // The bytes served must be the bytes asked for, or a seek lands elsewhere.
    assert_eq!(&part[..], &whole[1000..2000]);
}

#[tokio::test]
async fn a_range_past_the_end_is_refused_rather_than_truncated_wrongly() {
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();
    let path = format!("/api/recordings/{id}/render");
    let (_, _, whole) = fetch(&app, &path).await;

    // A start past the end gets the whole file, which an element recovers from.
    let res = app
        .router
        .clone()
        .oneshot(
            Request::get(&path)
                .header("range", format!("bytes={}-", whole.len() + 10))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // An open-ended range runs to the last byte.
    let res = app
        .router
        .clone()
        .oneshot(
            Request::get(&path)
                .header("range", "bytes=100-")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    let part = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(part.len(), whole.len() - 100);
}

#[tokio::test]
async fn other_peoples_singing_does_not_shape_the_speaker() {
    // Other people's singing, pooled into the profile, would describe an
    // anatomy belonging to nobody. The derived voice must not move when a
    // stranger's take arrives.
    let app = TestApp::new();
    upload(&app, "vowel-ah", wav_fixture_moving_vowel(8.0)).await;
    let (status, before) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{before}");

    upload_material(&app, "somebody-else", wav_fixture(4.0)).await;
    let (status, after) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");

    assert_eq!(
        before["degrees"], after["degrees"],
        "material changed the speaker's scale"
    );
    assert_eq!(
        before["tonicHz"], after["tonicHz"],
        "material moved where the speaker's music centres"
    );
}

#[tokio::test]
async fn a_store_with_nothing_but_material_says_to_calibrate() {
    // Refusing is right, and the message must say what to do.
    let app = TestApp::new();
    upload_material(&app, "somebody-else", wav_fixture_moving_vowel(8.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("calibration"),
        "the refusal does not name what is missing: {message}"
    );
}

#[tokio::test]
async fn a_stored_take_can_be_told_what_it_is_for() {
    // A take stored as material must be able to become a calibration one.
    let app = TestApp::new();
    let (_, body) = upload_material(&app, "vowel-ah", wav_fixture_moving_vowel(8.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();

    // Refuses first, which is what makes the rest of the test mean anything.
    let (status, _) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a store of material derived a voice"
    );

    let (status, updated) = send(
        &app,
        Request::put(format!("/api/recordings/{id}/role"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"role":"calibration"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["role"], "calibration");

    let (status, _) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the take was marked and still did not count"
    );
}

#[tokio::test]
async fn saying_what_a_take_is_for_does_not_touch_the_audio() {
    // Safe to expose because the role is metadata and the voiceprint a pure
    // function of the audio; a render runs the whole chain to prove it.
    let app = TestApp::new();
    let (_, body) = upload(&app, "vowel-ah", wav_fixture_moving_vowel(8.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();
    let (_, _, before) = fetch(&app, &format!("/api/recordings/{id}/audio")).await;

    for role in ["material", "calibration"] {
        let (status, _) = send(
            &app,
            Request::put(format!("/api/recordings/{id}/role"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"role":"{role}"}}"#)))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (_, _, after) = fetch(&app, &format!("/api/recordings/{id}/audio")).await;
    assert_eq!(
        before, after,
        "the audio changed when only the role was set"
    );
}

#[tokio::test]
async fn a_role_set_on_an_unknown_take_is_a_404() {
    let app = TestApp::new();
    let (status, _) = send(
        &app,
        Request::put("/api/recordings/deadbeefdeadbeef/role")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"role":"calibration"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A held vowel with the given formants, continuous rather than in bursts —
/// bursts would spend most of the take on silence with no formants.
fn wav_fixture_held_vowel(secs: f32, f1: f32, f2: f32) -> Vec<u8> {
    const RATE: u32 = 16_000;
    const F0: f32 = 125.0;
    let formant = |hz: f32, center: f32, bw: f32| 1.0 / (1.0 + ((hz - center) / bw).powi(2));

    let harmonics: Vec<(f32, f32)> = (1..(8_000.0 / F0).ceil() as u32)
        .map(|k| k as f32 * F0)
        .map(|hz| {
            (
                hz,
                // The moving-vowel fixture's shallow source, for its reason.
                (F0 / hz).sqrt()
                    * (formant(hz, f1, 90.0)
                        + 0.7 * formant(hz, f2, 110.0)
                        + 0.3 * formant(hz, 2800.0, 160.0)),
            )
        })
        .collect();

    let total = (RATE as f32 * secs) as usize;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
        for i in 0..total {
            let t = i as f32 / RATE as f32;
            let s: f32 = harmonics
                .iter()
                .map(|&(hz, gain)| gain * (2.0 * std::f32::consts::PI * hz * t).sin())
                .sum::<f32>()
                * 0.3;
            w.write_sample((s.clamp(-1.0, 1.0) * 32_767.0) as i16)
                .unwrap();
        }
        w.finalize().unwrap();
    }
    buf.into_inner()
}

#[tokio::test]
async fn the_speakers_own_vowel_corners_come_from_the_steps_they_recorded() {
    let app = TestApp::new();
    // Labelled as the guided flow labels them; the label says which vowel.
    upload(&app, "vowel-ee", wav_fixture_held_vowel(3.0, 300.0, 2300.0)).await;
    upload(&app, "vowel-ah", wav_fixture_held_vowel(3.0, 730.0, 1100.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/speaker/corners")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let corners = body["corners"].as_array().unwrap();
    assert_eq!(corners.len(), 2, "{corners:?}");

    // Front before open: the order the guided flow asks for them in.
    assert_eq!(corners[0]["step"], "vowel-ee");
    assert_eq!(corners[0]["corner"], "closeFront");
    assert_eq!(corners[1]["step"], "vowel-ah");
    assert_eq!(corners[1]["corner"], "open");

    // Asserted as the relation between the vowels — *ee* closer and fronter than
    // *ah* — not exact centres, which would test the formant tracker
    // (`formant_real.rs`).
    let (ee_f1, ee_f2) = (
        corners[0]["f1Hz"].as_f64().unwrap(),
        corners[0]["f2Hz"].as_f64().unwrap(),
    );
    let (ah_f1, ah_f2) = (
        corners[1]["f1Hz"].as_f64().unwrap(),
        corners[1]["f2Hz"].as_f64().unwrap(),
    );
    assert!(ee_f1 < ah_f1, "ee F1 {ee_f1} should be below ah's {ah_f1}");
    assert!(ee_f2 > ah_f2, "ee F2 {ee_f2} should be above ah's {ah_f2}");

    // Held still, so the spread is a fraction of the distance between them.
    let spread = corners[0]["f2SpreadHz"].as_f64().unwrap();
    assert!(
        spread < (ee_f2 - ah_f2) / 4.0,
        "a held vowel reported a spread of {spread} Hz"
    );
    assert!(corners[0]["frames"].as_u64().unwrap() >= 100);
}

#[tokio::test]
async fn a_take_that_names_no_step_places_no_corner() {
    let app = TestApp::new();
    // Calibration, but its label names no step, as an uploaded file's does: it
    // pools into the profile and must not be shown as a vowel.
    upload(
        &app,
        "some-recording.wav",
        wav_fixture_held_vowel(3.0, 300.0, 2300.0),
    )
    .await;
    // Material never defines the speaker at all, whatever it is called.
    upload_material(&app, "vowel-oo", wav_fixture_held_vowel(3.0, 300.0, 870.0)).await;

    let (status, body) = send(
        &app,
        Request::get("/api/speaker/corners")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["corners"].as_array().unwrap().len(), 0, "{body}");
}

#[tokio::test]
async fn corners_are_reported_from_takes_too_short_to_derive_a_scale_from() {
    let app = TestApp::new();
    // Too short for a scale, so `/api/voice` refuses; the corners are still
    // measured, which is why they are not part of the voice summary.
    upload(&app, "vowel-ee", wav_fixture_held_vowel(1.6, 300.0, 2300.0)).await;

    let (status, _) = send(
        &app,
        Request::get("/api/voice").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the scale should be refused"
    );

    let (status, body) = send(
        &app,
        Request::get("/api/speaker/corners")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["corners"].as_array().unwrap().len(), 1, "{body}");
}
