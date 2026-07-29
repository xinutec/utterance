//! End-to-end tests over the real router.
//!
//! Driven in-process through `tower::ServiceExt::oneshot` rather than over a
//! socket: no port to clash on, no server to wait for, and the whole stack —
//! routing, extractors, body limit, error mapping — is still exercised.

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

/// Store a take that defines the speaker.
///
/// Calibration rather than material, because in these tests the uploaded
/// fixture *is* the voice: everything derived — the scale, the timbre, the vowel
/// space — has to come from somewhere, and there is nothing else in the store.
/// `upload_material` is for the tests that care about the difference.
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
    // Deterministic, so the fixture is the same every run.
    let mut state: u32 = 0x9E37_79B9;
    let mut hiss = (0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            if (t / 0.5).fract() >= 0.6 {
                return 0.0;
            }
            // The last 80 ms of each burst is a fricative rather than a vowel:
            // band-limited noise where the tone would be. Without it the fixture
            // is all vowel and all silence, and nothing exercises the consonant
            // path — a knob that turns consonants off then changes nothing, and
            // a test that sweeps it passes while proving nothing.
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

            // Alternate vowels every half second, so both ends of the space are
            // visited often enough to survive the profile's percentile trim.
            //
            // **Three formants, and a source shallower than the textbook.** A
            // real *ah* measured through this same code gives eight scale
            // degrees; this fixture with two formants and a 1/k source gave
            // four, of which the two deepest were the fourth and the fifth —
            // the one pair of intervals that spans no harmonic lattice, so the
            // mapping built on that geometry had nothing to stand on. The
            // fixture was quietly less of a voice than any voice.
            //
            // A glottal source really does fall at about 6 dB per octave once
            // radiation is counted, so the slope here is not physics: it stands
            // in for everything else that puts energy in a real voice's upper
            // partials and that a sum of pure sines has none of — jitter,
            // shimmer, glottal noise, source-tract coupling. Tuned until the
            // measured partials look like a measured voice's, which is the only
            // thing this fixture is for.
            let (f1, f2, f3) = if ((t / 0.5) as u32).is_multiple_of(2) {
                (730.0, 1090.0, 2440.0)
            } else {
                (300.0, 2300.0, 3000.0)
            };
            (1..)
                .map(|k| k as f32 * F0)
                .take_while(|&hz| hz < 8_000.0)
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

/// The density at which the fixture's scale stops spanning a plane.
///
/// Top of the knob's published range. Which value does it is a property of the
/// speaker rather than a constant — a real take goes thin somewhere near 0.14 —
/// so the tests that use this check the scale really did collapse rather than
/// trusting the number.
const DENSITY_TOO_HIGH: &str = "density=0.5";

#[tokio::test]
async fn a_scale_too_thin_for_a_lattice_says_so_rather_than_rendering_silence() {
    // The failure this replaces was silent and looked exactly like a bug: the
    // lattice mapping declines a scale that points one way only, a score with no
    // field in it renders to consonants over silence, and the response is a
    // perfectly good 200 full of nothing.
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

    // Checked rather than assumed, so this cannot quietly become a test of a
    // scale that was fine all along.
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

    // The summary is what the studio reads before it points a player anywhere,
    // because an `<audio>` element handed a failing URL shows a broken control
    // and no message.
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
    // The refusal is the lattice's, not the density knob's. Every other mapping
    // works from a list of degrees and plays whatever is left, so a scale pruned
    // past a plane must still make music by some other route — otherwise this
    // reads as the knob having a broken upper half.
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
    // The other half of the claim, and the one that catches a check left
    // permanently on: at the default density the lattice plays, and a warning
    // shown then would train someone to ignore it.
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

#[tokio::test]
async fn both_mappings_can_sound_together() {
    // Freedom to combine, not only to choose: a stream of events over a texture
    // is a third thing, and neither mapping alone can produce it.
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
    // A knob that silently does nothing is worse than no knob — and the list is
    // taken from what the API publishes rather than written out here, so a knob
    // added to the table and never wired into the render fails this test instead
    // of appearing in the UI as a slider that does nothing.
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

    // Each knob is swept against the mappings it says it reaches, so a knob
    // belonging to one mapping is not asked to change another. A claim made in
    // the table is a claim checked here — and one made falsely fails, which is
    // the point of letting a knob make it.
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
                // A refusal is a change, and the loudest one available: a value
                // the slider can reach that this mapping has no answer for.
                // Density does it to the lattice a quarter of the way up, by
                // pruning the scale past a plane. Accepted only when it explains
                // itself — an unexplained one is the silence this check exists
                // to catch — and safe to accept because
                // `every_published_mapping_can_be_rendered` fails if a mapping
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
    // The generalisation of a real failure. `density` past about a quarter of its
    // travel prunes this speaker's scale below a plane, and the lattice mapping
    // answered with a perfectly good 200 containing no field — audible as
    // nothing, and reported as success. The test above sweeps one value per
    // knob, which is the wrong shape for this: what a published range promises
    // is that *every* position on the slider means something, and the ends are
    // exactly where nobody drags by hand.
    //
    // Two acceptable answers per position, and the whole point is that there is
    // no third: it makes sound, or it refuses and says which setting to move.
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

    // Tallied so this cannot pass by refusing everything, which would satisfy
    // the letter of the check and leave an app that makes no sound.
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
                // The score rather than the render: it is the same decision made
                // by the same code, without the seconds of synthesis after it,
                // and it is the thing that says whether anything sounds.
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

/// Whether a score has anything in it a listener would hear.
///
/// Either material counts, because the mappings make different ones — a texture
/// is heard through its gains and a note mapping through its events. The
/// consonants deliberately do not count: they are carried by every mapping, so a
/// pitched layer that fell silent would hide behind them, which is precisely how
/// the lattice failure sounded.
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

/// Both ends of a knob's published range, and one position between them.
///
/// The ends because they are what the range promises and what a slider dragged
/// to its stop produces; the middle because a range can also fail in the part
/// everyone does use. All three land on the step grid, which is the only place
/// the UI can put a value.
fn ends_and_middle(knob: &Value) -> Vec<f32> {
    let (min, max, step) = (
        knob["min"].as_f64().unwrap() as f32,
        knob["max"].as_f64().unwrap() as f32,
        knob["step"].as_f64().unwrap() as f32,
    );
    let on_grid = |raw: f32| (min + ((raw - min) / step).round() * step).clamp(min, max);
    let mut values = vec![min, on_grid((min + max) / 2.0), max];
    values.dedup();
    values
}

/// A value a quarter of the way from a knob's default toward its far end.
///
/// Far enough to be audible, near enough that it stays somewhere a person would
/// plausibly leave the slider — so a knob failing the test above has failed at a
/// setting someone would really use, not at an extreme nothing was built for.
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
    // The summary is what someone reads while deciding whether they like the
    // tuning. Reporting the derived degrees beside a render that snapped them to
    // equal temperament would misrepresent the one claim this project makes.
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
    // Same contract in the other direction: the UI offers exactly what the
    // render route accepts, so a listed mapping cannot 400.
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

        // ...and rendered the pitched material, not only the consonants. A
        // mapping that quietly produces no tones still answers 200 with a file
        // of the right length, which is a way for one to be broken and listed.
        let (_, score) = send(
            &app,
            Request::get(format!("/api/recordings/{id}/score?mapping={name}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let sounded = score["voices"].as_array().map(|v| v.len()).unwrap_or(0)
            + score["events"].as_array().map(|n| n.len()).unwrap_or(0);
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
    // The chart drawn from this sits beside a player pointed at the render. A
    // score that described a different set of parameters would be worse than no
    // chart at all, because the chart is the part someone would believe.
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

    // Every per-frame stream is the same length, or they cannot be drawn on one
    // time axis — which is the only thing this endpoint exists for.
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

    // The time axis has to be real, or a click on the chart seeks to the wrong
    // second.
    let step = plain["stepS"].as_f64().unwrap();
    let duration = plain["durationS"].as_f64().unwrap();
    let spanned = step * points as f64;
    assert!(
        (spanned - duration).abs() < duration * 0.05,
        "{points} points of {step}s span {spanned}s against a {duration}s take"
    );

    // And the knobs have to reach it, or the two sides of a comparison are the
    // same picture twice.
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
    // A 46-second take is thousands of frames per stream across a dozen streams.
    // Sending all of it costs megabytes to draw sub-pixel detail nobody sees.
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
    // `notes` has no per-frame material at all. Empty series is the honest shape;
    // a field synthesised from the notes so the chart has something to draw would
    // be the chart inventing its own subject.
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
    // An `<audio>` element only moves its playhead to a position it can fetch,
    // and without this it has no way to ask for one — so `currentTime = 27.5`
    // is dropped in silence and the compare page's jump-to-the-difference
    // button appears to do nothing.
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
    // The bytes served have to be the bytes asked for, or a seek lands somewhere
    // other than where the chart said it would.
    assert_eq!(&part[..], &whole[1000..2000]);
}

#[tokio::test]
async fn a_range_past_the_end_is_refused_rather_than_truncated_wrongly() {
    let app = TestApp::new();
    let (_, body) = upload(&app, "calibration", wav_fixture_moving_vowel(9.0)).await;
    let id = body["meta"]["id"].as_str().unwrap().to_string();
    let path = format!("/api/recordings/{id}/render");
    let (_, _, whole) = fetch(&app, &path).await;

    // A start beyond the file has no sensible partial answer; the whole file is
    // the safe response and is what an element recovers from.
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
    // **The bug this whole distinction exists to prevent.** A singer uploads
    // other voices to render, and pooling them into the profile measures a vowel
    // space, a pitch range and a timbre belonging to nobody. The project's claim
    // is that *this* speaker's spectrum gives *this* speaker's scale, and it is
    // worth nothing if the spectrum is a crowd.
    //
    // Two takes with genuinely different anatomy: one is the speaker, the other
    // is somebody else. The derived voice must not move when the stranger
    // arrives.
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
    // Refusing is right and the message has to say what to do about it. The
    // alternative — deriving a voice from whatever happens to be lying around —
    // is the failure above, reported as success.
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
