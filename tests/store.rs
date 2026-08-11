//! The recording store, over its public surface.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use utterance::store::{Role, Store, StoreError};
use utterance_analysis::resample::ANALYSIS_RATE;
use utterance_analysis::voiceprint::{Source, Voiceprint};

/// A store in a throwaway directory that removes itself when the test ends.
struct TempStore {
    store: Store,
    root: PathBuf,
}

impl TempStore {
    fn open() -> Self {
        // Unique per process and per call, so parallel test threads never share.
        static N: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "utterance-store-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        Self {
            store: Store::open(&root).expect("open store"),
            root,
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl std::ops::Deref for TempStore {
    type Target = Store;
    fn deref(&self) -> &Store {
        &self.store
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        // Best-effort: a test that already failed should report its own reason,
        // not a cleanup error on top.
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn a_voiceprint() -> Voiceprint {
    let samples: Vec<f32> = (0..ANALYSIS_RATE as usize)
        .map(|i| (2.0 * std::f32::consts::PI * 140.0 * i as f32 / ANALYSIS_RATE as f32).sin())
        .collect();
    utterance_analysis::analyse(
        &samples,
        Source {
            sample_rate_hz: ANALYSIS_RATE,
            channels: 1,
            duration_s: 1.0,
            peak: 1.0,
            clipped_fraction: 0.0,
        },
    )
}

/// Real WAV bytes, so a record can be re-analysed from its audio.
fn wav(secs: f32) -> Vec<u8> {
    let n = (ANALYSIS_RATE as f32 * secs) as usize;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: ANALYSIS_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut buf, spec).expect("wav writer");
        for i in 0..n {
            let t = i as f32 / ANALYSIS_RATE as f32;
            let v = (2.0 * std::f32::consts::PI * 140.0 * t).sin() * 0.5;
            w.write_sample((v * 32_767.0) as i16).expect("write sample");
        }
        w.finalize().expect("finalize");
    }
    buf.into_inner()
}

#[test]
fn a_stale_voiceprint_is_re_analysed_from_its_audio() {
    // The whole reason bumping SCHEMA_VERSION is cheap: the audio is the source
    // of truth and analysis is deterministic, so an out-of-date voiceprint is a
    // stale cache, not lost data.
    let store = TempStore::open();
    let audio = wav(1.0);
    let meta = store
        .put(&audio, "an old take", &a_voiceprint(), Role::Material)
        .unwrap();

    // Downgrade the stored voiceprint the way a schema bump leaves it: an old
    // version, missing the fields the current analyser adds.
    let path = store.path().join(&meta.id).join("voiceprint.json");
    fs::write(
        &path,
        r#"{"schemaVersion":1,"source":{"sampleRateHz":16000}}"#,
    )
    .unwrap();

    let rebuilt = store.voiceprint(&meta.id).unwrap();
    assert_eq!(rebuilt.schema_version, a_voiceprint().schema_version);
    assert!(
        rebuilt.frame.count > 0,
        "re-analysis produced an empty voiceprint"
    );
    // The label is the one thing not recoverable from audio; it must survive.
    assert_eq!(store.meta(&meta.id).unwrap().label, "an old take");
}

#[test]
fn a_rebuild_keeps_the_original_ordering() {
    // created_at_ms is preserved across a re-analysis, so refreshing a stale
    // record does not jump it to the top of the take list.
    let store = TempStore::open();
    let meta = store
        .put(&wav(1.0), "old", &a_voiceprint(), Role::Material)
        .unwrap();
    let path = store.path().join(&meta.id).join("voiceprint.json");
    fs::write(&path, r#"{"schemaVersion":1}"#).unwrap();

    assert_eq!(
        store.meta(&meta.id).unwrap().created_at_ms,
        meta.created_at_ms
    );
}

#[test]
fn stores_and_reads_back_a_recording() {
    let store = TempStore::open();
    let stored = a_voiceprint();
    let meta = store
        .put(
            b"fake audio bytes",
            "brother, take 1",
            &stored,
            Role::Material,
        )
        .unwrap();

    assert_eq!(store.audio(&meta.id).unwrap(), b"fake audio bytes");
    assert_eq!(store.meta(&meta.id).unwrap().label, "brother, take 1");
    // Compared against what went in rather than a literal, so bumping the
    // schema version does not require editing this test.
    let read_back = store.voiceprint(&meta.id).unwrap();
    assert_eq!(read_back.schema_version, stored.schema_version);
    assert_eq!(read_back.frame.count, stored.frame.count);
}

#[test]
fn identical_audio_gets_the_same_id() {
    let store = TempStore::open();
    let a = store
        .put(b"same bytes", "take 1", &a_voiceprint(), Role::Material)
        .unwrap();
    let b = store
        .put(b"same bytes", "take 2", &a_voiceprint(), Role::Material)
        .unwrap();

    assert_eq!(a.id, b.id);
    assert_eq!(
        store.list().unwrap().len(),
        1,
        "a re-upload created a duplicate"
    );
}

#[test]
fn different_audio_gets_a_different_id() {
    let store = TempStore::open();
    let a = store
        .put(b"one recording", "a", &a_voiceprint(), Role::Material)
        .unwrap();
    let b = store
        .put(b"another recording", "b", &a_voiceprint(), Role::Material)
        .unwrap();

    assert_ne!(a.id, b.id);
}

#[test]
fn an_empty_label_falls_back_to_the_id() {
    let store = TempStore::open();
    let meta = store
        .put(b"audio", "   ", &a_voiceprint(), Role::Material)
        .unwrap();

    assert_eq!(meta.label, meta.id);
}

#[test]
fn metadata_summarises_the_voiceprint() {
    let store = TempStore::open();
    let vp = a_voiceprint();
    let meta = store.put(b"audio", "take", &vp, Role::Material).unwrap();

    assert_eq!(meta.duration_s, vp.source.duration_s);
    assert_eq!(meta.sample_rate_hz, vp.source.sample_rate_hz);
    assert_eq!(meta.voiced_fraction, vp.pitch.voiced_fraction());
    assert_eq!(meta.onset_count, vp.events.onset_frames.len());
}

#[test]
fn listing_a_store_whose_directory_is_gone_is_not_an_error() {
    // The supported way to start over is deleting data/; the app must survive it
    // rather than refusing to load.
    let store = TempStore::open();
    fs::remove_dir_all(store.path()).unwrap();

    assert!(store.list().unwrap().is_empty());
}

#[test]
fn listing_is_newest_first() {
    let store = TempStore::open();
    let a = store
        .put(b"first", "a", &a_voiceprint(), Role::Material)
        .unwrap();
    // created_at_ms has millisecond resolution; two puts in the same millisecond
    // would tie, so make the ordering observable.
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = store
        .put(b"second", "b", &a_voiceprint(), Role::Material)
        .unwrap();

    let ids: Vec<String> = store.list().unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![b.id, a.id]);
}

#[test]
fn deletes_a_recording() {
    let store = TempStore::open();
    let meta = store
        .put(b"audio", "x", &a_voiceprint(), Role::Material)
        .unwrap();
    store.delete(&meta.id).unwrap();

    assert!(matches!(store.meta(&meta.id), Err(StoreError::NotFound(_))));
}

#[test]
fn unknown_ids_are_not_found() {
    let store = TempStore::open();

    assert!(matches!(
        store.meta("0123456789abcdef"),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn ids_that_are_not_ours_are_not_found_rather_than_resolved() {
    // Ids arrive from the URL path. Validating the shape means `../` is rejected
    // as an unknown recording instead of reaching the filesystem at all.
    let store = TempStore::open();
    for id in [
        "../../etc/passwd",
        "/etc/passwd",
        "..",
        "0123456789ABCDEF",
        "short",
        "",
    ] {
        assert!(
            matches!(store.meta(id), Err(StoreError::NotFound(_))),
            "id {id:?} was not rejected"
        );
        assert!(
            matches!(store.audio(id), Err(StoreError::NotFound(_))),
            "id {id:?} was not rejected"
        );
    }
}

#[test]
fn a_take_that_did_not_say_what_it_was_for_is_material() {
    // The safe direction, and the one every take stored before roles existed
    // reads back as. A recording that never declared itself must not start
    // shaping the sound world — a store fills up with other people's singing.
    let store = TempStore::open();
    let meta = store
        .put(&wav(1.0), "somebody else", &a_voiceprint(), Role::Material)
        .unwrap();
    assert_eq!(meta.role, Role::Material);
    assert_eq!(store.meta(&meta.id).unwrap().role, Role::Material);
}

#[test]
fn a_calibration_take_stays_one_when_the_analyser_changes() {
    // **The failure this exists to prevent**, which nothing downstream could
    // detect: `ensure_current` rebuilds the metadata from the audio, and the
    // role is not in the audio. Defaulting it there would demote every
    // calibration take on the next schema bump and dissolve the speaker's whole
    // sound world for a reason nothing reports.
    let store = TempStore::open();
    let meta = store
        .put(&wav(1.0), "vowel-ah", &a_voiceprint(), Role::Calibration)
        .unwrap();

    // Downgrade the stored voiceprint the way a schema bump leaves it.
    fs::write(
        store.path().join(&meta.id).join("voiceprint.json"),
        r#"{"schemaVersion":1,"source":{"sampleRateHz":16000}}"#,
    )
    .unwrap();

    store.ensure_current(&meta.id).unwrap();
    assert_eq!(
        store.meta(&meta.id).unwrap().role,
        Role::Calibration,
        "re-analysis demoted a calibration take to material"
    );
}

// --- a take is replaced in one step, or not at all ---------------------------
//
// `fs::write` truncates and then writes, so between those two the file on disk
// is short. `read_json` maps a parse failure to `StoreError::Corrupt`, so a
// request arriving mid-write reads the take as CORRUPT rather than as its old
// or new self — and a crash in that window leaves it corrupt permanently. The
// audio is the largest of the three files and holds the window open longest.
//
// Ablation, 2026-08-11: reverting `store::write` to a plain `fs::write` fails
// both of these. An earlier pass at the same fix in memview had four tests that
// all still passed against the old code, because they pinned the OUTCOME —
// which truncate-and-rewrite also reaches. These pin the MECHANISM instead.

/// A rename REPLACES the directory entry, so the file is a different inode
/// afterwards. A truncate-and-write modifies it in place and keeps it. That is
/// the difference, observed directly and without a race.
#[test]
fn rewriting_a_take_replaces_the_file_rather_than_editing_it_in_place() {
    use std::os::unix::fs::MetadataExt;

    let store = TempStore::open();
    let id = store
        .put(&wav(1.0), "first", &a_voiceprint(), Role::Material)
        .expect("put")
        .id;

    let meta_path = store.path().join(&id).join("meta.json");
    let before = fs::metadata(&meta_path).expect("meta").ino();

    store.put_role(&id, Role::Calibration).expect("put_role");
    let after = fs::metadata(&meta_path).expect("meta").ino();

    assert_ne!(
        before, after,
        "meta.json was rewritten in place, so a reader can see it half-written \
         and a crash can leave it that way"
    );
}

/// The property: a reader running alongside a writer never sees a take as
/// corrupt. Timing-dependent by nature, which is why the inode test above
/// carries the deterministic half of the claim.
#[test]
fn a_reader_alongside_a_writer_never_sees_a_take_as_corrupt() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    let store = Arc::new(TempStore::open());
    let id = store
        .put(&wav(4.0), "under edit", &a_voiceprint(), Role::Material)
        .expect("put")
        .id;

    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let (store, stop, id) = (Arc::clone(&store), Arc::clone(&stop), id.clone());
        std::thread::spawn(move || {
            let mut seen = 0u32;
            while !stop.load(Ordering::Relaxed) {
                match store.meta(&id) {
                    Ok(_) => seen += 1,
                    Err(StoreError::Corrupt { detail, .. }) => {
                        panic!("read a half-written take: {detail}")
                    }
                    // Anything else is the take momentarily absent, which this
                    // is not about.
                    Err(_) => {}
                }
            }
            seen
        })
    };

    for i in 0..60 {
        let role = if i % 2 == 0 {
            Role::Calibration
        } else {
            Role::Material
        };
        store.put_role(&id, role).expect("put_role");
    }
    stop.store(true, Ordering::Relaxed);

    let seen = reader.join().expect("the reader saw a corrupt take");
    assert!(
        seen > 0,
        "the reader never got to look, so this proved nothing"
    );
}
