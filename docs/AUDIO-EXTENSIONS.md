# Embedded animation audio

OrbitPal supports **`orbitpal-msft-audio-v1`**, an explicit subset of the **draft** [MSFT_audio_emitter specification at commit 8109e86a26c9210814f8d236589ce92a2f9fa8f8](https://github.com/najadojo/glTF/blob/8109e86a26c9210814f8d236589ce92a2f9fa8f8/extensions/2.0/Vendor/MSFT_audio_emitter/README.md), authored by Microsoft contributors. This is a pinned compatibility profile, not a ratified or universal glTF/VRM sound standard. The [Khronos registry](https://github.com/KhronosGroup/glTF/blob/main/extensions/README.md) does not make every audio proposal interchangeable.

The selected draft defines clips/emitters and animation events. Its JSON has **no version field**. The profile is versioned by OrbitPal and tied to the exact source revision above; unexpected `version`/`specVersion` fields or unknown semantic fields are rejected rather than guessed. No claim is made about other revisions, `KHR_audio_emitter`, `KHR_audio_graph`, MPEG media, VRMA audio or another vendor's format.

## Supported subset

| Data | Profile v1 |
| --- | --- |
| Audio encoding | RIFF PCM16 WAV, 1–2 channels, 8–48 kHz, at most 15 seconds |
| Resource | Canonical `data:audio/wav;base64,…` or an uncompressed embedded `bufferView` with `mimeType: audio/wav` |
| Emitter | One clip reference, weight omitted/1, volume 0–1, loop omitted/false |
| Binding | Explicit events on the original embedded animation index; time 0–120 seconds in nondecreasing order |
| Actions | `play` and `stop`; optional play `startOffset` 0–15 seconds |
| Presentation | Emotes and explicitly activated Autopilot walking/gestures, after their frame is rendered |
| Limits | 8 clips, 4 emitters, 128 events per model; 5 MiB per clip, 16 MiB declared audio total; existing 256 MiB model/8 MiB JSON limits remain |

Spatial/node and scene/ambient bindings, clip randomization, emitter loops, pause/resume and animated audio properties are outside this profile. Automatic idle/load sounds are deliberately silent. Extra user metadata in `extras` is inert. A required extension outside the subset fails with an explanation; optional unsupported audio remains muted with a warning. A malformed WAV/decode failure also surfaces explicitly.

The native inspector validates the layout, indices, embedded-resource policy and byte/count limits during import and again when imported models are served. WAV structure, PCM rate/channel/frame budgets and exact resolved clip byte counts are checked before WebAudio decoding. Existing sound playback additionally bounds the decoded cache to 16 MiB. Neither stage resolves an external audio URL, local sidecar or filesystem path from the extension. JSON-glTF import may still bundle its already supported relative BIN/texture resources; this change does not add arbitrary audio sidecar imports.

## Activation, routing and cancellation

The live Studio stage and the selected native overlay/OBS surface are the eligible outputs. Character-library, motion and Autopilot previews default to silence. Switching output disables the previous surface's model-audio controller. No audio starts from model loading, metadata inspection or an automatic idle clip.

Click or press a key in the eligible output to activate audio. Activation authorizes future events; an event skipped while locked is not queued or replayed after unlocking. OBS browser-source interaction may be needed to deliver that click. An output with browser-blocked WebAudio reports a notice and remains silent.

The model controller reuses the existing emote-audio player with a separate, model-scoped cache and internal emitter IDs. Model replacement, Stop, profile/output changes and component disposal invalidate pending preparation and stop its sources. A cancelled gesture cannot restart from a late resource/decode result. A new explicit gesture can reuse already prepared clips from the same model. Shared clips are decoded once while emitters retain independent stop identities. Model decoding has a five-second deadline per clip; an underlying browser decode cannot be forcibly interrupted, but a late result is discarded and cannot start playback.

Events follow actual emote/walking clip time **after rendering**, including the clip's effective presentation rate. Play offsets account for a small skipped interval; stale events more than 250 ms late are dropped. Joining a clip midway does not replay earlier events. Walking wraps stop the previous cycle's sources before the current cycle's events. Emote replacement/end and walking stop halt its sounds. Rates outside 0.25–4 are muted rather than played out of sync.

The existing voice-volume slider controls the model-audio master volume; emitter volume multiplies it. Zero volume immediately silences/stops sources. Optional separately assigned emote effects remain independent and may intentionally accompany the same gesture. Actual sound activity feeds the existing Studio block and overlay/OBS emote-playback heartbeat, rather than reporting idle/loading as playback.

## Fixture evidence and limits

`src/lib/fixtures/audio-draft-v1.glb` is an original, self-authored silent test fixture: one triangle, one translation animation and an embedded half-second PCM16 WAV. It is available under the repository MIT license and contains no downloaded character, recording or third-party audio. The native inspector and production Three.js GLTFParser both read it.

Local fixtures cover unsupported versions/features, malformed indices/URI/size/PCM data, native required-extension admission, silent load/previews, explicit activation, render-time Play/Stop, volume/rate/offset, loop/late-event behavior and cancellation/model replacement. Mock WebAudio verifies source starts and stops. These tests do not claim physical sound-device, live OBS, codec-driver or all-platform installation acceptance. None of the sixteen bundled character files declares an audio extension; this feature does not invent sounds for them.
