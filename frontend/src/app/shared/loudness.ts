/**
 * Integrated loudness, ITU-R BS.1770: how loud a whole piece sounds, in LUFS.
 *
 * Not RMS. Hearing weights treble up and deep bass down, so two renders at the
 * same RMS can differ audibly — exactly when one has more high noise in it, as
 * breath does. K-weighting models that; gating keeps silence out of the mean.
 */

/** One biquad's coefficients, normalised so a0 is 1. */
interface Biquad {
  b: [number, number, number];
  a: [number, number];
}

type Three = [number, number, number];

function normalised([b0, b1, b2]: Three, [a0, a1, a2]: Three): Biquad {
  return { b: [b0 / a0, b1 / a0, b2 / a0], a: [a1 / a0, a2 / a0] };
}

/**
 * The K-weighting pair at any sample rate: a +4 dB shelf above 1.5 kHz (the
 * head's acoustic effect) and a high-pass at 38 Hz. The standard tabulates them
 * at 48 kHz only; these are the same filters designed for `rate`.
 */
function kWeighting(rate: number): Biquad[] {
  const shelf = (() => {
    const gain = 10 ** (4 / 40);
    const w0 = (2 * Math.PI * 1500) / rate;
    const alpha = Math.sin(w0) / (2 * Math.SQRT1_2);
    const cos = Math.cos(w0);
    const root = 2 * Math.sqrt(gain) * alpha;
    return normalised(
      [
        gain * (gain + 1 + (gain - 1) * cos + root),
        -2 * gain * (gain - 1 + (gain + 1) * cos),
        gain * (gain + 1 + (gain - 1) * cos - root),
      ],
      [gain + 1 - (gain - 1) * cos + root, 2 * (gain - 1 - (gain + 1) * cos), gain + 1 - (gain - 1) * cos - root],
    );
  })();
  const highPass = (() => {
    const w0 = (2 * Math.PI * 38) / rate;
    const alpha = Math.sin(w0) / (2 * 0.5);
    const cos = Math.cos(w0);
    return normalised([(1 + cos) / 2, -(1 + cos), (1 + cos) / 2], [1 + alpha, -2 * cos, 1 - alpha]);
  })();
  return [shelf, highPass];
}

function filter(x: Float32Array, { b: [b0, b1, b2], a: [a1, a2] }: Biquad): Float32Array {
  const y = new Float32Array(x.length);
  let [x1, x2, y1, y2] = [0, 0, 0, 0];
  for (const [i, x0] of x.entries()) {
    const out = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
    [x2, x1, y2, y1] = [x1, x0, y1, out];
    y[i] = out;
  }
  return y;
}

/** Blocks of 400 ms, overlapping by 75%. */
const BLOCK_S = 0.4;
const STEP_S = 0.1;
/** Blocks quieter than this are silence, whatever the piece. */
const ABSOLUTE_GATE_LUFS = -70;
/** Blocks this far below the piece's own level are pauses, not the piece. */
const RELATIVE_GATE_LU = 10;

const lufs = (meanSquare: number): number => -0.691 + 10 * Math.log10(meanSquare);

/**
 * Integrated loudness of `channels` (front left/right/centre, each weighted 1)
 * at `rate`, or `null` where nothing rises above the silence gate.
 */
export function integratedLoudness(channels: readonly Float32Array[], rate: number): number | null {
  const weighted = channels.map((c) => kWeighting(rate).reduce(filter, c));
  const length = Math.min(...weighted.map((c) => c.length));
  const block = Math.round(BLOCK_S * rate);
  const step = Math.round(STEP_S * rate);

  // Per block, the mean square summed across channels.
  const blocks: number[] = [];
  for (let start = 0; start + block <= length; start += step) {
    let power = 0;
    for (const c of weighted) {
      let sum = 0;
      for (const v of c.subarray(start, start + block)) sum += v * v;
      power += sum / block;
    }
    blocks.push(power);
  }

  const loud = blocks.filter((p) => p > 0 && lufs(p) > ABSOLUTE_GATE_LUFS);
  if (loud.length === 0) return null;
  const mean = (xs: number[]): number => xs.reduce((s, x) => s + x, 0) / xs.length;
  const gate = lufs(mean(loud)) - RELATIVE_GATE_LU;
  const kept = loud.filter((p) => lufs(p) > gate);
  return lufs(mean(kept));
}

/**
 * Volumes that make two pieces equally loud by turning the louder one down —
 * never up, so matching cannot make anything louder than it was rendered.
 * Unknown loudness on either side leaves both alone.
 */
export function matchingVolumes(a: number | null, b: number | null): { a: number; b: number } {
  if (a === null || b === null) return { a: 1, b: 1 };
  const down = (db: number): number => Math.min(1, 10 ** (db / 20));
  return { a: down(b - a), b: down(a - b) };
}
