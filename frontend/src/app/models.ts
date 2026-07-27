/**
 * Wire types, re-exported from the ts-rs output.
 *
 * Import from here, never from `generated/` directly: the generated directory is
 * deleted and rewritten by `scripts/gen-types.sh`, and this barrel is the one
 * place that has to change if a type is renamed.
 */
export type { Deleted } from "./generated/Deleted";
export type { ErrorBody } from "./generated/ErrorBody";
export type { Events } from "./generated/Events";
export type { Formants } from "./generated/Formants";
export type { FrameGrid } from "./generated/FrameGrid";
export type { Partial } from "./generated/Partial";
export type { Partials } from "./generated/Partials";
export type { Pitch } from "./generated/Pitch";
export type { RecordingDetail } from "./generated/RecordingDetail";
export type { RecordingMeta } from "./generated/RecordingMeta";
export type { Source } from "./generated/Source";
export type { Voiceprint } from "./generated/Voiceprint";
