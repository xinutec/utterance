/**
 * Minimal WAV writing, so captured audio reaches the backend losslessly.
 *
 * The backend's analyser takes WAV and nothing else — a deliberately narrow door
 * (see `utterance-analysis/src/wav.rs`). This is the other side of it.
 */

/** Bytes in the canonical WAV header this writer emits. */
const HEADER_BYTES = 44;

/** Full-scale value for signed 16-bit PCM. */
const INT16_MAX = 32767;

/**
 * Encode mono float samples as a 16-bit PCM WAV file.
 *
 * 16-bit rather than 32-bit float: it halves the upload, and the quantisation
 * floor sits around -96 dBFS, far below the noise floor of any microphone in a
 * room. Samples are clamped, not scaled to fit — normalising here would make the
 * energy envelope depend on the take's loudest moment.
 */
export function encodeWav(samples: Float32Array, sampleRate: number): Blob {
  const buffer = new ArrayBuffer(HEADER_BYTES + samples.length * 2);
  const view = new DataView(buffer);

  const ascii = (offset: number, text: string): void => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };

  const dataBytes = samples.length * 2;
  ascii(0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true); // size of everything after this field
  ascii(8, "WAVE");
  ascii(12, "fmt ");
  view.setUint32(16, 16, true); // fmt chunk size
  view.setUint16(20, 1, true); // format 1 = uncompressed PCM
  view.setUint16(22, 1, true); // channels
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true); // byte rate: rate * channels * bytes
  view.setUint16(32, 2, true); // block align: channels * bytes per sample
  view.setUint16(34, 16, true); // bits per sample
  ascii(36, "data");
  view.setUint32(40, dataBytes, true);

  for (const [i, sample] of samples.entries()) {
    const clamped = Math.max(-1, Math.min(1, sample));
    view.setInt16(HEADER_BYTES + i * 2, Math.round(clamped * INT16_MAX), true);
  }

  return new Blob([buffer], { type: "audio/wav" });
}

/** Concatenate the captured blocks into one contiguous buffer. */
export function concatBlocks(blocks: readonly Float32Array[]): Float32Array {
  const total = blocks.reduce((n, b) => n + b.length, 0);
  const out = new Float32Array(total);
  let at = 0;
  for (const block of blocks) {
    out.set(block, at);
    at += block.length;
  }
  return out;
}
