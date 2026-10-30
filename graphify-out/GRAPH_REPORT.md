# Graph Report - glkvm-rs  (2026-10-30)

## Corpus Check
- 257 files · ~402,033 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 19 file(s) not represented in the graph (top: (none) 6, .toml 5, .conf 4)

## Summary
- 7479 nodes · 16918 edges · 471 communities (236 shown, 235 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 222 edges (avg confidence: 0.75)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `929b415f`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- H266 Packetizer
- Stream TX/Mid
- cometdesk-cli/src/main.rs
- cometdesk-clip/src/lib.rs
- Media Model
- SSRC/Streams API
- Console
- Probe Control
- register_nack.rs
- SeqNo
- client.rs
- bridge.rs
- Rtc Core
- SDP Data
- Loss Controller
- clipsync.rs
- Crypto Config
- duration
- VP8 Payload
- events.rs
- SDP Negotiation
- str0m/src/lib.rs
- src/hid.rs
- Pacer Tests
- clipboard.rs
- src/info.rs
- store.rs
- tls.rs
- Errors
- Time Types
- Session Mgmt
- Session
- rtc.rs
- Trendline BWE
- Probe Estimator
- SDP Parser
- src/direct.rs
- Media Time
- SRTP
- TwccClusterId
- CodecConfig
- MediaAttribute
- flexfec.rs
- Fingerprint
- rate_control.rs
- h264.rs
- audio.rs
- input.rs
- Result
- av1.rs
- channel.rs
- rtcp/mod.rs
- streamer.rs
- codec.rs
- cometdesk-agent/src/main.rs
- CodecPacketizer
- PacketError
- viewer.rs
- display.rs
- SrtpCrypto
- vt.rs
- MediaKind
- red.rs
- app.rs
- DevicesUi
- Pt
- theme.rs
- sdp/mod.rs
- payload_params.rs
- Formatter
- vla.rs
- Deserialize
- ReportList
- Changes
- vp9.rs
- .push
- mock_device.rs
- ProbeClusterConfig
- RtpPacket
- DelayController
- Depacketizer
- Error
- janus.rs
- Media
- .new
- SessionAttribute
- insert.rs
- RtcSctp
- VideoRenderer
- cometdesk_bridge.py
- Msg
- rtx_cache_buf.rs
- arrival_group.rs
- SendSideBandwidthEstimator
- H265ProfileTierLevel
- H266ProfileTierLevel
- .do_poll
- h265.rs
- twcc.rs
- annexb.rs
- crypto/mod.rs
- RenderPath
- console.rs
- zerocopy.rs
- actions.rs
- shell.rs
- Extension
- PayloadParams
- FormatParam
- TargetOs
- Gpu
- super
- DepacketizingBuffer
- io/mod.rs
- detector.rs
- Reason
- Codec
- packet/tele.rs
- fuzz.rs
- .toolbar
- Community 128
- HidCommand
- RtcpFb
- PacketChunk
- sctp/mod.rs
- pion.rs
- Msg
- app/control.rs
- TwccSendRecord
- layout/mod.rs
- contiguity_vp8.rs
- H264LevelIdc
- Extension
- CodecDepacketizer
- Event
- SdesType
- SctpInitData
- Secret
- DataSize
- event.rs
- volume.rs
- pipeline.rs
- steps
- steps
- H266Level
- Bridge
- ci.yml
- keymap.rs
- str0m/src/config.rs
- with
- Signal
- Direction
- contiguity_vp9.rs
- H265Level
- TwccSeq
- Twcc
- ext.rs
- get_h266
- buffer_rx.rs
- Info
- FormatParams
- link_capacity_estimator.rs
- CFRef
- VideoChange
- read
- PinStore
- gen_icon.swift
- device.rs
- SctpError
- ChannelConfig
- H264Profile
- test_client.py
- gen-layouts.py
- cargo.yml
- glkvm-rs-native-macos-console-for-gl-inet-comet-pl.md
- H265AggregationPacket
- H265PACIPacket
- H265Profile
- capabilities.rs
- bench_app.py
- PowerAction
- .new
- jobs
- wol.rs
- source.rs
- src/receive.rs
- super
- json
- matrix
- matrix
- packet/mod.rs
- UserEvent
- .depacketize
- H266Profile
- BitRead
- SctpEvent
- smoother.rs
- SrtpProfile
- RtpMeta
- H264ProfileIdc
- H265Packetizer
- (&[u8], usize)
- UserExtensionValues
- Delta
- Pion WebRTC+FlexFEC transport (/pion/ws)
- Fingerprint
- Clipboard bridge design
- glkvm-rs native console project
- Binary HID frame encoding (U1)
- fmt.rs
- jobs
- with
- ExtensionsForm
- asyncio
- PowerAction
- ethernet
- cometdesk-media
- str0m Sans-IO WebRTC library
- H264ProfileLevel
- value_history.rs
- ClipDirection
- WheelAccumulator
- Av1Packetizer
- glkvm-gui eframe/egui app
- manifest.yaml
- annexB
- BitPattern
- RtcSctpState
- linux-crypto-test
- windows-crypto-test
- platform
- glkvm-rs native console project
- _internal_test_exports/mod.rs
- Rng<'a>
- ProbeResult
- BufferedThresholdConfig
- Ui
- api/mod.rs
- build-agents.sh
- macos-crypto-test
- on
- .try_from
- Instant
- VideoFrame
- Measurement targets M1-M7
- setup.sh
- ocr.rs
- remove.sh
- Fixture-based testing strategy
- package-dmg.sh
- sign-run.sh
- PayloadParams
- ssl-gen.sh
- tokenade CLI agent conventions
- GL native client (/lws/ws), out of scope
- README.md
- target-display-fix.sh
- COMETDESK.md
- PacerReason
- Item
- VtDecoder
- Volume
- ObuType
- MovingAverage
- EdidPreset
- layout/data.rs
- Serializer
- ConsoleAction
- Av1Depacketizer

## God Nodes (most connected - your core abstractions)
1. `Console` - 123 edges
2. `Ssrc` - 115 edges
3. `PacketError` - 114 edges
4. `Pt` - 110 edges
5. `Mid` - 101 edges
6. `Session` - 98 edges
7. `RtcConfig` - 86 edges
8. `SeqNo` - 86 edges
9. `Session` - 85 edges
10. `Media` - 83 edges

## Surprising Connections (you probably didn't know these)
- `Insert clipboard vs type (protocol v2)` --semantically_similar_to--> `Type clipboard (hid/print)`  [INFERRED] [semantically similar]
  docs/clipboard-bridge.md → PLAN.md
- `supervise()` --calls--> `wake()`  [INFERRED]
  apps/cometdesk-gui/src/clipsync.rs → crates/cometdesk-cli/src/commands.rs
- `quality()` --calls--> `show()`  [INFERRED]
  crates/cometdesk-cli/src/commands.rs → apps/cometdesk-gui/src/sysnotify.rs
- `U3: streamer lifecycle` --references--> `glkvm-media video/audio sources`  [INFERRED]
  docs/protocol.md → PLAN.md
- `Login / auth (URL-encoded, 2FA, two-step)` --references--> `glkvm::client REST client`  [INFERRED]
  docs/protocol.md → PLAN.md

## Import Cycles
- 1-file cycle: `vendor/str0m/src/lib.rs -> vendor/str0m/src/lib.rs`
- 1-file cycle: `vendor/str0m/src/pacer/leaky.rs -> vendor/str0m/src/pacer/leaky.rs`
- 1-file cycle: `crates/cometdesk/src/layout/mod.rs -> crates/cometdesk/src/layout/mod.rs`
- 1-file cycle: `vendor/str0m/src/rtp/rtcp/twcc.rs -> vendor/str0m/src/rtp/rtcp/twcc.rs`
- 1-file cycle: `vendor/str0m/src/bwe/loss_controller.rs -> vendor/str0m/src/bwe/loss_controller.rs`
- 1-file cycle: `vendor/str0m/src/packet/contiguity_vp8.rs -> vendor/str0m/src/packet/contiguity_vp8.rs`
- 1-file cycle: `vendor/str0m/src/packet/contiguity_vp9.rs -> vendor/str0m/src/packet/contiguity_vp9.rs`
- 1-file cycle: `apps/cometdesk-agent/src/main.rs -> apps/cometdesk-agent/src/main.rs`
- 1-file cycle: `vendor/str0m/src/bwe/delay/rate_control.rs -> vendor/str0m/src/bwe/delay/rate_control.rs`
- 1-file cycle: `vendor/str0m/src/bwe/delay/arrival_group.rs -> vendor/str0m/src/bwe/delay/arrival_group.rs`
- 1-file cycle: `vendor/str0m/src/streams/register.rs -> vendor/str0m/src/streams/register.rs`
- 1-file cycle: `vendor/str0m/src/streams/register_nack.rs -> vendor/str0m/src/streams/register_nack.rs`
- 1-file cycle: `vendor/str0m/src/util/value_history.rs -> vendor/str0m/src/util/value_history.rs`
- 1-file cycle: `vendor/str0m/src/crypto/srtp.rs -> vendor/str0m/src/crypto/srtp.rs`
- 1-file cycle: `vendor/str0m/src/rtp/error.rs -> vendor/str0m/src/rtp/error.rs`
- 1-file cycle: `vendor/str0m/src/bwe/time.rs -> vendor/str0m/src/bwe/time.rs`
- 1-file cycle: `vendor/str0m/src/rtp/rtcp/list.rs -> vendor/str0m/src/rtp/rtcp/list.rs`
- 1-file cycle: `vendor/str0m/src/bwe/macros.rs -> vendor/str0m/src/bwe/macros.rs`
- 2-file cycle: `vendor/str0m/src/bwe/delay/control.rs -> vendor/str0m/src/bwe/mod.rs -> vendor/str0m/src/bwe/delay/control.rs`
- 2-file cycle: `apps/cometdesk-gui/src/app.rs -> apps/cometdesk-gui/src/console.rs -> apps/cometdesk-gui/src/app.rs`

## Hyperedges (group relationships)
- **Clipboard bridge end-to-end pipeline** — docs_clipboard_bridge_agent, docs_clipboard_bridge_relay, docs_clipboard_bridge_noise [EXTRACTED 1.00]
- **glkvm-media transport sources** — plan_direct_h264, plan_pion_webrtc, plan_janus [EXTRACTED 1.00]

## Communities (471 total, 235 thin omitted)

### Community 0 - "H266 Packetizer"
Cohesion: 0.06
Nodes (86): annexb(), ANNEXB_NALUSTART_CODE, depacketize_all(), detect_h266_keyframe(), H266Depacketizer, H266FRAGMENTATION_UNIT_HEADER_SIZE, H266FragmentationUnitHeader, H266NALU_AGGREGATION_PACKET_TYPE (+78 more)

### Community 1 - "Stream TX/Mid"
Cohesion: 0.05
Nodes (19): RtcpReportIntervals, PacketReceipt, MIN_SPURIOUS_PADDING_SIZE, NextPacket, NextPacketKind, Blank, Regular, Resend (+11 more)

### Community 2 - "cometdesk-cli/src/main.rs"
Cohesion: 0.03
Nodes (75): BridgeCmd, Remove, Setup, Status, ButtonArg, Power, PowerLong, Reset (+67 more)

### Community 3 - "cometdesk-clip/src/lib.rs"
Cohesion: 0.05
Nodes (57): CHUNK, clipboard_file_streams_into_staging(), cut(), Endpoint, Error, Crypto, Frame, Handshake (+49 more)

### Community 4 - "Media Model"
Cohesion: 0.04
Nodes (17): blocked_buffer(), deadlines_cover_payload_types_rids_and_resets(), MAX_PENDING_PAYLOADS, Media, PayloaderEntry, Rids, Any, None (+9 more)

### Community 5 - "SSRC/Streams API"
Cohesion: 0.04
Nodes (12): DirectApi<'a>, AppSpecificFeedback, MID_PROBE, MidRid, Ssrc, RX_LOOKUP_CLEANUP_INTERVAL, RX_LOOKUP_EXPIRY, RxLookup (+4 more)

### Community 6 - "Console"
Cohesion: 0.08
Nodes (3): Console, RegionPick, Typing

### Community 7 - "Probe Control"
Cohesion: 0.10
Nodes (31): allocation_probe_fires_when_desired_increases_in_alr(), allocation_probe_is_triggered_in_alr_when_allocation_increases(), allocation_probe_requires_desired_increase_in_alr(), ALR_ENDED_TIMEOUT, BandwidthLimitedCause, DelayBasedLimited, DelayBasedLimitedDelayIncreased, LossLimitedBwe (+23 more)

### Community 8 - "register_nack.rs"
Cohesion: 0.07
Nodes (38): all_packets_have_same_frequency(), expected(), expected_overflow(), expected_received_loss(), jitter_at_0(), jitter_at_20(), jitter_rtp_timestamp_rollover(), active_window_sliding() (+30 more)

### Community 9 - "SeqNo"
Cohesion: 0.04
Nodes (24): DATAGRAM_MAX_PACKET_SIZE, mk_extend!, RtpHeader, test_generate_one_length_padding_packet(), test_parse_rtp_headers(), test_parse_rtp_headers_two_byte_form(), test_parse_rtp_headers_with_csrc(), truncate_off_srtp_padding() (+16 more)

### Community 10 - "client.rs"
Cohesion: 0.05
Nodes (22): ConfigResult, Connection, Credentials, Enabled, Envelope, ErrorResult, header(), Inner (+14 more)

### Community 11 - "bridge.rs"
Cohesion: 0.07
Nodes (31): health_text(), agent_binary(), agents_dir(), BridgeSetup, Inner, AGENT_NAMES, BRIDGE_VERSION, BRIDGE_WS (+23 more)

### Community 12 - "Rtc Core"
Cohesion: 0.05
Nodes (12): Bwe, DirectApi, SdpApi, Channel<'a>, Rtc, rtc_is_send(), rtc_is_unwind_safe(), RtcState (+4 more)

### Community 13 - "SDP Data"
Cohesion: 0.06
Nodes (45): bundle_mids_returns_correct_mids(), FingerprintFmt, g722_red_emits_wire_clock_rate(), h265_and_h264_coexist(), h265_chrome_profile_only_sdp(), h265_empty_fmtp_not_serialized(), h265_in_complete_sdp(), h265_incomplete_params_all_permutations() (+37 more)

### Community 14 - "Loss Controller"
Cohesion: 0.07
Nodes (26): AsValid, ChannelParameters, Config, HoldInfo, LogNormalDistribution, loss_spike_recovery(), LossBasedBweResult, LossController (+18 more)

### Community 15 - "clipsync.rs"
Cohesion: 0.05
Nodes (55): AckRequest, AckResult, ActiveFile, agent_is_older(), BACKOFF_MAX, BACKOFF_MIN, Batch, BridgeState (+47 more)

### Community 17 - "duration"
Cohesion: 0.06
Nodes (19): RemoteIngressStats, Losses, Disabled, Enabled, StreamTxStats, calculate_rtt(), (Option<Instant>, T), Soonest (+11 more)

### Community 18 - "VP8 Payload"
Cohesion: 0.06
Nodes (25): KEY_IDX_MASK, PICTURE_ID_MAX, PICTURE_ID_SHORT_MASK, PICTURE_ID_SHORT_MAX, read_vp8_descriptor_byte(), rewrite_payload(), test_vp8_descriptor_exposes_fields(), test_vp8_descriptor_keyframe_detection_matches_helper() (+17 more)

### Community 19 - "events.rs"
Cohesion: 0.07
Nodes (31): BACKOFF_MAX, BACKOFF_MIN, ConnState, Closed, Connected, Connecting, Disconnected, Kicked (+23 more)

### Community 20 - "SDP Negotiation"
Cohesion: 0.09
Nodes (33): add_ice_details(), AddMedia, answer_uses_negotiated_dtls_role(), apply_direct_changes(), as_new_medias_contiguous_indices_with_non_media_changes(), Change, AddApp, AddChannel (+25 more)

### Community 21 - "str0m/src/lib.rs"
Cohesion: 0.02
Nodes (63): Event, AppSpecificFeedback, ChannelData, Closed, Connected, EgressBitrateEstimate, FecRtp, KeyframeRequest (+55 more)

### Community 22 - "src/hid.rs"
Cohesion: 0.07
Nodes (33): map_button(), abs_mapping_is_monotonic(), click_flushes_pending_move_first(), Coalescer, DEFAULT_MOVE_INTERVAL, duplicate_position_is_not_resent(), encode_button(), encode_key() (+25 more)

### Community 23 - "Pacer Tests"
Cohesion: 0.10
Nodes (27): assert_poll_success(), duration_ms(), enqueue_packet_noisy(), handle_timeout_noisy(), Inner, make_packet(), MAX_BITRATE, MAX_DEBT_IN_TIME (+19 more)

### Community 24 - "clipboard.rs"
Cohesion: 0.06
Nodes (33): grab(), region(), screenshot(), ClipCommand, RestoreSoon, Set, SetForInsert, IMAGE_EVERY (+25 more)

### Community 25 - "src/info.rs"
Cohesion: 0.05
Nodes (13): DESKTOP_SWITCHDESKTOP, EVERY, focus(), IMC_GETOPENSTATUS, is_terminal(), kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, LOAD_EVERY (+5 more)

### Community 26 - "store.rs"
Cohesion: 0.10
Nodes (29): BLOB_PREFIX, CACHE, clip_secret(), Credentials, credentials_round_trip_and_read_legacy_password(), delete_clip_secret(), delete_password(), device() (+21 more)

### Community 27 - "tls.rs"
Cohesion: 0.10
Nodes (6): client_config(), fingerprint(), parse_ignores_comments_and_blank_lines(), parse_pins(), save_pins(), TofuVerifier

### Community 28 - "Errors"
Cohesion: 0.07
Nodes (23): RtcError, ChangesOutOfOrder, Dtls, FeedbackNotEnabled, Ice, Io, Net, NoReceiverSource (+15 more)

### Community 29 - "Time Types"
Cohesion: 0.08
Nodes (12): instant_ord(), super_instant_test(), TimeDelta, Negative, NegativeInfinity, Positive, PositiveInfinity, .ZERO (+4 more)

### Community 30 - "Session Mgmt"
Cohesion: 0.04
Nodes (31): Bwe<'a>, BweKind, Remb, Twcc, FecRtp, no_padding_without_active_media(), overuse_suppresses_padding(), PacerControl (+23 more)

### Community 32 - "Session"
Cohesion: 0.09
Nodes (33): show(), bridge_remove(), bridge_setup(), bridge_status(), click(), confirm(), edid(), FIXTURE_ROUTES (+25 more)

### Community 33 - "rtc.rs"
Cohesion: 0.06
Nodes (28): AudioState, Closed, Connecting, Disconnected, Streaming, BACKOFF_MAX, BACKOFF_MIN, Ended (+20 more)

### Community 34 - "Trendline BWE"
Cohesion: 0.10
Nodes (23): InterGroupDelayDelta, DELAY_COUNT_RANGE, delay_variation(), duration_ms(), K_DOWN, K_UP, MAX_ADOPT_OFFSET_MS, OVER_USE_THRESHOLD_DEFAULT_MS (+15 more)

### Community 35 - "Probe Estimator"
Cohesion: 0.10
Nodes (20): log_probe_bitrate_estimate!, invalid_receive_send_ratio_is_rejected(), lost_probe_packets_do_not_affect_estimate(), MAX_ACTIVE_PROBES, MAX_PROBE_INTERVAL, MAX_VALID_RATIO, MIN_CLUSTER_SIZE, MIN_RATIO_FOR_UNSATURATED_LINK (+12 more)

### Community 36 - "SDP Parser"
Cohesion: 0.06
Nodes (53): ChannelBufferedAmountLow, ChannelClose, ChannelOpen, IceConnectionStateChange, Input, Receive, Timeout, Output (+45 more)

### Community 37 - "src/direct.rs"
Cohesion: 0.07
Nodes (34): BACKOFF_MAX, BACKOFF_MIN, BinMessage, Frame, Pong, Unknown, choose_format(), DirectOptions (+26 more)

### Community 38 - "Media Time"
Cohesion: 0.04
Nodes (34): long_event_uses_contiguous_segments(), MAX_DURATION, MAX_SEGMENT, MIN_DURATION, MIN_PAUSE, packets(), telephone_packets_have_separate_send_times(), TelephonePackets (+26 more)

### Community 39 - "SRTP"
Cohesion: 0.07
Nodes (34): CTR_BUFFER_PADDING, DECRYPTED_PAYLOAD, error_details(), KEY, LABEL_RTCP_AES, LABEL_RTCP_AUTHENTICATION_KEY, LABEL_RTCP_SALT, LABEL_RTP_AES (+26 more)

### Community 40 - "TwccClusterId"
Cohesion: 0.07
Nodes (14): LeakyBucketPacer, Pacer, PacerImpl, LeakyBucket, Null, NullPacer, PaddingRequest, QueuePriority (+6 more)

### Community 41 - "CodecConfig"
Cohesion: 0.11
Nodes (14): CodecConfig, comfort_noise_supports_dynamic_payload_types_at_higher_clock_rates(), conflicting_remote_red_mappings_do_not_panic(), enable_comfort_noise_uses_static_8khz_payload_type(), enable_red_links_all_audio_codecs_with_distinct_pts(), enable_red_links_opus(), red_dropped_when_remote_lacks_red(), red_kept_when_both_sides_enable() (+6 more)

### Community 42 - "MediaAttribute"
Cohesion: 0.04
Nodes (37): CodecSpec, is_dir(), MediaAttribute, Candidate, EndOfCandidates, ExtMap, Fingerprint, Fmtp (+29 more)

### Community 43 - "flexfec.rs"
Cohesion: 0.09
Nodes (38): a_rebuilt_packet_completes_another_repair(), complete_group_counts_as_consistent(), Decoder, encode(), expect(), extend(), feed(), flags() (+30 more)

### Community 45 - "rate_control.rs"
Cohesion: 0.12
Nodes (17): BETA, DEFAULT_BACKOFF_TIME, duration_ms(), make_control(), MAX_ESTIMATE_RATIO, MULTIPLICATIVE_INCREASE_COEF, OBSERVED_BIT_RATE_SMOOTHING_FACTOR, OBSERVED_BITRATE_RESET_THRESHOLD_STD (+9 more)

### Community 46 - "h264.rs"
Cohesion: 0.06
Nodes (33): ANNEXB_NALUSTART_CODE, AUD_NALU_TYPE, FILLER_NALU_TYPE, FU_END_BITMASK, FU_START_BITMASK, FUA_HEADER_SIZE, FUA_NALU_TYPE, FUB_NALU_TYPE (+25 more)

### Community 47 - "audio.rs"
Cohesion: 0.07
Nodes (18): AudioSession, CHANNELS, device_name(), MAX_BUFFER, MAX_FRAME, output_devices(), PACKET_QUEUE, Player (+10 more)

### Community 48 - "input.rs"
Cohesion: 0.11
Nodes (25): autorepeat_is_dropped(), caps_lock_is_a_normal_key_off_macos(), caps_lock_toggle_is_a_full_press_each_time(), cursor_in_black_bars_is_dropped_and_blocks_clicks(), cursor_maps_corners_to_device_range(), focus_loss_releases_everything(), InputMapper, keys_pressed_under_cmd_are_released_with_cmd() (+17 more)

### Community 49 - "Result"
Cohesion: 0.17
Nodes (32): packetize_respects_mtu(), Result, test_h265_aggregated_exact_layout(), test_h265_aggregation_with_donl_sequences(), test_h265_annexb_roundtrip_with_depacketizer(), test_h265_ap_buffer_overflow_fallback(), test_h265_ap_roundtrip_with_depacketizer(), test_h265_donl_increments_correctly() (+24 more)

### Community 50 - "av1.rs"
Cohesion: 0.08
Nodes (33): AGGREGATION_HEADER_SIZE, discards_temporal_delimiter_and_tile_list_obu(), many_packets_with_border_aligned_obus(), MAX_NUM_OBUS_TO_OMTI_SIZE, OBU_EXTENSION_PRESENT_MASK, obu_payload_size_set_after_extension_when_absent(), obu_payload_size_set_after_extension_when_present(), obu_payload_size_set_when_absent() (+25 more)

### Community 51 - "channel.rs"
Cohesion: 0.11
Nodes (15): association_lost_releases_held_stream_ids(), Channel, channel_id_allocation(), ChannelAllocation, ChannelData, ChannelHandler, ChannelId, negotiated_stream_id_cannot_overlap_held_generation() (+7 more)

### Community 52 - "rtcp/mod.rs"
Cohesion: 0.03
Nodes (71): Goodbye, Fir, FirEntry, FeedbackMessageType, NotUsed, PayloadFeedback, ReceptionReport, SourceCount (+63 more)

### Community 54 - "streamer.rs"
Cohesion: 0.10
Nodes (23): VideoInfo, clamps_and_builds_query(), Encoder, H264State, Hdmi, is_jpeg(), limits(), presets_stay_inside_limits() (+15 more)

### Community 55 - "codec.rs"
Cohesion: 0.24
Nodes (3): g722_16khz_media_time_halves_on_the_wire(), g722_rtp_clock_rate_is_8khz(), g722_spec()

### Community 57 - "cometdesk-agent/src/main.rs"
Cohesion: 0.09
Nodes (22): BACKOFF_MAX, BACKOFF_MIN, BATTERY_LOW, DEFAULT_RELAY, detach_console(), fail(), IDLE_TIMEOUT, LOG_FILE (+14 more)

### Community 59 - "CodecPacketizer"
Cohesion: 0.06
Nodes (19): CodecPacketizer, Av1, Boxed, ComfortNoise, G711, G722, H264, H265 (+11 more)

### Community 60 - "PacketError"
Cohesion: 0.08
Nodes (25): PacketError, ErrAv1CorruptedPacket, ErrH265CorruptedPacket, ErrH265PACIPHESTooLong, ErrH266CorruptedPacket, ErrInvalidH265PacketType, ErrPayloadTooLarge, ErrRedCorruptedPacket (+17 more)

### Community 61 - "viewer.rs"
Cohesion: 0.10
Nodes (10): App, Bench, BENCH_WARMUP, build_event_loop(), process_cpu_time(), run(), UserEvent, Frame (+2 more)

### Community 62 - "display.rs"
Cohesion: 0.09
Nodes (10): blank(), change(), list(), MAX_MODES, modes(), Monitor, parse(), set() (+2 more)

### Community 63 - "SrtpCrypto"
Cohesion: 0.10
Nodes (8): AeadKey, AesKey, DummySrtpCryptoImpl, SrtpCrypto, AwsLc, OpenSsl, WinCrypto, SrtpCryptoImpl

### Community 64 - "vt.rs"
Cohesion: 0.06
Nodes (3): DELTA, KEY, output_callback()

### Community 65 - "MediaKind"
Cohesion: 0.07
Nodes (12): AsSdpMediaLine, MediaType, (Mid, usize), MediaKind, Audio, Video, MediaType, Application (+4 more)

### Community 66 - "red.rs"
Cohesion: 0.11
Nodes (23): RedSink<'a>, decode_rejects_too_many_redundant_blocks(), encode_skips_oversized_blocks(), F_BIT, MAX_BLOCK_LEN, MAX_RED_RECOVERY_DEPTH, MAX_REDUNDANT_BLOCKS, MAX_TS_OFFSET (+15 more)

### Community 67 - "app.rs"
Cohesion: 0.14
Nodes (5): App, connect(), hide_menu_bar_in_fullscreen(), set_dock_icon(), Connected

### Community 69 - "DevicesUi"
Cohesion: 0.16
Nodes (6): ConnectRequest, DevicesUi, Form, Prompt, Password, Totp

### Community 70 - "Pt"
Cohesion: 0.09
Nodes (24): CodecSpec, PT_AV1, PT_AV1_RTX, PT_COMFORT_NOISE, PT_G722, PT_G722_RED, PT_H265, PT_H265_RTX (+16 more)

### Community 71 - "theme.rs"
Cohesion: 0.09
Nodes (19): ACCENT, ACCENT_HOVER, apply(), banner(), BG, BORDER, CARD, CARD_HOVER (+11 more)

### Community 73 - "sdp/mod.rs"
Cohesion: 0.08
Nodes (11): VERSION, Sdp, SdpError, Inconsistent, ParseError, sdp_ser!, sdp(), SdpAnswer (+3 more)

### Community 74 - "payload_params.rs"
Cohesion: 0.13
Nodes (19): CodecSpec, [bool; 128], h264_codec_spec(), h265_codec_spec(), h265_codec_spec_profile_only(), h266_codec_spec(), h266_codec_spec_profile_only(), payload_params_red_defaults_none() (+11 more)

### Community 75 - "Formatter"
Cohesion: 0.12
Nodes (4): FingerprintFmt<'a>, Proto, Sctp, Srtp

### Community 76 - "vla.rs"
Cohesion: 0.12
Nodes (28): assert_ser_deser(), div_round_up(), parse_leb_u63(), read_bit(), read_bits(), read_lower_4bits_ignoring_leading_zeros(), ResolutionAndFramerate, serialize() (+20 more)

### Community 77 - "Deserialize"
Cohesion: 0.20
Nodes (10): HidState, Jiggler, Keyboard, KeyboardLeds, Keymaps, KeymapsResult, Mouse, Outputs (+2 more)

### Community 78 - "ReportList"
Cohesion: 0.13
Nodes (7): &'a ReportList<T>, Iter, IterOwned, ReportList<T>, test_lists_from_iter(), test_max_length_into_iter(), test_max_length_iter()

### Community 79 - "Changes"
Cohesion: 0.10
Nodes (17): add_new_lines(), add_pending_changes(), apply_answer(), apply_offer(), as_media_lines(), as_sdp(), AsSdpParams, AsSdpParams<'a, 'b> (+9 more)

### Community 80 - "vp9.rs"
Cohesion: 0.09
Nodes (18): detect_vp9_keyframe_bitstream(), MAX_SPATIAL_LAYERS, MAX_VP9REF_PICS, packetize_respects_mtu(), test_vp9_packet_unmarshal(), test_vp9_packetizer_flexible_mode(), test_vp9_packetizer_fragmented_keyframe(), test_vp9_packetizer_keyframe() (+10 more)

### Community 81 - ".push"
Cohesion: 0.22
Nodes (26): bounded_queue_meta(), full_queue_ignores_older_packets_and_duplicates(), incomplete_frames_cannot_grow_queue_forever(), padding_after_incomplete_vp8_frame_does_not_grow_without_bound(), padding_after_initial_incomplete_vp8_frame_does_not_grow_without_bound(), padding_after_waiting_vp8_frame_does_not_grow_without_bound(), padding_only_run_does_not_grow_the_queue_without_bound(), padding_only_run_with_loss_stays_bounded_and_reports_discontinuity() (+18 more)

### Community 82 - "mock_device.rs"
Cohesion: 0.17
Nodes (16): acceptor(), changed_certificate_is_refused(), concurrent_requests_share_one_relogin(), Config, creds(), expired_token_triggers_one_relogin(), Mock, PASSWORD (+8 more)

### Community 83 - "ProbeClusterConfig"
Cohesion: 0.09
Nodes (18): low_rate_padding_reaches_minimum_packet_count(), MAX_PADDING_PACKET_SIZE, min_probe_delta_is_enforced_in_next_probe_time(), probe_cluster_complete_even_with_zero_duration(), probe_cluster_complete_when_both_criteria_met(), probe_cluster_not_complete_after_bytes_but_not_packets(), probe_cluster_not_complete_after_packets_but_not_bytes(), probe_cluster_not_complete_before_start() (+10 more)

### Community 84 - "RtpPacket"
Cohesion: 0.10
Nodes (18): RtpPacket, after(), packet(), rtx_cache_0_duration(), rtx_cache_0_sized(), rtx_cache_100_sized_backwards(), rtx_cache_1_sized(), rtx_cache_eviction() (+10 more)

### Community 85 - "DelayController"
Cohesion: 0.17
Nodes (7): DelayController, MAX_RTT_HISTORY_WINDOW, MAX_TWCC_GAP, RTT_SMOOTHING_FACTOR, UPDATE_INTERVAL, log_bitrate_estimate!, log_delay_variation!

### Community 86 - "Depacketizer"
Cohesion: 0.09
Nodes (9): G711Depacketizer, G711Packetizer, G722Packetizer, G7xxPacketizer, packetize_respects_mtu(), test_g7xx_payload(), test_g7xx_unmarshal(), Depacketizer (+1 more)

### Community 87 - "Error"
Cohesion: 0.06
Nodes (25): CHUNK, flag(), MsdDrive, MsdImage, MsdPart, MsdState, MsdStorage, Session (+17 more)

### Community 88 - "janus.rs"
Cohesion: 0.11
Nodes (20): KEEPALIVE, msg(), negotiate(), parse_reply(), PLUGIN, PROTOCOL, Reply, Candidate (+12 more)

### Community 89 - "Media"
Cohesion: 0.14
Nodes (4): Media, Shared, STALE, Upload

### Community 90 - ".new"
Cohesion: 0.10
Nodes (8): detect_h265_keyframe(), H265FragmentationUnitHeader, H265FragmentationUnitPacket, H265NALUHeader, H265SingleNALUnitPacket, reconstruct_from_fu_packets(), test_detect_h265_keyframe(), test_h265_packetizer_fu_fragmentation_roundtrip_payload()

### Community 91 - "SessionAttribute"
Cohesion: 0.06
Nodes (19): Bandwidth, Session, SessionAttribute, AllowMixedExts, Candidate, EndOfCandidates, Fingerprint, Group (+11 more)

### Community 92 - "insert.rs"
Cohesion: 0.10
Nodes (17): insert(), key(), paste_key(), send_unicode(), paste_keys(), InsertReply, Failed, Inserted (+9 more)

### Community 95 - "VideoRenderer"
Cohesion: 0.11
Nodes (6): Nv12, RenderError, ZeroCopyUnavailable, SHADER, VideoRenderer, write_plane()

### Community 96 - "cometdesk_bridge.py"
Cohesion: 0.13
Nodes (12): _agent_path(), firmware_version(), handle_agent_download(), handle_agent_upload(), list_agents(), main(), _read(), _restrict_socket() (+4 more)

### Community 97 - "Msg"
Cohesion: 0.07
Nodes (28): Msg, Ack, Capture, CaptureFailed, Chunk, DirList, Fetch, FileCancel (+20 more)

### Community 98 - "rtx_cache_buf.rs"
Cohesion: 0.19
Nodes (12): buffer_cmp(), Entry, evict_all(), evict_oldest(), evict_with_gap(), EvictingBuffer, EvictingBuffer<T>, grow_and_reindex() (+4 more)

### Community 99 - "arrival_group.rs"
Cohesion: 0.13
Nodes (15): ArrivalGroup, ArrivalGroupAccumulator, Belongs, NewGroup, Skipped, Yes, BURST_TIME_INTERVAL, duration_us() (+7 more)

### Community 100 - "SendSideBandwidthEstimator"
Cohesion: 0.11
Nodes (12): log_inherent_loss!, log_loss!, log_loss_based_bitrate_estimate!, log_loss_bw_limit_in_window!, log_pacer_media_debt!, log_pacer_padding_debt!, BITRATE_WINDOW, Bwe (+4 more)

### Community 101 - "H265ProfileTierLevel"
Cohesion: 0.10
Nodes (7): H265ProfileTierLevel, .FALLBACK, H265Tier, High, Main, test_h265_profile_tier_level_from_fmtp(), test_h265_profile_tier_level_new()

### Community 102 - "H266ProfileTierLevel"
Cohesion: 0.10
Nodes (7): H266ProfileTierLevel, .FALLBACK, H266Tier, High, Main, test_h266_profile_tier_level_from_fmtp(), test_h266_profile_tier_level_new()

### Community 103 - ".do_poll"
Cohesion: 0.16
Nodes (19): close_is_reported_before_reset_complete_when_events_are_batched(), connect_client_server(), dcep_receiver_applies_reliability_params(), entry_by_id_mut(), entry_index(), err_stream_already_exist_in_band_returns_close(), insert_entry(), negotiated_reuse_does_not_report_open_for_old_closed_stream() (+11 more)

### Community 104 - "h265.rs"
Cohesion: 0.06
Nodes (28): ANNEXB_NALUSTART_CODE, H265FRAGMENTATION_UNIT_HEADER_SIZE, H265NALU_AGGREGATION_PACKET_TYPE, H265NALU_AUD_NALU_TYPE, H265NALU_BLA_N_LP, H265NALU_BLA_W_LP, H265NALU_BLA_W_RADL, H265NALU_CRA_NUT (+20 more)

### Community 105 - "twcc.rs"
Cohesion: 0.24
Nodes (20): empty_twcc(), negative_deltas(), register_write_parse_large_delta(), register_write_parse_mixed_delta(), register_write_parse_small_delta(), register_write_parse_small_delta_missing(), report_padded_to_even_word(), report_truncated_to_max_byte_size() (+12 more)

### Community 106 - "annexb.rs"
Cohesion: 0.10
Nodes (27): access_unit_to_avcc(), annexb(), AUD, classify(), emulation_prevention_bytes_are_not_start_codes(), find_start_code(), first_start(), IDR (+19 more)

### Community 108 - "RenderPath"
Cohesion: 0.12
Nodes (5): Renderer, RenderPath, Auto, Upload, ZeroCopy

### Community 109 - "console.rs"
Cohesion: 0.11
Nodes (8): Audio, CAPTURE_REPLY_TIMEOUT, FrameStats, HIDDEN_GRACE, KEY_GAP, NOTICE_TIME, read_video_info(), Stream

### Community 110 - "zerocopy.rs"
Cohesion: 0.15
Nodes (7): decode_fixture(), device(), iosurface_planes_are_sampled_without_copy(), Keep, read_texel(), TextureCache, textures_outlive_the_frame()

### Community 111 - "actions.rs"
Cohesion: 0.10
Nodes (19): CHUNK_CHARS, chunks(), chunks_break_after_newlines_and_respect_utf8(), combo(), combo_presses_then_releases_in_reverse(), COMBOS, CONFIRM_CHARS, confirm_reason() (+11 more)

### Community 112 - "shell.rs"
Cohesion: 0.13
Nodes (11): base64(), check_path(), DEFAULT_TIMEOUT, end_marker_skips_the_echoed_command(), find_end(), MAX_LINE, Output, PATH (+3 more)

### Community 113 - "Extension"
Cohesion: 0.11
Nodes (6): RtpHeader, ExtensionMap, remap_exts_audio(), remap_exts_illegal(), remap_exts_swaparoo(), remap_exts_video()

### Community 114 - "PayloadParams"
Cohesion: 0.07
Nodes (9): collision_preserves_configured_primary_and_rtx_pts(), dynamic_comfort_noise_collision_preserves_configured_pt(), h265_remote_no_fmtp(), h265_remote_profile_only(), h266_remote_no_fmtp(), h266_remote_profile_only(), PayloadParams, .EXACT_MATCH_SCORE (+1 more)

### Community 115 - "FormatParam"
Cohesion: 0.14
Nodes (23): FormatParam, Apt, BarePtList, BareRange, H265ProfileTierLevel, H266ProfileTierLevel, LevelAsymmetryAllowed, LevelIdx (+15 more)

### Community 116 - "TargetOs"
Cohesion: 0.14
Nodes (9): agent_name(), install_command(), install_commands_carry_the_code_and_fit_the_run_dialog(), TargetOs, .ALL, Bios, Linux, MacOs (+1 more)

### Community 118 - "super"
Cohesion: 0.09
Nodes (14): crop_png(), crops_by_fraction(), EdidResult, CapabilityResult, VencMode, .ALL, Normal, Smart (+6 more)

### Community 119 - "DepacketizingBuffer"
Cohesion: 0.16
Nodes (6): DepacketizingBuffer, Entry, LastProcessed, Advanced, Expired, TestDepack

### Community 120 - "io/mod.rs"
Cohesion: 0.13
Nodes (9): DatagramRecv, DatagramRecvInner, Dtls, Rtcp, Rtp, Stun, MAX_DTLS_OVERHEAD, MAX_RTP_OVERHEAD (+1 more)

### Community 121 - "detector.rs"
Cohesion: 0.11
Nodes (26): IntervalBudget, .WINDOW, test_alr_hysteresis_thresholds(), test_budget_ratio_clamped(), test_can_build_up_underuse_false(), test_can_build_up_underuse_true(), test_debt_recovery(), test_increase_budget_accumulates() (+18 more)

### Community 122 - "Reason"
Cohesion: 0.09
Nodes (18): Reason, BweDelayControl, BweProbeControl, BweProbeEstimator, Channel, DTLS, Feedback, Ice (+10 more)

### Community 123 - "Codec"
Cohesion: 0.09
Nodes (17): Codec, Av1, CN, G722, H264, H265, H266, Null (+9 more)

### Community 124 - "packet/tele.rs"
Cohesion: 0.14
Nodes (7): depacketizer_emits_one_report_at_a_time(), depacketizer_rejects_empty_and_partial_reports(), NEXT, payload_ignores_reserved_bit_and_masks_volume(), REPORT, REPORT_LEN, TelephoneEventDepacketizer

### Community 126 - "fuzz.rs"
Cohesion: 0.14
Nodes (7): depack(), rtp_header(), rtp_packet(), rng_primary!, Rng, random_config(), random_extmap()

### Community 127 - ".toolbar"
Cohesion: 0.15
Nodes (6): Confirm, BridgeSetup, Click, Power, Session, Type

### Community 128 - "Community 128"
Cohesion: 0.14
Nodes (14): (), CipherCtx, HMAC_KEY_LEN, HMAC_TAG_LEN, IV_LEN, KEY_LEN, RTCP_AAD_LEN, rtcp_iv() (+6 more)

### Community 129 - "HidCommand"
Cohesion: 0.13
Nodes (9): HidSender, HidCommand, Button, Key, MoveAbs, ReleaseAll, Wheel, wheel_delta() (+1 more)

### Community 130 - "RtcpFb"
Cohesion: 0.05
Nodes (32): BITRATE_MAX, MANTISSA_MAX, Remb, REMB_OFFSET, test_receiver_estimated_maximum_bitrate_marshal(), test_receiver_estimated_maximum_bitrate_overflow(), test_receiver_estimated_maximum_bitrate_truncate(), test_receiver_estimated_maximum_bitrate_unmarshal() (+24 more)

### Community 131 - "PacketChunk"
Cohesion: 0.15
Nodes (13): build_interims(), ChunkInterim, Missing, Received, PacketChunk, VectorDouble, VectorSingle, PacketStatus (+5 more)

### Community 133 - "sctp/mod.rs"
Cohesion: 0.12
Nodes (10): DEFAULT_MAX_BUFFERED_AMOUNT, DEFAULT_REMOTE_MAX_MESSAGE_SIZE, malformed_remote_snap_does_not_disable_local_opt_in(), max_payload_size_matches_mtu_minus_overhead(), partial_snap_init_requires_both_chunks(), ppi_adjust_buf(), SCTP_OVERHEAD, STREAM_OPEN_RETRY_INTERVAL (+2 more)

### Community 134 - "pion.rs"
Cohesion: 0.14
Nodes (16): add_flexfec(), config_message(), decode_sdp(), encode_sdp(), fec_level(), negotiate(), parse_signal(), sdp_travels_as_base64_json() (+8 more)

### Community 135 - "Msg"
Cohesion: 0.09
Nodes (16): entry_name(), file(), home(), list(), read(), roots(), Browser, size() (+8 more)

### Community 136 - "app/control.rs"
Cohesion: 0.20
Nodes (13): App, button(), key_name(), num(), ok(), Out, Later, Now (+5 more)

### Community 137 - "TwccSendRecord"
Cohesion: 0.19
Nodes (8): no_acked_duplicates_when_reordered(), Run, test_send_register_apply_report_for_old_seq_numbers(), test_twcc_register_send_records(), test_twcc_send_register_loss(), TwccRecvReport, TwccSendRecord, TwccSendRegister

### Community 138 - "layout/mod.rs"
Cohesion: 0.16
Nodes (15): ALTGR, altgr_is_right_alt_alone(), control_characters_and_missing(), every_table_is_sorted_and_has_the_basics(), german_windows_altgr_and_swapped_keys(), Keys, Layout, layout() (+7 more)

### Community 139 - "contiguity_vp8.rs"
Cohesion: 0.15
Nodes (15): Contiguity, None, Vp8, Vp9, FrameContiguityState, contiguous_l1t1_no_l1_contig_l0(), contiguous_l1t2(), contiguous_l1t3() (+7 more)

### Community 140 - "H264LevelIdc"
Cohesion: 0.10
Nodes (18): H264LevelIdc, Level1, Level1_1, Level1_2, Level1_3, Level1B, Level2, Level2_1 (+10 more)

### Community 141 - "Extension"
Cohesion: 0.08
Nodes (16): Extension, AbsoluteCaptureTime, AbsoluteSendTime, AudioLevel, ColorSpace, FrameMarking, PlayoutDelay, RepairedRtpStreamId (+8 more)

### Community 142 - "CodecDepacketizer"
Cohesion: 0.20
Nodes (15): CodecDepacketizer, Av1, Boxed, ComfortNoise, G711, H264, H265, H266 (+7 more)

### Community 143 - "Event"
Cohesion: 0.05
Nodes (40): INSTANCE_PORT, BURST, dropped(), IO_TIMEOUT, MAX_REQUEST, Note, read_note(), REFILL (+32 more)

### Community 144 - "SdesType"
Cohesion: 0.10
Nodes (17): pad_bytes_to_word(), cname_serialize_deserialize(), Descriptions, Sdes, SdesType, CNAME, EMAIL, END (+9 more)

### Community 145 - "SctpInitData"
Cohesion: 0.14
Nodes (4): LOCAL_MAX_MESSAGE_SIZE, b64_encode(), SctpInitData, webrtc_transport_config()

### Community 146 - "Secret"
Cohesion: 0.09
Nodes (14): load_secret(), remove_secret(), save_secret(), secret_path(), ALPHABET, code_round_trip(), Secret, equals_inside_value_and_unbalanced_quote() (+6 more)

### Community 147 - "DataSize"
Cohesion: 0.17
Nodes (10): AckedBitrateEstimator, ESTIMATE_FLOOR, SMALL_SAMPLE_THRESHOLD, SMALL_SAMPLE_UNCERTAINTY, test_correct_estimate_after_initial_window(), test_long_gap_preserves_overflow_via_modulo_after_initial(), test_no_estimate_before_initial_window_has_passed(), UNCERTAINTY (+2 more)

### Community 148 - "event.rs"
Cohesion: 0.15
Nodes (4): SdpSimulcastLayer, Simulcast, SimulcastLayer, SimulcastLayerBuilder

### Community 150 - "volume.rs"
Cohesion: 0.15
Nodes (7): endpoint(), exec(), osa(), out(), percent(), read(), set()

### Community 151 - "pipeline.rs"
Cohesion: 0.15
Nodes (5): MediaControl, Decoded, FramePipeline, KEYFRAME_REQUEST_MIN_INTERVAL, Shared

### Community 152 - "steps"
Cohesion: 0.16
Nodes (19): runs-on, steps, RUSTDOCFLAGS, cargo-deny, pii, snowflake, steps, runs-on (+11 more)

### Community 153 - "steps"
Cohesion: 0.11
Nodes (18): env, runs-on, steps, timeout-minutes, FUZZ_TIMEOUT, jobs, cargo-fuzz, name (+10 more)

### Community 154 - "H266Level"
Cohesion: 0.12
Nodes (16): H266Level, Level1, Level15_5, Level2, Level2_1, Level3, Level3_1, Level4 (+8 more)

### Community 156 - "ci.yml"
Cohesion: 0.11
Nodes (18): config_dir(), runs-on, steps, env, CARGO_TERM_COLOR, jobs, deny, test (+10 more)

### Community 157 - "keymap.rs"
Cohesion: 0.18
Nodes (7): all_key_codes(), code_name(), is_meta_code(), names_are_wire_safe(), names_match_firmware_when_available(), SAMPLE, translate()

### Community 158 - "str0m/src/config.rs"
Cohesion: 0.21
Nodes (8): BweConfig, default_mtu_is_datagram_mtu_target(), enable_comfort_noise_updates_codec_config(), set_mtu_accepts_equal_bounds(), set_mtu_panics_on_inverted_range(), set_mtu_panics_on_target_above_max(), set_mtu_panics_on_target_below_min(), set_mtu_round_trips()

### Community 159 - "with"
Cohesion: 0.26
Nodes (17): L_CRYPTO, R_CRYPTO, steps, steps, steps, steps, env, id (+9 more)

### Community 160 - "Signal"
Cohesion: 0.17
Nodes (8): Signal, Normal, Overuse, Underuse, State, Decrease, Hold, Increase

### Community 161 - "Direction"
Cohesion: 0.12
Nodes (5): Direction, Inactive, RecvOnly, SendOnly, SendRecv

### Community 162 - "contiguity_vp9.rs"
Cohesion: 0.27
Nodes (12): contiguous_l1t1_no_l1_contig_l0(), contiguous_l1t2(), contiguous_l1t3(), contiguous_l1t3_no_l2_contig_l0(), contiguous_l1t3_no_l2_discontig_l0(), FrameContiguityState, get_codec_extra(), keyframe_accepts_restarted_picture_ids() (+4 more)

### Community 163 - "H265Level"
Cohesion: 0.13
Nodes (14): H265Level, Level1, Level2, Level2_1, Level3, Level3_1, Level4, Level4_1 (+6 more)

### Community 164 - "TwccSeq"
Cohesion: 0.14
Nodes (6): TwccSeq, Receipt, ReceiveWindow, TwccPacketId, TwccRecvRegister, TwccSendRecordsIter

### Community 165 - "Twcc"
Cohesion: 0.16
Nodes (3): Twcc, TwccIter, TwccSendRecordsIter<'a>

### Community 166 - "ext.rs"
Cohesion: 0.13
Nodes (14): abs_capture_time_extended_form(), abs_capture_time_short_form(), abs_capture_time_two_byte_form(), abs_send_time(), abs_send_time_two_byte_form(), audio_level_round_trip(), EXT_URI, IdHasher (+6 more)

### Community 168 - "get_h266"
Cohesion: 0.22
Nodes (24): get_h265(), get_h266(), h265_config(), h265_remote_ptl(), h266_config(), h266_remote_ptl(), test_h265_no_fmtp_remote_clears(), test_h265_profile_id_cleared_on_ptl_match() (+16 more)

### Community 169 - "buffer_rx.rs"
Cohesion: 0.23
Nodes (15): end_on_defacto(), end_on_marker(), gap_after_emit(), gap_after_padding(), late_packet_in_front_of_waiting_frame(), MAX_BUFFERED_PACKETS, out_of_order_packets_with_hold_0(), packets_after_hold_out() (+7 more)

### Community 170 - "Info"
Cohesion: 0.13
Nodes (12): invalidate(), Probe, session(), Battery, Display, Focus, Info, Load (+4 more)

### Community 171 - "FormatParams"
Cohesion: 0.20
Nodes (7): FormatParams, .DEFAULT_TELEPHONE_EVENT_MAX, h265_combined_params_roundtrip(), h266_combined_params_roundtrip(), sprop_max_don_diff_roundtrip(), sprop_stereo_roundtrip(), telephone_event_ranges_roundtrip()

### Community 172 - "link_capacity_estimator.rs"
Cohesion: 0.28
Nodes (8): estimate_expires_after_reset_window(), ignores_invalid_probes(), LinkCapacityEstimator, .DEFAULT_RESET_WINDOW, reset_clears_estimate(), starts_with_no_estimate(), stores_probe_result(), takes_maximum_of_multiple_probes()

### Community 173 - "CFRef"
Cohesion: 0.25
Nodes (14): CFArrayGetCount(), CFArrayGetValueAtIndex(), CFDictionaryCreate(), CFRef, CFRelease(), CFStringCreateWithCString(), CFStringGetCString(), exec() (+6 more)

### Community 174 - "VideoChange"
Cohesion: 0.15
Nodes (13): Video, VideoChange, Latency, Mode, Orientation, Quality, Venc, Orientation (+5 more)

### Community 175 - "read"
Cohesion: 0.28
Nodes (13): battery(), ime(), keyboards(), load(), memory(), read(), root_disk(), run() (+5 more)

### Community 176 - "PinStore"
Cohesion: 0.16
Nodes (11): app_dir(), changed_cert_is_rejected(), check_pin(), first_contact_pins_then_matches(), persists_and_reloads(), PinCheck, Match, Mismatch (+3 more)

### Community 177 - "gen_icon.swift"
Cohesion: 0.21
Nodes (6): AppKit, CoreGraphics, draw(), gradient(), png(), rgb()

### Community 179 - "device.rs"
Cohesion: 0.17
Nodes (10): audio_arrives(), direct_video_decodes_in_hardware(), event_socket_connects_and_answers_pings(), first_keyframe(), janus_delivers_a_keyframe(), janus_delivers_audio(), login(), pion_delivers_a_keyframe() (+2 more)

### Community 180 - "SctpError"
Cohesion: 0.10
Nodes (9): DcepAck, DcepOpen, SctpError, DcepBadUtf8, DcepIncorrectMessageType, DcepOpenTooSmall, InvalidSnap, Proto (+1 more)

### Community 181 - "ChannelConfig"
Cohesion: 0.17
Nodes (7): ChannelConfig, DcepOpen, Reliability, MaxPacketLifetime, MaxRetransmits, Reliable, (ReliabilityType, u32)

### Community 182 - "H264Profile"
Cohesion: 0.14
Nodes (14): H264Profile, Baseline, CAVLC444Intra, ConstrainedBaseline, ConstrainedHigh, Extended, High, High10 (+6 more)

### Community 183 - "test_client.py"
Cohesion: 0.32
Nodes (5): auth_token(), main(), recv_exact(), recv_frame(), send_frame()

### Community 184 - "gen-layouts.py"
Cohesion: 0.26
Nodes (6): fetch(), level(), main(), parse(), rust_char(), unescape()

### Community 185 - "cargo.yml"
Cohesion: 0.15
Nodes (12): concurrency, cancel-in-progress, group, env, TOOLCHAINS, name, on, pull_request (+4 more)

### Community 186 - "glkvm-rs-native-macos-console-for-gl-inet-comet-pl.md"
Cohesion: 0.29
Nodes (6): Constraints, Goal, References, Status, Success Criteria, Why Now

### Community 187 - "H265AggregationPacket"
Cohesion: 0.18
Nodes (3): H265AggregationPacket, H265AggregationUnit, H265AggregationUnitFirst

### Community 188 - "H265PACIPacket"
Cohesion: 0.12
Nodes (3): H265PACIPacket, H265TSCI, test_h265_paci_roundtrip_with_tsci()

### Community 189 - "H265Profile"
Cohesion: 0.15
Nodes (12): H265Profile, FormatRangeExtensions, HighThroughput, HighThroughputScreenContentCoding, Main, Main10, MainStillPicture, MultiviewMain (+4 more)

### Community 190 - "capabilities.rs"
Cohesion: 0.08
Nodes (22): Capabilities, Enabled, Feature, Atx, H264, H265, Hid, Msd (+14 more)

### Community 191 - "bench_app.py"
Cohesion: 0.24
Nodes (4): cpu_times(), main(), parse_cputime(), top_power()

### Community 192 - "PowerAction"
Cohesion: 0.12
Nodes (13): AtxButton, Power, PowerLong, Reset, AtxLeds, AtxState, PowerAction, Off (+5 more)

### Community 193 - ".new"
Cohesion: 0.15
Nodes (7): derive_key(), Derived, AeadAes128Gcm, AeadAes256Gcm, Aes128CmSha1_80, SrtpKey, SrtpKey<ML, SL>

### Community 194 - "jobs"
Cohesion: 0.17
Nodes (11): concurrency, cancel-in-progress, group, env, TOOLCHAINS, jobs, platform-build-test, name (+3 more)

### Community 195 - "wol.rs"
Cohesion: 0.17
Nodes (5): Mac, mac_formats(), Session, WolDevice, WolList

### Community 196 - "source.rs"
Cohesion: 0.11
Nodes (10): EncodedFrame, MediaSession, QUEUE_DEPTH, SourceOptions, start(), Transport, .ALL, Direct (+2 more)

### Community 197 - "src/receive.rs"
Cohesion: 0.06
Nodes (25): Job, Outbox, FILE_CHUNK, MAX_CLIP_FILES, MAX_CLIP_FILES_BYTES, Purpose, Clipboard, Save (+17 more)

### Community 198 - "super"
Cohesion: 0.19
Nodes (5): ComfortNoiseDepacketizer, ComfortNoisePacketizer, depacketizer_rejects_empty_cn_payload(), packetizer_does_not_emit_empty_cn_payload(), packetizer_rejects_payload_larger_than_mtu()

### Community 199 - "json"
Cohesion: 0.24
Nodes (4): Client, default_socket(), launch(), main()

### Community 200 - "matrix"
Cohesion: 0.18
Nodes (11): name, os, name, os, test, crypto, dir, rust (+3 more)

### Community 201 - "matrix"
Cohesion: 0.22
Nodes (11): mobile-build, strategy, l_crypto, r_crypto, name, runs-on, strategy, timeout-minutes (+3 more)

### Community 203 - "packet/mod.rs"
Cohesion: 0.13
Nodes (18): Av1CodecExtra, detect_av1_keyframe(), detect_h264_keyframe(), H264CodecExtra, H265CodecExtra, H266CodecExtra, CodecExtra, Av1 (+10 more)

### Community 204 - "UserEvent"
Cohesion: 0.13
Nodes (13): UserEvent, ClipSecret, Connected, Control, Frame, Notice, Password, Refresh (+5 more)

### Community 205 - ".depacketize"
Cohesion: 0.17
Nodes (18): H265Depacketizer, H265Payload, H265AggregationPacket, H265FragmentationUnitPacket, H265PACIPacket, H265SingleNALUnitPacket, test_h265_aggregation_packet(), test_h265_donl_aggregation_round_trip() (+10 more)

### Community 206 - "H266Profile"
Cohesion: 0.18
Nodes (9): H266Profile, Main10, Main10_444, Main10_444StillPicture, Main10StillPicture, MultilayerMain10, MultilayerMain10_444, MultilayerMain10_444StillPicture (+1 more)

### Community 208 - "SctpEvent"
Cohesion: 0.18
Nodes (8): SctpEvent, AssociationLost, BufferedAmountLow, Close, Data, Open, StreamResetComplete, Transmit

### Community 209 - "smoother.rs"
Cohesion: 0.33
Nodes (5): ESTIMATE_WINDOW, EstimateSmoother, in_tolerance(), TimeBitrate, TOLERANCE

### Community 210 - "SrtpProfile"
Cohesion: 0.20
Nodes (6): SrtpProfile, AeadAes128Gcm, AeadAes256Gcm, Aes128CmSha1_80, .ALL, PassThrough

### Community 211 - "RtpMeta"
Cohesion: 0.20
Nodes (3): Depacketized, ext_vals_extracts_from_first_and_last_packet(), RtpMeta

### Community 212 - "H264ProfileIdc"
Cohesion: 0.20
Nodes (9): H264ProfileIdc, X2C, X42, X4D, X58, X64, X6E, X7A (+1 more)

### Community 216 - "Delta"
Cohesion: 0.27
Nodes (6): &'a TwccSendRegister, Delta, Large, Small, test_twcc_iter_correct_deltas(), test_twcc_iter_limited_with_status_count()

### Community 217 - "Pion WebRTC+FlexFEC transport (/pion/ws)"
Cohesion: 0.23
Nodes (12): M7 Pion+FlexFEC under loss results, Opus audio over Pion/Janus, gl-pion FlexFEC-03 repair layout, Pion/Janus signaling (base64 SDP), U3: streamer lifecycle, Opus audio playback (opus-decoder + cpal), FlexFEC-03 decode (glkvm-media/flexfec.rs), glkvm-media video/audio sources (+4 more)

### Community 219 - "Clipboard bridge design"
Cohesion: 0.33
Nodes (5): glkvm-agent on target, Insert clipboard vs type (protocol v2), Noise NNpsk0 bridge protocol (glkvm-clip), glkvm-bridge relay (kvmd extra), Type clipboard (hid/print)

### Community 220 - "glkvm-rs native console project"
Cohesion: 0.40
Nodes (4): Login / auth (URL-encoded, 2FA, two-step), Capability gating by model/API, glkvm::client REST client, Trust-on-first-use TLS cert pinning

### Community 221 - "Binary HID frame encoding (U1)"
Cohesion: 0.22
Nodes (7): U1: HID binary frames on /api/ws, HID input coalescing, glkvm::events (/api/ws), Binary HID frame encoding (U1), Single ordered HID queue, Overlook prior art (Swift client), release_all() safety behavior

### Community 222 - "fmt.rs"
Cohesion: 0.20
Nodes (3): NetError, Io, Stun

### Community 223 - "jobs"
Cohesion: 0.22
Nodes (9): runs-on, steps, jobs, fuzz-build, lint, related-crates, runs-on, runs-on (+1 more)

### Community 224 - "with"
Cohesion: 0.22
Nodes (9): with, arguments, check_diff, command, components, fetch-depth, line_width_rules, persist-credentials (+1 more)

### Community 225 - "ExtensionsForm"
Cohesion: 0.28
Nodes (3): ExtensionsForm, OneByte, TwoByte

### Community 226 - "asyncio"
Cohesion: 0.50
Nodes (4): client(), main(), recv_agent(), recv_client()

### Community 227 - "PowerAction"
Cohesion: 0.10
Nodes (18): command(), run(), check(), install(), prepare(), RESTARTED, swap(), Request (+10 more)

### Community 228 - "ethernet"
Cohesion: 0.22
Nodes (8): ethernet, driver, enabled, host_mac, kvm_mac, start, otg, devices

### Community 229 - "cometdesk-media"
Cohesion: 0.43
Nodes (8): cometdesk, cometdesk-agent, cometdesk-cli, cometdesk-clip, cometdesk-gui, cometdesk-media, cometdesk-view, str0m

### Community 230 - "str0m Sans-IO WebRTC library"
Cohesion: 0.40
Nodes (6): str0m 0.24.1 changelog, str0m crypto backends, str0m RTP mode API, Sans-IO design pattern, Single-mutation / drain run-loop invariant, str0m Sans-IO WebRTC library

### Community 231 - "H264ProfileLevel"
Cohesion: 0.39
Nodes (3): H264ProfileLevel, .FALLBACK, test_profile_level_parsing()

### Community 233 - "value_history.rs"
Cohesion: 0.22
Nodes (4): DEFAULT_VALUE_HISTORY_DURATION, test(), ValueHistory<T>, with_value_test()

### Community 234 - "ClipDirection"
Cohesion: 0.25
Nodes (5): ClipDirection, .ALL, Both, FromTarget, ToTarget

### Community 235 - "WheelAccumulator"
Cohesion: 0.32
Nodes (3): trackpad_pixels_accumulate(), wheel_lines_are_whole_steps(), WheelAccumulator

### Community 236 - "Av1Packetizer"
Cohesion: 0.27
Nodes (5): Av1Packetizer, leb_128_size(), Obu, Packet, parse_obus()

### Community 238 - "glkvm-gui eframe/egui app"
Cohesion: 0.29
Nodes (7): Phase 2 console app CPU/energy results, GUI control socket (GLKVM_CONTROL), glkvmctl.py automation client, U2: Direct video /api/media/ws frames, Direct H.264 transport (/api/media/ws), glkvm-gui eframe/egui app, Zero-copy IOSurface/Metal rendering

### Community 239 - "manifest.yaml"
Cohesion: 0.33
Nodes (5): daemon, description, name, path, place

### Community 240 - "annexB"
Cohesion: 0.29
Nodes (3): Foundation, annexB(), VideoToolbox

### Community 241 - "BitPattern"
Cohesion: 0.40
Nodes (3): .PROFILES, BitPattern, test_iop_pattern_matching()

### Community 242 - "RtcSctpState"
Cohesion: 0.33
Nodes (5): RtcSctpState, AwaitAssociationEstablished, AwaitRemoteAssociation, Established, Uninited

### Community 244 - "linux-crypto-test"
Cohesion: 0.40
Nodes (5): linux-crypto-test, name, runs-on, strategy, timeout-minutes

### Community 245 - "windows-crypto-test"
Cohesion: 0.40
Nodes (5): windows-crypto-test, name, runs-on, strategy, timeout-minutes

### Community 246 - "platform"
Cohesion: 0.40
Nodes (5): platform, features, name, os, target

### Community 247 - "glkvm-rs native console project"
Cohesion: 0.33
Nodes (6): Clipboard bridge design, RNDIS USB Ethernet transport, GLKVM KVMD device firmware, glkvm-rs native console project, PiKVM (upstream of GLKVM), RustDesk/rdev prior art

### Community 250 - "ProbeResult"
Cohesion: 0.17
Nodes (10): ProbeResult, ClusterTooSmall, InsufficientBytes, InsufficientPackets, InvalidDataSize, InvalidSendReceiveRatio, MissingTimingInfo, RecvIntervalInvalid (+2 more)

### Community 251 - "BufferedThresholdConfig"
Cohesion: 0.50
Nodes (4): BufferedThresholdConfig, Configured, Desired, Unconfigured

### Community 254 - "build-agents.sh"
Cohesion: 0.67
Nodes (3): build_linux(), CARGO_PROFILE_RELEASE_STRIP, build-agents.sh script

### Community 255 - "macos-crypto-test"
Cohesion: 0.50
Nodes (4): macos-crypto-test, name, runs-on, timeout-minutes

### Community 256 - "on"
Cohesion: 0.50
Nodes (4): on, schedule, workflow_dispatch, cron

### Community 257 - ".try_from"
Cohesion: 0.27
Nodes (7): DatagramRecv<'a>, MultiplexKind, Dtls, Rtcp, Rtp, Stun, Receive<'a>

### Community 259 - "VideoFrame"
Cohesion: 0.20
Nodes (6): decodes_keyframe_then_delta_synchronously(), delta_frame_without_parameter_sets_needs_keyframe(), fixture(), Plane, same_parameter_sets_reuse_the_session(), VideoFrame

### Community 260 - "Measurement targets M1-M7"
Cohesion: 0.67
Nodes (3): Phase 0 baseline measurements, Measurement targets M1-M7, Phases 0-3 roadmap

### Community 345 - "PacerReason"
Cohesion: 0.22
Nodes (9): PacerReason, FirstEver, Handle, Immediate, Paced, Padding, Probe1, Probe2 (+1 more)

### Community 435 - "VtDecoder"
Cohesion: 0.26
Nodes (5): DecodeError, NeedKeyframe, Os, Sink, VtDecoder

### Community 443 - "Volume"
Cohesion: 0.50
Nodes (3): CACHE, Slow, Volume

### Community 444 - "ObuType"
Cohesion: 0.18
Nodes (10): ObuType, Frame, FrameHeader, MetaData, Padding, RedundantFrameHeader, SequenceHeader, TemporalDelimiter (+2 more)

### Community 461 - "EdidPreset"
Cohesion: 0.29
Nodes (3): EdidPreset, normalize(), Session

### Community 462 - "layout/data.rs"
Cohesion: 0.38
Nodes (4): CODES, LAYOUTS, S, Table

### Community 465 - "ConsoleAction"
Cohesion: 0.40
Nodes (5): ConsoleAction, Disconnect, None, TargetChanged, ToggleFullscreen

## Knowledge Gaps
- **803 isolated node(s):** `Windows`, `Linux`, `MacOs`, `Bios`, `Both` (+798 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 2602 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **235 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `()` connect `Community 128` to `SrtpProfile`, `Fingerprint`, `fmt.rs`, `SrtpCrypto`?**
  _High betweenness centrality (0.024) - this node is a cross-community bridge._
- **Are the 2 inferred relationships involving `Console` (e.g. with `Browser` and `FileSend`) actually correct?**
  _`Console` has 2 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Windows`, `Linux`, `MacOs` to the rest of the system?**
  _803 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `H266 Packetizer` be split into smaller, more focused modules?**
  _Cohesion score 0.06401799100449775 - nodes in this community are weakly interconnected._
- **Why does `DataSize` connect `DataSize` to `arrival_group.rs`, `SendSideBandwidthEstimator`, `Probe Estimator`, `TwccClusterId`, `Loss Controller`, `ProbeClusterConfig`, `str0m/src/lib.rs`, `Pacer Tests`, `detector.rs`, `ProbeResult`, `Time Types`?**
  _High betweenness centrality (0.018) - this node is a cross-community bridge._
- **Should `Stream TX/Mid` be split into smaller, more focused modules?**
  _Cohesion score 0.05217391304347826 - nodes in this community are weakly interconnected._
- **Why does `Frequency` connect `Media Time` to `Stream TX/Mid`, `Media Model`, `SDP Parser`, `Pt`, `ext.rs`, `CodecConfig`, `payload_params.rs`, `buffer_rx.rs`, `MediaAttribute`, `SDP Data`, `SeqNo`, `str0m/src/lib.rs`, `super`, `codec.rs`, `packet/tele.rs`, `fuzz.rs`, `VideoRenderer`?**
  _High betweenness centrality (0.016) - this node is a cross-community bridge._