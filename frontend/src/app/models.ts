/**
 * Wire types, re-exported from the ts-rs output.
 *
 * Import from here, never from `generated/` directly: the generated directory is
 * deleted and rewritten by `scripts/gen-types.sh`, and this barrel is the one
 * place that has to change if a type is renamed.
 */
export type { CalibrationStep } from "./generated/CalibrationStep";
export type { Controls } from "./generated/Controls";
export type { Corner } from "./generated/Corner";
export type { Deleted } from "./generated/Deleted";
export type { ErrorBody } from "./generated/ErrorBody";
export type { ErrorCode } from "./generated/ErrorCode";
export type { Events } from "./generated/Events";
export type { Formants } from "./generated/Formants";
export type { FrameGrid } from "./generated/FrameGrid";
export type { Knob } from "./generated/Knob";
export type { Mapping } from "./generated/Mapping";
export type { MappingChoice } from "./generated/MappingChoice";
export type { Material } from "./generated/Material";
export type { Partial } from "./generated/Partial";
export type { Partials } from "./generated/Partials";
export type { Pitch } from "./generated/Pitch";
export type { RecordingDetail } from "./generated/RecordingDetail";
export type { RecordingMeta } from "./generated/RecordingMeta";
export type { Role } from "./generated/Role";
export type { ScaleDegree } from "./generated/ScaleDegree";
export type { ScoreView } from "./generated/ScoreView";
export type { SpeakerCorner } from "./generated/SpeakerCorner";
export type { Source } from "./generated/Source";
export type { SpeakerCorners } from "./generated/SpeakerCorners";
export type { TelemetryEvent } from "./generated/TelemetryEvent";
export type { Texture } from "./generated/Texture";
export type { Voiceprint } from "./generated/Voiceprint";
export type { VoiceSummary } from "./generated/VoiceSummary";
