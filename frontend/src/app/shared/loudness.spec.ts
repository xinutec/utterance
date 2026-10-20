import { describe, expect, it } from "vitest";

import { integratedLoudness, matchingVolumes } from "./loudness";

const RATE = 44_100;

function sine(hz: number, amplitude: number, seconds = 3): Float32Array {
  return Float32Array.from({ length: RATE * seconds }, (_, i) =>
    amplitude * Math.sin((2 * Math.PI * hz * i) / RATE),
  );
}

describe("integratedLoudness", () => {
  it("reads a full-scale 997 Hz sine in one channel at -3.01 LUFS, the standard's own calibration", () => {
    expect(integratedLoudness([sine(997, 1)], RATE)).toBeCloseTo(-3.01, 1);
  });

  it("follows level: half the amplitude is 6 dB quieter", () => {
    const full = integratedLoudness([sine(997, 1)], RATE)!;
    const half = integratedLoudness([sine(997, 0.5)], RATE)!;
    expect(full - half).toBeCloseTo(6.02, 1);
  });

  it("weights treble up and deep bass down, as hearing does", () => {
    const mid = integratedLoudness([sine(997, 0.5)], RATE)!;
    expect(integratedLoudness([sine(4000, 0.5)], RATE)!).toBeGreaterThan(mid + 2);
    expect(integratedLoudness([sine(30, 0.5)], RATE)!).toBeLessThan(mid - 2);
  });

  it("ignores silence rather than averaging it in", () => {
    const tone = sine(997, 0.5, 2);
    // A room's hiss, not digital zero, so only the gate can leave it out.
    const padded = sine(997, 1e-5, 6);
    padded.set(tone, RATE * 2);
    // Averaged in, the four silent seconds would cost 4.8 dB. Blocks straddling
    // the tone's edges still pass the gate, so a little is lost.
    const lost = integratedLoudness([tone], RATE)! - integratedLoudness([padded], RATE)!;
    expect(lost).toBeLessThan(1);
  });

  it("has no loudness for silence", () => {
    expect(integratedLoudness([new Float32Array(RATE * 2)], RATE)).toBeNull();
  });
});

describe("matchingVolumes", () => {
  it("turns the louder side down by the difference", () => {
    const v = matchingVolumes(-20, -18);
    expect(v.a).toBe(1);
    expect(v.b).toBeCloseTo(10 ** (-2 / 20), 6);
  });

  it("never turns a side up", () => {
    const v = matchingVolumes(-30, -10);
    expect(Math.max(v.a, v.b)).toBe(1);
  });

  it("leaves both alone when either loudness is unknown", () => {
    expect(matchingVolumes(null, -18)).toEqual({ a: 1, b: 1 });
  });
});
