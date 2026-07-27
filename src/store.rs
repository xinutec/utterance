//! Filesystem-backed recording store.
//!
//! One directory per recording, holding the original audio, its voiceprint and a
//! little metadata. Plain files rather than a database because the interesting
//! artefacts here are documents we want to read, diff and copy into fixtures by
//! hand — and because a recording plus its voiceprint is self-contained, so a
//! directory *is* the record.
//!
//! Deleting `data/` is a supported way to start over.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use music_analysis::voiceprint::Voiceprint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What we know about a recording without opening its voiceprint.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecordingMeta {
    /// Content-addressed: the first 16 hex digits of the audio's SHA-256.
    ///
    /// Uploading the same audio twice therefore lands on the same recording
    /// rather than accumulating duplicates — useful while iterating, where the
    /// same take gets re-sent often.
    pub id: String,
    /// Human label, as given at upload.
    pub label: String,
    /// Unix milliseconds when the recording was first stored.
    ///
    /// Typed as a TS `number`, not the `bigint` ts-rs infers from `u64`:
    /// `JSON.parse` produces a number, so `bigint` would be a type the runtime
    /// never actually delivers. Unix milliseconds stay inside JavaScript's
    /// safe-integer range until the year 287396.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at_ms: u64,
    pub duration_s: f32,
    pub sample_rate_hz: u32,
    /// Fraction of frames carrying a fundamental — the quickest signal of
    /// whether a take is usable.
    pub voiced_fraction: f32,
    pub onset_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("recording not found: {0}")]
    NotFound(String),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("corrupt record {id}: {detail}")]
    Corrupt { id: String, detail: String },
}

/// Reads and writes recordings under a root directory.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Open (creating if absent) a store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|source| StoreError::Io {
            path: root.clone(),
            source,
        })?;
        Ok(Self { root })
    }

    /// Store audio and its voiceprint, returning the metadata.
    ///
    /// Writing an existing id overwrites it, which is a no-op in practice: the
    /// id is derived from the audio bytes, so the same id means the same audio,
    /// and re-analysing it is deterministic.
    pub fn put(
        &self,
        audio: &[u8],
        label: &str,
        voiceprint: &Voiceprint,
    ) -> Result<RecordingMeta, StoreError> {
        let id = content_id(audio);
        let dir = self.dir(&id);
        fs::create_dir_all(&dir).map_err(|source| StoreError::Io {
            path: dir.clone(),
            source,
        })?;

        let meta = RecordingMeta {
            created_at_ms: now_ms(),
            label: if label.trim().is_empty() {
                id.clone()
            } else {
                label.trim().to_string()
            },
            id: id.clone(),
            duration_s: voiceprint.source.duration_s,
            sample_rate_hz: voiceprint.source.sample_rate_hz,
            voiced_fraction: voiceprint.pitch.voiced_fraction(),
            onset_count: voiceprint.events.onset_frames.len(),
        };

        write(&dir.join(AUDIO), audio)?;
        write_json(&dir.join(VOICEPRINT), voiceprint)?;
        write_json(&dir.join(META), &meta)?;
        Ok(meta)
    }

    /// Every stored recording, newest first.
    ///
    /// A directory that fails to parse is skipped rather than failing the whole
    /// listing: one bad record should not make the app unusable, and the
    /// alternative is a UI that shows nothing and explains nothing.
    pub fn list(&self) -> Result<Vec<RecordingMeta>, StoreError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(StoreError::Io {
                    path: self.root.clone(),
                    source,
                });
            }
        };

        let mut out: Vec<RecordingMeta> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let id = e.file_name().to_string_lossy().into_owned();
                match self.meta(&id) {
                    Ok(m) => Some(m),
                    Err(err) => {
                        tracing::warn!("skipping unreadable recording {id}: {err}");
                        None
                    }
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.created_at_ms
                .cmp(&a.created_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }

    pub fn meta(&self, id: &str) -> Result<RecordingMeta, StoreError> {
        self.read_json(id, META)
    }

    pub fn voiceprint(&self, id: &str) -> Result<Voiceprint, StoreError> {
        self.read_json(id, VOICEPRINT)
    }

    pub fn audio(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.checked_dir(id)?.join(AUDIO);
        fs::read(&path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => StoreError::NotFound(id.to_string()),
            _ => StoreError::Io { path, source },
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let dir = self.checked_dir(id)?;
        fs::remove_dir_all(&dir).map_err(|source| StoreError::Io { path: dir, source })
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Resolve a recording directory, rejecting ids that are not ours.
    ///
    /// Ids reach this from the URL path. Validating the *shape* rather than
    /// sanitising the string means `../` and absolute paths are rejected as
    /// unknown recordings, which is both safer and the honest answer.
    fn checked_dir(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let dir = self.dir(id);
        if !dir.is_dir() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        Ok(dir)
    }

    fn read_json<T: for<'de> Deserialize<'de>>(
        &self,
        id: &str,
        name: &str,
    ) -> Result<T, StoreError> {
        let path = self.checked_dir(id)?.join(name);
        let bytes = fs::read(&path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => StoreError::NotFound(id.to_string()),
            _ => StoreError::Io {
                path: path.clone(),
                source,
            },
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
            id: id.to_string(),
            detail: format!("{name}: {e}"),
        })
    }
}

const AUDIO: &str = "audio.wav";
const VOICEPRINT: &str = "voiceprint.json";
const META: &str = "meta.json";

/// Length of the hex id taken from the content hash.
///
/// 16 hex digits is 64 bits. At any collection size a person will ever record by
/// hand, an accidental collision is far less likely than the disk losing the file.
const ID_LEN: usize = 16;

fn content_id(audio: &[u8]) -> String {
    let digest = Sha256::digest(audio);
    digest
        .iter()
        .take(ID_LEN / 2)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn is_valid_id(id: &str) -> bool {
    id.len() == ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    fs::write(path, bytes).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| StoreError::Corrupt {
        id: path.display().to_string(),
        detail: e.to_string(),
    })?;
    write(path, &bytes)
}
