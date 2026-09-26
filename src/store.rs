//! Filesystem-backed recording store: one directory per recording, holding the
//! audio, its voiceprint and a little metadata — documents to read, diff and copy
//! into fixtures, which a database would hide. Deleting `data/` starts over.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utterance_analysis::voiceprint::{self, Voiceprint};

/// What a recording is *for* — not who owns it; there is one user. The store
/// holds other people's singing to render beside the speaker's own vowels, and
/// pooled together they would describe an anatomy belonging to nobody.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// This take defines the speaker: their scale, timbre, range and vowel space.
    Calibration,
    /// Something to render; it says nothing about who the speaker is. The
    /// default, so a take shapes the sound world only on purpose.
    #[default]
    Material,
}

/// What we know about a recording without opening its voiceprint.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecordingMeta {
    /// Content-addressed: the first 16 hex digits of the audio's SHA-256, so the
    /// same audio uploaded twice is one recording.
    pub id: String,
    /// Human label, as given at upload.
    pub label: String,
    /// Unix milliseconds when the recording was first stored. A TS `number`,
    /// not `bigint`: `JSON.parse` delivers a number, and these stay safe
    /// integers for millennia.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at_ms: u64,
    pub duration_s: f32,
    pub sample_rate_hz: u32,
    /// Fraction of frames carrying a fundamental — the quickest signal of
    /// whether a take is usable.
    pub voiced_fraction: f32,
    pub onset_count: usize,
    /// Highest absolute sample in the source, 0..1.
    pub peak: f32,
    /// Whether the take was driven into the rails — on the summary, so the take
    /// list can flag it.
    pub clipped: bool,
    /// Whether this take defines the speaker. Defaults to material when absent.
    #[serde(default)]
    pub role: Role,
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

    /// Store audio and its voiceprint, returning the metadata. The id comes from
    /// the audio, so overwriting an id rewrites the same record.
    pub fn put(
        &self,
        audio: &[u8],
        label: &str,
        voiceprint: &Voiceprint,
        role: Role,
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
            peak: voiceprint.source.peak,
            clipped: voiceprint.source.is_clipped(),
            role,
        };

        write(&dir.join(AUDIO), audio)?;
        write_json(&dir.join(VOICEPRINT), voiceprint)?;
        write_json(&dir.join(META), &meta)?;
        Ok(meta)
    }

    /// Bring a record up to the current analyser, re-analysing if it is stale.
    ///
    /// The audio is the source of truth, so a voiceprint is a cache that can
    /// always be rebuilt — which is what makes bumping
    /// [`voiceprint::SCHEMA_VERSION`] cheap. Defaulting missing fields instead
    /// would answer questions about old takes wrongly.
    pub fn ensure_current(&self, id: &str) -> Result<(), StoreError> {
        let dir = self.checked_dir(id)?;

        let current = matches!(
            self.read_json::<Written>(id, VOICEPRINT),
            Ok(w) if w.schema_version == voiceprint::SCHEMA_VERSION
        );
        if current && self.read_json::<RecordingMeta>(id, META).is_ok() {
            return Ok(());
        }

        let audio = self.audio(id)?;
        let voiceprint =
            utterance_analysis::analyse_wav(&audio).map_err(|e| StoreError::Corrupt {
                id: id.to_string(),
                detail: e.to_string(),
            })?;

        // Label and role are not in the audio, so they are carried across, read
        // loosely since the old metadata may be what failed to parse. Defaulting
        // the role would demote every calibration take on the next bump.
        let label = self.stored_label(id).unwrap_or_else(|| id.to_string());
        let role = self.stored_role(id).unwrap_or_default();
        tracing::info!(
            "re-analysing {id} for schema v{}",
            voiceprint::SCHEMA_VERSION
        );

        let meta = RecordingMeta {
            // Preserved so a rebuild does not reshuffle the take list.
            created_at_ms: self.stored_created_at(id).unwrap_or_else(now_ms),
            label,
            id: id.to_string(),
            duration_s: voiceprint.source.duration_s,
            sample_rate_hz: voiceprint.source.sample_rate_hz,
            voiced_fraction: voiceprint.pitch.voiced_fraction(),
            onset_count: voiceprint.events.onset_frames.len(),
            peak: voiceprint.source.peak,
            clipped: voiceprint.source.is_clipped(),
            role,
        };
        write_json(&dir.join(VOICEPRINT), &voiceprint)?;
        write_json(&dir.join(META), &meta)?;
        Ok(())
    }

    /// Say what an already-stored take is for. A take that came in as a file,
    /// or before roles existed, has no other way to become a calibration one.
    /// Only metadata changes, so no measurement can be invalidated.
    pub fn put_role(&self, id: &str, role: Role) -> Result<RecordingMeta, StoreError> {
        let dir = self.checked_dir(id)?;
        let mut meta = self.meta(id)?;
        meta.role = role;
        write_json(&dir.join(META), &meta)?;
        Ok(meta)
    }

    /// A single field from the stored metadata, whatever else is wrong with it.
    fn stored_field(&self, id: &str, key: &str) -> Option<serde_json::Value> {
        let bytes = fs::read(self.dir(id).join(META)).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        value.get(key).cloned()
    }

    fn stored_label(&self, id: &str) -> Option<String> {
        self.stored_field(id, "label")?.as_str().map(str::to_string)
    }

    fn stored_created_at(&self, id: &str) -> Option<u64> {
        self.stored_field(id, "createdAtMs")?.as_u64()
    }

    fn stored_role(&self, id: &str) -> Option<Role> {
        serde_json::from_value(self.stored_field(id, "role")?).ok()
    }

    /// Every stored recording, newest first. An unreadable directory is skipped,
    /// so one bad record cannot empty the list.
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
        self.ensure_current(id)?;
        self.read_json(id, META)
    }

    pub fn voiceprint(&self, id: &str) -> Result<Voiceprint, StoreError> {
        self.ensure_current(id)?;
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

    /// Resolve a recording directory. Ids come from the URL, so the *shape* is
    /// validated: `../` and absolute paths are simply unknown recordings.
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

/// Just enough of a stored voiceprint to know which analyser wrote it. Every
/// read checks this first, so serde skips the per-frame values rather than
/// building them.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Written {
    schema_version: u32,
}

const AUDIO: &str = "audio.wav";
const VOICEPRINT: &str = "voiceprint.json";
const META: &str = "meta.json";

/// Length of the hex id: 64 bits, so a collision is far less likely than losing
/// the disk.
const ID_LEN: usize = 16;

fn content_id(audio: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(audio);
    digest
        .iter()
        .take(ID_LEN / 2)
        .fold(String::with_capacity(ID_LEN), |mut id, b| {
            // Infallible: writing to a String cannot fail.
            let _ = write!(id, "{b:02x}");
            id
        })
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

/// Replace `path` in one step, or leave it as it was.
///
/// `fs::write` truncates and then writes, so a reader mid-write sees a corrupt
/// take and a crash leaves it corrupt. Write-then-rename gives a reader the whole
/// old file or the whole new one. The temp is a sibling because `rename` across
/// filesystems fails, and the recordings live on a volume mount.
///
/// ⚠ Atomic per file, not exclusive between writers: two processes editing the
/// same take can lose one update. Different takes never collide, which is what
/// makes a rolling deployment safe.
fn write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| StoreError::Io { path, source }
    };
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);

    fs::write(&tmp, bytes).map_err(io(&tmp))?;
    fs::rename(&tmp, path).map_err(|source| {
        // Best effort: the rename failure is the news.
        let _ = fs::remove_file(&tmp);
        StoreError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| StoreError::Corrupt {
        id: path.display().to_string(),
        detail: e.to_string(),
    })?;
    write(path, &bytes)
}
