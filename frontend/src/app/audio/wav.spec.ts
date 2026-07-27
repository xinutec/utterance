import { describe, expect, it } from "vitest";

import { concatBlocks, encodeWav } from "./wav";

/** Read the encoded blob back as a DataView so the bytes can be asserted on. */
async function bytes(blob: Blob): Promise<DataView> {
  return new DataView(await blob.arrayBuffer());
}

const ascii = (view: DataView, offset: number, length: number): string =>
  Array.from({ length }, (_, i) => String.fromCharCode(view.getUint8(offset + i))).join("");

describe("encodeWav", () => {
  it("writes a header the backend decoder recognises", async () => {
    const view = await bytes(encodeWav(new Float32Array([0, 0.5, -0.5]), 48000));

    expect(ascii(view, 0, 4)).toBe("RIFF");
    expect(ascii(view, 8, 4)).toBe("WAVE");
    expect(ascii(view, 12, 4)).toBe("fmt ");
    expect(ascii(view, 36, 4)).toBe("data");
    expect(view.getUint16(20, true)).toBe(1); // uncompressed PCM
    expect(view.getUint16(22, true)).toBe(1); // mono
    expect(view.getUint32(24, true)).toBe(48000);
    expect(view.getUint16(34, true)).toBe(16); // bits per sample
  });

  it("declares sizes that match the payload", async () => {
    const view = await bytes(encodeWav(new Float32Array(100), 16000));

    // A wrong size here is the classic WAV bug: players and decoders either
    // truncate the audio or read past the end, and neither reports why.
    expect(view.getUint32(40, true)).toBe(200); // data chunk: 100 samples * 2 bytes
    expect(view.getUint32(4, true)).toBe(view.byteLength - 8);
    expect(view.byteLength).toBe(44 + 200);
  });

  it("derives byte rate and block align from the format", async () => {
    const view = await bytes(encodeWav(new Float32Array(10), 44100));

    expect(view.getUint32(28, true)).toBe(44100 * 2);
    expect(view.getUint16(32, true)).toBe(2);
  });

  it("round-trips sample values", async () => {
    const view = await bytes(encodeWav(new Float32Array([0, 1, -1, 0.5]), 16000));

    expect(view.getInt16(44, true)).toBe(0);
    expect(view.getInt16(46, true)).toBe(32767);
    expect(view.getInt16(48, true)).toBe(-32767);
    expect(view.getInt16(50, true)).toBe(Math.round(0.5 * 32767));
  });

  it("clamps out-of-range samples instead of wrapping them", async () => {
    // Wrapping would turn a clipped peak into a full-scale sample of the
    // opposite sign — an impulse the analyser would read as an onset.
    const view = await bytes(encodeWav(new Float32Array([2, -2]), 16000));

    expect(view.getInt16(44, true)).toBe(32767);
    expect(view.getInt16(46, true)).toBe(-32767);
  });

  it("encodes an empty recording without producing a malformed file", async () => {
    const view = await bytes(encodeWav(new Float32Array(0), 16000));

    expect(view.byteLength).toBe(44);
    expect(view.getUint32(40, true)).toBe(0);
  });
});

describe("concatBlocks", () => {
  it("joins captured blocks in order", () => {
    const joined = concatBlocks([new Float32Array([1, 2]), new Float32Array([3]), new Float32Array([4, 5])]);

    expect(Array.from(joined)).toEqual([1, 2, 3, 4, 5]);
  });

  it("handles no blocks at all", () => {
    expect(concatBlocks([]).length).toBe(0);
  });
});
