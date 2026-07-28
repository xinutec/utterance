//! The voice as a set of per-frame streams, before anything musical is decided.
//!
//! Every continuously-sounding mapping reads the same eight things — f0, vowel
//! frontness, vowel openness, F3, spectral flux, energy, brightness,
//! aperiodicity — and every one of them needs the same two treatments: gaps
//! carried across rather than filled with a middle value, and smoothing at the
//! timescale that stream belongs to.
//!
//! **Why a module rather than a copy in each mapping.** The rules for reading a
//! voice are not aesthetic. That an unvoiced frame is a frame with no
//! measurement rather than a frame at zero hertz is true whatever the mapping
//! does with it, and a second mapping that got it wrong would not be a different
//! aesthetic — it would be a bug that sounds like one. Sharing them means a
//! mapping chooses what to *do* with a stream and never how to read it.

use utterance_analysis::voiceprint::Voiceprint;

use crate::voice::Voice;

/// Frames the pitch drift is averaged over.
///
/// Two seconds. Long enough to cross several syllables, so what survives is the
/// phrase-level declination rather than the pitch of any particular word. This
/// is the slow timescale the note mapping had no way to express.
pub const DRIFT_FRAMES: usize = 200;

/// Frames the articulation streams are averaged over.
///
/// A fifth of a second — about one syllable. Short enough that the harmony
/// follows the articulation, long enough that a single misfit formant frame
/// cannot jolt the whole field.
pub const ROOT_FRAMES: usize = 20;

/// Frames loudness is averaged over.
///
/// 80 ms. Fast enough to keep the speaker's dynamics, slow enough that the field
/// does not flutter at the syllable rate.
pub const LEVEL_FRAMES: usize = 8;

/// Vowel position per frame, carried across frames with no estimate.
///
/// Returns openness and frontness, each 0..1 in the speaker's own space.
///
/// A held vowel is still that vowel while a consonant interrupts it, so a gap in
/// the formant track is missing information rather than a jump to the middle of
/// the space. Carrying the last known position forward keeps the field moving
/// the way the mouth moved; interpolating to a default would make every
/// consonant a lurch toward the centre.
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

/// Mouth shape per frame, from the third formant, carried across gaps.
///
/// Held at the middle of the speaker's range where F3 was never measured well
/// enough to have one. Unlike the colour, the middle here is not a stand-in for
/// a measurement: it is the position at which this stream does nothing, so an
/// unmeasured F3 leaves the chord exactly as the other streams built it.
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

/// Tone colour per frame, from the measured spectral centroid.
///
/// Voiced frames only, carried across the gaps for the same reason the vowel
/// track is: a consonant is far brighter than any tone a throat sustains, and
/// letting one through would flick the whole field white at every *s*. The
/// consonants are already sounded as themselves, by the noise layer.
///
/// Without a measured brightness range the colour holds still. That is an
/// absence of information rather than a fallback: the alternative — driving it
/// from some other stream — is exactly the thing this function exists to undo.
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

/// Pitch per frame with unvoiced gaps carried across.
///
/// Same reasoning as the vowel track: an unvoiced frame is a frame with no
/// measurement, not a frame at zero hertz, and zero would drag the drift down
/// every time he pronounced a consonant.
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

/// The energy envelope as a linear 0..1, relative to the take's loudest moment.
pub fn level(vp: &Voiceprint) -> Vec<f32> {
    let loudest = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    vp.rms_db
        .iter()
        .map(|db| 10f32.powf((db - loudest) / 20.0).clamp(0.0, 1.0))
        .collect()
}

/// Breath fraction at one frame, from how periodic the voice was there.
pub fn breath_at(vp: &Voiceprint, i: usize) -> f32 {
    let aperiodicity = vp.pitch.aperiodicity.get(i).copied().unwrap_or_default();
    (aperiodicity / 0.6).clamp(0.0, 1.0) * 0.3
}

/// Centred moving average over `window` frames.
///
/// Centred rather than trailing so the field moves *with* the voice rather than
/// lagging it by half the window, which at the drift timescale would be a second
/// of delay and audible as the music answering rather than accompanying.
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
