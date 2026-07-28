//! End-to-end tests over the real router.
//!
//! Driven in-process through `tower::ServiceExt::oneshot` rather than over a
//! socket: no port to clash on, no server to wait for, and the whole stack —
//! routing, extractors, body limit, error mapping — is still exercised.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use music::config::Config;
use music::routes;
use music::state::AppState;
use music::store::Store;
use music_analysis::voiceprint::SCHEMA_VERSION;
use serde_json::Value;
use tower::ServiceExt;

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
            "music-api-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));

        let cfg = Config {
            bind_addr: "127.0.0.1:0".into(),
            data_dir: dir.clone(),
            // API-only: these tests are about the API, and a static dir would
            // add a fallback that turns a routing mistake into a 200 with an
            // HTML body.
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
        // Best-effort: a test that already failed should report its own reason,
        // not a cleanup error on top.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A spoken-ish vowel at 16 kHz mono: a harmonic series under a two-formant
/// envelope, gated into bursts so there is something for every part of the
/// voiceprint to find — pitch in the bursts, silence between them, onsets at
/// the edges.
fn wav_fixture(secs: f32) -> Vec<u8> {
    wav_fixture_at(secs, 16_000, 1)
}

/// The same fixture at an arbitrary rate and channel count.
///
/// Built by tiling one second of audio: the fundamental and the burst period
/// both divide a second exactly, so the tiles join seamlessly and a long file
/// costs no more to synthesise than a short one. Without that, generating half a
/// minute at 48 kHz dominates the runtime of the whole test suite.
fn wav_fixture_at(secs: f32, rate: u32, channels: u16) -> Vec<u8> {
    /// Divides one second a whole number of times, so tiles join without a click.
    const F0: f32 = 125.0;
    let formant = |hz: f32, center: f32, bw: f32| 1.0 / (1.0 + ((hz - center) / bw).powi(2));

    // Band-limited to 8 kHz whatever the sample rate: a real voice has next to
    // nothing above that, and it keeps the harmonic sum the same size at 48 kHz.
    let harmonics: Vec<(f32, f32)> = (1..)
        .map(|k| k as f32 * F0)
        .take_while(|&hz| hz < 8_000.0_f32.min(rate as f32 / 2.0))
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

/// Send a request to a JSON endpoint and parse the response.
///
/// Parsing failure is a hard failure, not an empty `Value::Null`: every endpoint
/// reached through here answers in JSON on success *and* on error, so
/// unparseable bytes mean the handler did something we did not expect — exactly
/// the thing a test must not quietly turn into "no fields present".
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

async fn upload(app: &TestApp, label: &str, wav: Vec<u8>) -> (StatusCode, Value) {
    let req = Request::post(format!("/api/recordings?label={label}"))
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

    // The point of the whole pipeline: a voice-shaped input must come back with
    // populated series, not an empty document that technically validates.
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

    // Recording quality reaches the summary, so the take list can flag a bad
    // take without opening every voiceprint. This fixture sits below the rail.
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
    // Comfortably past axum's 2 MB default, using 48 kHz stereo to get there in
    // twelve seconds of fixture. The app itself records 48 kHz *mono* (~96 KB/s),
    // where a real half-minute take is about 2.9 MB — over the default either
    // way, so without the raised limit every recording would be rejected at the
    // door. Stereo is used here only to keep the fixture short.
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

/// A take whose vowel actually moves, so the speaker profile has a vowel space
/// with width to it.
///
/// The plain fixture holds one vowel throughout, which is correct for what it
/// tests and useless here: a speaker who never moved their tongue has a vowel
/// space of zero extent, and calibration rightly refuses to normalise into one.
/// This alternates two vowels burst by burst — roughly *ah* and *ee*.
fn wav_fixture_moving_vowel(secs: f32) -> Vec<u8> {
    const RATE: u32 = 16_000;
    const F0: f32 = 125.0;
    let formant = |hz: f32, center: f32, bw: f32| 1.0 / (1.0 + ((hz - center) / bw).powi(2));

    let total = (RATE as f32 * secs) as usize;
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            if (t / 0.5).fract() >= 0.6 {
                return 0.0;
            }
            // Alternate vowels every half second, so both ends of the space are
            // visited often enough to survive the profile's percentile trim.
            let (f1, f2) = if ((t / 0.5) as u32).is_multiple_of(2) {
                (730.0, 1090.0)
            } else {
                (300.0, 2300.0)
            };
            (1..)
                .map(|k| k as f32 * F0)
                .take_while(|&hz| hz < 8_000.0)
                .map(|hz| {
                    let gain = (F0 / hz) * (formant(hz, f1, 90.0) + 0.5 * formant(hz, f2, 110.0));
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

    // Eight seconds at 44.1 kHz in 16-bit mono is about 700 KB. Anything much
    // smaller is a header with no music behind it.
    assert!(
        bytes.len() > 400_000,
        "rendered only {} bytes — the score was probably empty",
        bytes.len()
    );
}

#[tokio::test]
async fn a_render_is_the_same_every_time() {
    // Determinism reaches all the way to the output: the same take, the same
    // calibration and the same mapping must give byte-identical audio, or "the
    // mapping changed" cannot be told from "the renderer wandered".
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

    // The palette is what gives the tone somewhere to travel; an empty one
    // renders silence, and a single entry renders a colour that never moves.
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
    // The message has to say what to do, since this is the state every new
    // installation starts in.
    let message = body["error"].as_str().unwrap_or_default().to_string()
        + body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("record"),
        "unhelpful message for an empty store: {body}"
    );
}

#[tokio::test]
async fn refuses_to_calibrate_from_material_that_never_held_a_pitch() {
    // A take with no sustained phonation cannot give a harmonic series, and a
    // scale derived from one would be arithmetic on noise reported with full
    // confidence.
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
    // Which vowel a tuning comes from is unsettled, and the automatic choice is
    // a heuristic. A listener who disagrees has to be able to say so, or the
    // heuristic quietly becomes the decision.
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
    // Silently falling back to the automatic choice would render music in a
    // scale the caller did not ask for and report success.
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
    // Eligibility before preference. A brief take can measure a rich-looking
    // spectrum, and choosing on richness alone would pick it and then refuse it
    // for being too short — reporting no music while a good calibration take sat
    // in the store unexamined.
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
    // They are alternatives over one voiceprint, and the only way to judge
    // either is against the other. If they rendered the same bytes, the choice
    // would be doing nothing.
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

#[tokio::test]
async fn an_unknown_mapping_is_refused_rather_than_ignored() {
    // Silently falling back to the default would render something the caller
    // did not ask for and report success.
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
