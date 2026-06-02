# ALVR Optimization Plan — RTX 4080 Super / Quest 3 / Wired USB

_Generated 2026-06-02 from source-validated analysis (workflow `wf_0dcc9015-b1d`: 8 subsystem investigators → adversarial per-finding verification → hardware-weighted synthesis; 30 agents, 1.87M tokens)._

## Context

- **Base branch:** `diagnostics` (modern ALVR ~Apr 2025, fork of alvr-org/ALVR). The local tree was stranded on ancient v12.0.0 and has been moved here.
- **Target hardware:** Windows · RTX 4080 Super (Ada AD103, **two NVENC engines**, H264/HEVC/AV1) · Meta Quest 3 (XR2 Gen 2, decodes H264/HEVC/AV1, **no eye tracking**) · **wired USB** tether (effectively lossless).
- **Toolchain:** nightly pinned in-repo (`rustup override`), `rust-version=1.88` satisfied; `alvr_session` nightly-check passes. Full streamer/encoder build happens on Windows.
- **Two findings reshaped the original menu:** the *wired/lossless* link demotes all loss-recovery/FEC/probe items; *Quest 3 has no eye tracker* so gaze-contingent foveation is moot.

## Actionable roadmap (7 items, sequenced)

| # | Ticket | Item | Action | Effort | Impact |
|---|---|---|---|---|---|
| 20 | `ALVR-n20` | NVENC config dump + per-stage latency HUD (tooling) | implement | low | dev-velocity |
| 4 | `ALVR-cvb` | Capped-VBR rate-control preset (avg=max, VBV=0) | implement | low | quality |
| 7 | `ALVR-av1` | Default to AV1 10-bit (rig is known, no detection) | config-flip | low | quality |
| 15 | `ALVR-deb` | Chroma-upsample / debanding shader on decode pat | implement | low | quality |
| 16 | `ALVR-mci` | MediaCodec low-latency tuning (KEY_LOW_LATENCY,  | cherry-pick | trivial | latency |
| 13 | `ALVR-zcp` | Zero-copy capture->encode (register DXGI texture | cherry-pick | med | latency |
| 2 | `ALVR-sdk/ALVR-spl` | NVENC Split-Frame Encoding toggle (splitEncodeMo | cherry-pick | med | latency |
| 19 | `ALVR-pp2` | Improved pose prediction | implement | high | quality |

### Tier 1 — Instrument first
_Unblocks measurement; every later change becomes a struct/latency diff._

#### #20 — NVENC config trace + per-stage latency HUD (dev tooling enabler)  · `ALVR-n20`
- **Action:** cherry-pick · **Effort:** low · **Impact:** dev-velocity — Dev-velocity, high leverage as an enabler. The config dump is near-trivial and immediately turns 'is ALVR's NVENC config like VD's?' into a one-line struct diff
- **Now:** A per-stage latency HUD already exists in the dashboard (8 stages graphed with tooltips, backed by GraphStatistics). What is missing for true VD-parity tooling: (1) NO dump of the populated NV_ENC_INITIALIZE_PARAMS/NV_ENC_CONFIG/rcParams — only terse Debug strings, so comparing ALVR's encoder config against a VD captur
- **Patch:** (1) Config dump: at the end of FillEncodeConfig (and on Reconfigure), serialize the populated initializeParams + encodeConfig.rcParams to a single structured log line (or JSON) — preset GUID name, tuningInfo, frameRate, gopLength, refFrames, rcParams (mode/vbv/AQ/multiPass/lowDelayKeyFrameScale), enableWeightedPrediction, splitEncodeMode. Emit via the existing LogEncoder/Info binding so it lands in the session log an
- **Anchors:** `alvr/server_openvr/cpp/platform/win32/VideoEncoderNVENC.cpp:31, 70, 391`; `alvr/server_openvr/cpp/platform/win32/VideoEncoderNVENC.cpp:162-234, 375-416`; `alvr/dashboard/src/dashboard/components/statistics.rs:126-152`; `alvr/events/src/lib.rs:36-51`
- **Ref:** korejan/ALVR VideoEncoderNVENC.cpp (sets splitEncodeMode and is the reference for which params matter to dump)

### Tier 2 — Trivial high-value wins
_Small diffs, no protocol changes, immediate quality/latency._

#### #4 — Capped-VBR rate-control mode (VBR with averageBitRate=maxBitRate, tight VBV, zero reorder)  · `ALVR-cvb`
- **Action:** implement · **Effort:** low · **Impact:** quality — Small-to-moderate quality-per-bit gain on high-complexity frames (sharp text/edges in motion) without raising average bitrate. On a wired Quest3/4080 link the b
- **Now:** ALVR defaults to CBR. The encoder always sets averageBitRate == maxBitRate (so it is effectively already 'capped' in CBR too) and uses a tight ~1.1x single-frame VBV. VD's exact capped-VBR recipe (rcMode=VBR, averageBitRate=maxBitRate, vbvBufferSize=0, vbvInitialDelay=0) is reachable today only by manually setting the 
- **Patch:** Two options. (A) Cheap, no schema break: document the override recipe and/or add a preset (dashboard/components/settings_controls/presets) that sets the five nvenc rc_* overrides to the capped-VBR values. (B) First-class: add CappedVbr=2 to RateControlMode in settings.rs (line 93), and in VideoEncoderNVENC.cpp FillEncodeConfig (line 373-395) add a case that sets rateControlMode=NV_ENC_PARAMS_RC_VBR, averageBitRate=ma
- **Anchors:** `alvr/session/src/settings.rs:90-98`; `alvr/session/src/settings.rs:1739-1741`; `alvr/server_openvr/cpp/platform/win32/VideoEncoderNVENC.cpp:373-417`; `alvr/session/src/settings.rs:148-157`
- **Ref:** korejan/ALVR VideoEncoderNVENC.cpp lines 257-297 is functionally identical to local (same avg==max, vbv*1.1, same override block) — korejan does NOT add a cappe

#### #7 — Auto-pick codec / preset / bit-depth from detected GPU (Ada) + headset (Quest 3) -> AV1 10-bit  · `ALVR-av1`
- **Action:** drop · **Effort:** high · **Impact:** quality — Large real-world quality-per-bit and banding win for this exact rig. AV1 (or even HEVC) at the same bitrate is materially cleaner than ALVR's default H264; 10-b
- **Now:** Codec, preset and bit-depth are STATIC user-set defaults: H264, NVENC preset P1(Speed)/LowLatency tuning, 8-bit. The only dynamic logic is downgrade-on-incompatibility (AV1->HEVC if client can't decode AV1; 10-bit->8-bit if client can't). There is zero capability-driven UPGRADE: an Ada AD103 + Quest3 (both fully AV1 10
- **Patch:** Highest-leverage, lowest-risk slice for THIS hardware: (1) Make preferred_codec default resolution capability-aware. Add a server-side NVENC capability probe in VideoEncoderNVENC (use the already-present GetCapabilityValue against NV_ENC_CODEC_AV1_GUID/HEVC_GUID at init, e.g. NV_ENC_CAPS_SUPPORT_10BIT_ENCODE and codec-GUID enumeration via nvEncGetEncodeGUIDs) and surface it to server_core. (2) In server_core/connecti
- **Anchors:** `alvr/session/src/settings.rs:518-529 + 1734-1736`; `alvr/session/src/settings.rs:1749-1752 + 277-281`; `alvr/server_core/src/connection.rs:714-728`; `alvr/server_core/src/connection.rs:690-699`
- **Ref:** No upstream/korejan auto-detect exists (korejan is H264/HEVC-only, no AV1, no auto codec). The capability-query primitive (NvEncoder::GetCapabilityValue, nvEncG

#### #15 — Chroma-upsample / debanding shader on decode path  · `ALVR-deb`
- **Action:** implement · **Effort:** low · **Impact:** quality — Moderate perceived quality win in gradients (skies, dark scenes, fog) which is exactly where 4:2:0+8-bit banding is visible on Quest 3 LCD. A TPDF dither pass i
- **Now:** The decoded frame (an AHardwareBuffer in YUV 4:2:0, 8- or 10-bit) is bound as a GL_TEXTURE_EXTERNAL_OES texture. Chroma upsampling and YUV->RGB are done implicitly by the Qualcomm GPU's external-texture sampler (plain bilinear chroma reconstruction). staging_fragment.glsl copies that to a staging RGBA texture (mediump)
- **Patch:** Add an ordered-dither / triangular-noise deband pass to stream.wgsl fragment_main, applied just before output (after sRGB/gamma, before return). Cheapest effective version: add ~0.5-1 LSB of triangular probability density function (TPDF) dither derived from screen-space coords + frame index, gated behind a new override flag DEBAND (plumbed like ENABLE_UPSCALING). This breaks up 8-bit quantization banding with negligi
- **Anchors:** `alvr/graphics/resources/stream.wgsl:72-163`; `alvr/graphics/resources/staging_fragment.glsl:4-21`; `alvr/graphics/src/staging.rs:120-147`; `alvr/graphics/src/lib.rs:85-112`
- **Ref:** none (no debanding exists in alvr-org or korejan client). Standard TPDF/ordered-dither shader technique.

#### #16 — MediaCodec low-latency tuning (KEY_LOW_LATENCY, operating-rate, priority, output buffering)  · `ALVR-mci`
- **Action:** cherry-pick · **Effort:** trivial · **Impact:** latency — Small. The big lever (qti vendor low-latency) is already pulled, so this is incremental: re-enabling KEY_LOW_LATENCY may shave a small amount of decoder-interna
- **Now:** ALVR already sets the most impactful low-latency knobs for the Quest: operating-rate=MAX (decoder runs flat-out), priority=0 (realtime), and the Qualcomm vendor low-latency flag vendor.qti-ext-dec-low-latency.enable=1 -- which is the one that actually matters on Quest 3's Adreno/Qualcomm decoder. The standard Android K
- **Patch:** Two small, safe knobs remain: (1) Re-enable the standard android 'low-latency' (KEY_LOW_LATENCY) key for Quest 3 specifically. Quest 3 is API 32+ (Android 12+/13), well past the API-30 floor that caused the original crash concern, and on Qualcomm setting both the std key and the qti vendor key is fine. Add it to the default dict (or gate by API level) -- low risk, may reduce internal decoder reorder/lookahead latency
- **Anchors:** `alvr/session/src/settings.rs:1813-1838`; `alvr/client_core/src/video_decoder/android.rs:258-281`; `alvr/client_core/src/video_decoder/android.rs:250-256`; `alvr/client_core/src/video_decoder/android.rs:58-77`
- **Ref:** none directly; KEY_LOW_LATENCY usage is standard Android MediaCodec (API 30+). korejan's client is a separate (alxr) architecture and not a 1:1 reference for th

### Tier 3 — Encode-path latency
_Real encode-path cuts exploiting the dual AD103 NVENC engines. Split-frame needs the SDK bump first._

#### #13 — Zero-copy capture->encode (register DXGI texture directly as NVENC input)  · `ALVR-zcp`
- **Action:** cherry-pick · **Effort:** med · **Impact:** latency — Low-to-medium. Eliminates one on-GPU full-frame CopyResource (~4096-wide RGBA) per frame from the critical encode path. On a 4080S this copy is a small number o
- **Now:** NOT zero-copy. NvEncoderD3D11::AllocateInputBuffers CREATES its own internal D3D11 input textures and registers THOSE with NvEncRegisterResource (NvEncoderD3D11.cpp:71-106, via NvEncoder::RegisterInputResources/RegisterResource at NvEncoder.cpp:651-703). Then every frame VideoEncoderNVENC::Transmit does an explicit GPU
- **Patch:** Register the FrameRender output texture(s) directly as the NVENC input resource and skip AllocateInputBuffers' internal textures + the per-frame CopyResource. Concretely: have NvEncoderD3D11 (or a thin ALVR subclass) accept an externally-owned ID3D11Texture2D and call RegisterResource on it instead of creating its own, then EncodeFrame maps that registered resource directly. Because FrameRender uses one persistent st
- **Anchors:** `alvr/server_openvr/cpp/platform/win32/VideoEncoderNVENC.cpp:118-122`; `alvr/server_openvr/cpp/platform/win32/NvEncoderD3D11.cpp:71-106`; `alvr/server_openvr/cpp/platform/win32/NvEncoder.cpp:651-703`; `alvr/server_openvr/cpp/platform/win32/FrameRender.cpp:880`
- **Ref:** korejan/ALVR does NOT solve this - it also does CopyResource(pInputTexture, pTexture) at VideoEncoderNVENC.cpp:110. So no off-the-shelf reference; would be orig

#### #2 — NVENC Split-Frame Encoding toggle (splitEncodeMode)  · `ALVR-sdk/ALVR-spl`
- **Action:** cherry-pick · **Effort:** med · **Impact:** latency — Medium-high for this exact hardware. The 4080S (AD103) has TWO independent NVENC engines; AUTO split lets a SINGLE session distribute the frame across both engi
- **Now:** splitEncodeMode is NEVER set anywhere in the local tree. VideoEncoderNVENC::FillEncodeConfig sets many NV_ENC_CONFIG / rcParams fields but never touches initializeParams.encodeConfig split mode (VideoEncoderNVENC.cpp:162-418). CRITICAL BLOCKER: the bundled NVENC API header is version 12.0 (nvEncodeAPI.h:117-118, NVENCA
- **Patch:** Two-step: (1) Update the vendored NVIDIA Video Codec SDK headers (alvr_server/nvEncodeAPI.h, and NvEncoder.cpp/.h / NvEncoderD3D11.* if signatures changed) from v12.0 to >=12.2 (SDK 13 like korejan). (2) Add a setting nvenc.split_encode_mode (enum: Disabled/Auto/TwoForced/ThreeForced, default Auto) to NvencConfig (settings.rs:113-160), plumb through Settings.cpp/Settings.h as m_nvencSplitEncodeMode, then in FillEncod
- **Anchors:** `alvr/server_openvr/cpp/alvr_server/nvEncodeAPI.h:117-118`; `alvr/server_openvr/cpp/platform/win32/VideoEncoderNVENC.cpp:162-418`; `alvr/server_openvr/cpp/platform/win32/NvEncoder.cpp:154-155`
- **Ref:** korejan/ALVR VideoEncoderNVENC.cpp:160 (initializeParams.splitEncodeMode = NV_ENC_SPLIT_AUTO_MODE) on NVENC SDK 13

### Tier 4 — Architectural (last)
_Only pose prediction survived verification; needs the HUD + hard clamping._

#### #19 — Improved pose prediction  · `ALVR-pp2`
- **Action:** implement · **Effort:** high · **Impact:** quality — Quality/smoothness, moderate. The biggest easy win is the variance-aware horizon (A): on a low-jitter wired Link the latency is stable, so gains there are small
- **Now:** Prediction is a first-order, constant-velocity extrapolation. The prediction horizon equals the moving-average total pipeline latency, clamped to max_prediction_ms (default 100). For the head on Quest, the pose is located at the render-time predicted_display_time by the OpenXR runtime (so Meta's own constant-velocity p
- **Patch:** Two independent, low-risk-first levers: (A) Horizon estimator: replace the flat average in average_total_pipeline_latency (client_core/src/statistics.rs) with a percentile or mean+k*stddev so the predict-ahead tracks the latency the frame will ACTUALLY experience rather than the mean (reduces under-prediction during latency spikes). (B) Pose model: for controllers (and any pose ALVR extrapolates itself), add a 2nd-or
- **Anchors:** `alvr/client_core/src/lib.rs:239-250`; `alvr/client_core/src/statistics.rs:124-126`; `alvr/client_openxr/src/interaction.rs:694-737`; `alvr/client_openxr/src/stream.rs:547, 631-645`

### Dependency order
```
ALVR-n20 (HUD/dump)  ──┬─> ALVR-cvb (capped-VBR)
                       ├─> ALVR-av1 (AV1 10-bit default)
                       ├─> ALVR-zcp (zero-copy)
                       ├─> ALVR-sdk (SDK 12->13) ──> ALVR-spl (split-frame AUTO)
                       └─> ALVR-pp2 (pose prediction)
ALVR-deb (deband)   ── independent
ALVR-mci (mediacodec input) ── independent
```

**Sequencing:** Opt 20 first (de-risks 2,4,13). Tier 2 parallel: 4,7,15,16. Tier 3: zero-copy 13, then SDK bump validated then splitEncodeMode AUTO toggle. Pose prediction 19 last. Opt 2 and 13 exploit the dual-NVENC silicon; FEC/probe/QoS/spacewarp/phase dropped on the wired lossless link.

## Dropped / not applicable

Verified against source and rejected for this hardware (full reasoning in the workflow output).

| # | Item | Why dropped |
|---|---|---|
| 1 | Per-eye dual NVENC sessions | RECOMMEND DROP. The gap is real but not worth implementing on this hardware. Patch_sketch's own reasoning is correct: a 4080 Super encodes one ~4096-w |
| 3 | Sliced / subframe streaming (overlap encode->n | RECOMMEND DROP for this hardware. Reasons:

1. Gap is real on the SERVER side but the PAYOFF is gated by the CLIENT, and the client is the bottleneck  |
| 5 | 2-pass quarter-resolution as the default NVENC | No corrections needed; the claim is fully accurate. All four cited locations verified against actual local source on the current branch: (1) settings. |
| 7 | Auto-pick codec / preset / bit-depth from dete | RECOMMEND DROP for THIS hardware (RTX 4080 Super + wired Quest 3). Rationale:

1. The 'auto-pick' machinery (NVENC GUID enumeration probe + plumbing c |
| 8 | Reference-frame invalidation / LTR instead of  | Recommend DROP for this specific hardware, despite the gap being real. Rationale: (1) Target is WIRED USB Quest 3 (USB 3 ~2.5 Gbps over the cable / de |
| 12 | VRS/ROI-aligned foveation QP map at the encode | Recommend DROP for this hardware. Rationale: FFR already physically downsamples the periphery BEFORE the encoder sees the texture — that is the domina |
| 6 | Network throughput probe on connect to seed ad | GAP IS REAL but recommend DROP for this hardware. Reasoning: (1) Default is ConstantMbps(30) with variant=ConstantMbps (settings.rs:1691,1723), so out |
| 9 | Multi-channel / prioritized transport (separat | RECOMMEND DROP for target hardware (Quest 3, wired USB tether). Reasoning:

(A) Per-flow DSCP: moot on a USB/loopback tether. DSCP is a hint for conge |
| 10 | Adaptive FEC + tighter send pacing (Reed-Solom | Recommend DROP for the target hardware. On a wired USB Quest 3 link the loss rate is effectively zero and jitter is low, so: (1) Reed-Solomon FEC woul |
| #11 | Eye-tracked (gaze-contingent) foveated encodin | Repo: github.com/doublej/ALVR fork (origin). Branch HEAD detached. Key files re-read and verified: alvr/server_openvr/cpp/platform/win32/FFR.cpp (29-9 |
| 3 | Incremental / partial-frame decode (whole-fram | Note on target hardware framing: this optimization is Android/Quest-client-side (MediaCodec), so it is independent of the Windows/RTX 4080 server NVEN |
| 14 | Spacewarp / frame extrapolation (ASW/SSW equiv | Branch: diagnostics. origin = github.com/doublej/ALVR fork (no upstream/korejan remote configured, so cherry-pick from upstream cannot be confirmed lo |
| 17 | FSR-class edge-adaptive client upscaler (strea | Verdict: DROP / moot. The 'FSR-class edge-adaptive client upscaler (stream lower res, reconstruct on device)' is fully implemented, wired end-to-end,  |
| 18 | Capture<->vsync phase alignment (server frame  | Recommend DROP for the WIRED Quest 3 / RTX 4080S target, for three reasons:

(a) Phase alignment between capture and display matters most when there's |

> Note: #7 (auto-*detect* codec) was dropped as high-effort, but the **default flip to AV1 10-bit** (`ALVR-av1`) was kept — your hardware is known, so no detection is needed.

## Build (later, on Windows)

```
cargo xtask prepare-deps --platform windows
cargo xtask build-streamer --release
# client: cargo xtask build-client --release  (needs NDK env)
```

Encoder changes live in `alvr/server_openvr/cpp/platform/win32/` — only compilable on the Windows box (MSVC + D3D11 + NVENC).
