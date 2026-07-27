import { Injectable, signal } from "@angular/core";

import { concatBlocks, encodeWav } from "./wav";

/** A finished take, ready to upload. */
export interface Take {
  readonly wav: Blob;
  readonly durationS: number;
  readonly sampleRateHz: number;
  /** Highest absolute sample in the take, 0..1 — how close it came to clipping. */
  readonly peak: number;
}

/**
 * Microphone capture straight to WAV.
 *
 * Every piece of browser audio "help" is switched off on purpose. Echo
 * cancellation, noise suppression and automatic gain control all work by
 * modifying the spectrum and the level over time — which is the entire content
 * of a voiceprint. AGC alone would flatten the energy envelope that phrasing is
 * read from, and noise suppression eats the unvoiced consonants.
 */
@Injectable({ providedIn: "root" })
export class Recorder {
  /** Whether a capture is running. */
  readonly recording = signal(false);
  /** Seconds captured so far, updated as blocks arrive. */
  readonly elapsedS = signal(0);
  /** Live input level in 0..1, for a meter that shows the mic is actually live. */
  readonly level = signal(0);

  private context?: AudioContext;
  private stream?: MediaStream;
  private node?: AudioWorkletNode;
  private blocks: Float32Array[] = [];

  /** Whether this browser can capture at all — false over plain HTTP. */
  static get supported(): boolean {
    return typeof AudioWorkletNode !== "undefined" && navigator.mediaDevices !== undefined;
  }

  async start(): Promise<void> {
    if (this.recording()) return;

    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      },
    });

    // No sampleRate constraint: let the device run natively and record whatever
    // it gives us. Asking the browser to convert adds a resampler we did not
    // write and cannot inspect, and the backend normalises the rate anyway.
    const context = new AudioContext();
    await context.audioWorklet.addModule("capture-worklet.js");

    const node = new AudioWorkletNode(context, "capture-processor");
    node.port.onmessage = (event: MessageEvent<Float32Array>) => this.onBlock(event.data, context.sampleRate);

    context.createMediaStreamSource(this.stream).connect(node);
    // Worklets only pull input while connected to a destination. Nothing is
    // played: the processor returns no output, so the destination stays silent
    // and the microphone is never echoed back into the room.
    node.connect(context.destination);

    this.context = context;
    this.node = node;
    this.blocks = [];
    this.elapsedS.set(0);
    this.level.set(0);
    this.recording.set(true);
  }

  /** Stop capturing and return the take, or `null` if nothing was recorded. */
  async stop(): Promise<Take | null> {
    if (!this.recording()) return null;

    const sampleRateHz = this.context?.sampleRate ?? 48000;
    await this.teardown();

    const samples = concatBlocks(this.blocks);
    this.blocks = [];
    this.recording.set(false);
    this.level.set(0);
    if (samples.length === 0) return null;

    let peak = 0;
    for (const s of samples) peak = Math.max(peak, Math.abs(s));

    return {
      wav: encodeWav(samples, sampleRateHz),
      durationS: samples.length / sampleRateHz,
      sampleRateHz,
      peak,
    };
  }

  /** Abandon a capture without producing a take. */
  async cancel(): Promise<void> {
    await this.teardown();
    this.blocks = [];
    this.recording.set(false);
    this.elapsedS.set(0);
    this.level.set(0);
  }

  private onBlock(block: Float32Array, sampleRate: number): void {
    this.blocks.push(block);
    this.elapsedS.update((s) => s + block.length / sampleRate);

    let peak = 0;
    for (const s of block) peak = Math.max(peak, Math.abs(s));
    // Decay the meter rather than tracking the instantaneous peak, so speech
    // reads as a level rather than flickering with every glottal pulse.
    this.level.update((current) => Math.max(peak, current * 0.85));
  }

  private async teardown(): Promise<void> {
    if (this.node) {
      this.node.port.onmessage = null;
      this.node.disconnect();
      this.node = undefined;
    }
    this.stream?.getTracks().forEach((t) => {
      t.stop();
    });
    this.stream = undefined;
    await this.context?.close();
    this.context = undefined;
  }
}
