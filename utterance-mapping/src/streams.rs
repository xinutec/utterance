//! The voice as per-frame streams, before anything musical is decided: gaps
//! carried across, smoothing at each stream's timescale. Shared, because how to
//! read a voice is not aesthetic — an unvoiced frame has no measurement, not a
//! measurement of zero — so a mapping only chooses what to do with a stream.

use utterance_analysis::voiceprint::Voiceprint;

use crate::voice::Voice;

/// Frames the pitch drift is averaged over: two seconds, so what survives is
/// phrase-level declination rather than any word's pitch.
pub const DRIFT_FRAMES: usize = 200;

/// Frames the articulation streams are averaged over: about a syllable, so the
/// harmony follows articulation but one misfit formant frame cannot jolt it.
pub const ROOT_FRAMES: usize = 20;

/// Frames loudness is averaged over: 80 ms keeps the dynamics without
/// fluttering at the syllable rate.
pub const LEVEL_FRAMES: usize = 8;

/// Vowel position per frame as (openness, frontness), 0..1 in the speaker's
/// space. A gap carries the last position forward: a consonant does not move the
/// vowel to the middle of the chart.
pub fn vowel(vp: &Voiceprint, voice: &Voice) -> (Vec<f32>, Vec<f32>) {
    let mut open = vec![0.5f32; vp.frame.count];
    let mut front = vec![0.5f32; vp.frame.count];
    let (mut last_open, mut last_front) = (0.5f32, 0.5f32);

    for i in 0..vp.frame.count {
        if let (Some(Some(f1)), Some(Some(f2))) = (vp.formants.f1.get(i), vp.formants.f2.get(i)) {
            let (o, f) = voice.space.normalise(*f1, *f2);
            last_open = o.clamp(0.0, 1.0);
            last_front = f.clamp(0.0, 1.0);
        }
        open[i] = last_open;
        front[i] = last_front;
    }
    (open, front)
}

/// Mouth shape per frame, from F3, carried across gaps. Where F3 has no range the
/// stream sits at its middle, where it changes nothing.
pub fn depth(vp: &Voiceprint, voice: &Voice) -> Vec<f32> {
    let mut last = 0.5f32;
    (0..vp.frame.count)
        .map(|i| {
            if let Some(Some(f3)) = vp.formants.f3.get(i)
                && let Some(placed) = voice.space.depth(*f3)
            {
                last = placed;
            }
            last
        })
        .collect()
}

/// Tone colour per frame, from the spectral centroid of voiced frames only,
/// carried across gaps — a consonant would flash the field white. Without a
/// brightness range it holds still rather than borrowing another stream.
pub fn brightness(vp: &Voiceprint, voice: &Voice) -> Vec<f32> {
    let Some(range) = voice.brightness else {
        return vec![0.5; vp.frame.count];
    };

    let mut last = 0.5f32;
    (0..vp.frame.count)
        .map(|i| {
            let voiced = vp.pitch.hz.get(i).copied().flatten().is_some();
            if let (true, Some(centroid)) = (voiced, vp.texture.centroid_hz.get(i)) {
                last = range.place(*centroid);
            }
            last
        })
        .collect()
}

/// Pitch per frame with unvoiced gaps carried across, so consonants do not drag
/// the drift to zero.
pub fn filled(hz: &[Option<f32>]) -> Vec<f32> {
    let first = hz.iter().flatten().copied().next().unwrap_or(1.0);
    let mut last = first;
    hz.iter()
        .map(|h| {
            if let Some(v) = *h {
                last = v;
            }
            last
        })
        .collect()
}

/// The take's loudest frame, in dBFS.
pub fn loudest_db(vp: &Voiceprint) -> f32 {
    vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

/// A level in dBFS as a linear amplitude relative to `loudest_db` — relative, so
/// a quiet recording has the same dynamics as a loud one.
pub fn relative_amplitude(db: f32, loudest_db: f32) -> f32 {
    10f32.powf((db - loudest_db) / 20.0)
}

/// The energy envelope as a linear 0..1, relative to the take's loudest moment.
pub fn level(vp: &Voiceprint) -> Vec<f32> {
    let loudest = loudest_db(vp);
    vp.rms_db
        .iter()
        .map(|&db| relative_amplitude(db, loudest).clamp(0.0, 1.0))
        .collect()
}

/// Breath fraction at one frame, from how periodic the voice was there.
pub fn breath_at(vp: &Voiceprint, i: usize) -> f32 {
    let aperiodicity = vp.pitch.aperiodicity.get(i).copied().unwrap_or_default();
    (aperiodicity / 0.6).clamp(0.0, 1.0) * 0.3
}

/// Centred moving average over `window` frames: trailing would make the music
/// answer the voice rather than accompany it.
pub fn smooth(values: &[f32], window: usize) -> Vec<f32> {
    if values.is_empty() || window <= 1 {
        return values.to_vec();
    }
    let half = window / 2;
    (0..values.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(values.len());
            values[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
        })
        .collect()
}
