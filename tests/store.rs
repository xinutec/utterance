//! The recording store, over its public surface.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use music::store::{Store, StoreError};
use music_analysis::resample::ANALYSIS_RATE;
use music_analysis::voiceprint::{Source, Voiceprint};

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
            "music-store-test-{}-{}",
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
    music_analysis::analyse(
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

#[test]
fn stores_and_reads_back_a_recording() {
    let store = TempStore::open();
    let stored = a_voiceprint();
    let meta = store
        .put(b"fake audio bytes", "brother, take 1", &stored)
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
    let a = store.put(b"same bytes", "take 1", &a_voiceprint()).unwrap();
    let b = store.put(b"same bytes", "take 2", &a_voiceprint()).unwrap();

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
    let a = store.put(b"one recording", "a", &a_voiceprint()).unwrap();
    let b = store
        .put(b"another recording", "b", &a_voiceprint())
        .unwrap();

    assert_ne!(a.id, b.id);
}

#[test]
fn an_empty_label_falls_back_to_the_id() {
    let store = TempStore::open();
    let meta = store.put(b"audio", "   ", &a_voiceprint()).unwrap();

    assert_eq!(meta.label, meta.id);
}

#[test]
fn metadata_summarises_the_voiceprint() {
    let store = TempStore::open();
    let vp = a_voiceprint();
    let meta = store.put(b"audio", "take", &vp).unwrap();

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
    let a = store.put(b"first", "a", &a_voiceprint()).unwrap();
    // created_at_ms has millisecond resolution; two puts in the same millisecond
    // would tie, so make the ordering observable.
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = store.put(b"second", "b", &a_voiceprint()).unwrap();

    let ids: Vec<String> = store.list().unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![b.id, a.id]);
}

#[test]
fn deletes_a_recording() {
    let store = TempStore::open();
    let meta = store.put(b"audio", "x", &a_voiceprint()).unwrap();
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
