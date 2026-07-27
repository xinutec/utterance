// Audio worklet that forwards raw microphone blocks to the main thread.
//
// A worklet rather than MediaRecorder because MediaRecorder encodes to Opus,
// and every artefact this project measures — harmonic structure, formant
// positions, the exact shape of a pitch glide — is precisely what a perceptual
// codec is designed to throw away. This path stays at the full float precision
// the input device produced.
//
// Lives in public/ because a worklet is loaded by URL at runtime
// (audioWorklet.addModule), not imported by the bundler.
class CaptureProcessor extends AudioWorkletProcessor {
  process(inputs) {
    const input = inputs[0];
    if (!input || input.length === 0) {
      // No input connected yet. Returning true keeps the node alive.
      return true;
    }

    // Channel 0 only. The capture graph requests a mono stream; if the device
    // insists on more, the extra channels are the same signal.
    const channel = input[0];
    if (channel && channel.length > 0) {
      // Copy before posting: the render quantum buffer is reused by the audio
      // thread on the very next callback, so a transferred view would be
      // overwritten before the main thread ever read it.
      this.port.postMessage(new Float32Array(channel));
    }
    return true;
  }
}

registerProcessor("capture-processor", CaptureProcessor);
