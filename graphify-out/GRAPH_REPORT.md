# Graph Report - glkvm-rs  (2026-10-29)

## Corpus Check
- 249 files · ~392,950 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 19 file(s) not represented in the graph (top: (none) 6, .toml 5, .conf 4)

## Summary
- 7296 nodes · 16517 edges · 451 communities (225 shown, 226 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 217 edges (avg confidence: 0.74)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `6dcd83d9`
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
- StreamTxStats
- client.rs
- bridge.rs
- Rtc Core
- SDP Data
- Loss Controller
- clipsync.rs
- Crypto Config
- time_tricks.rs
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
- instant
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
- session.rs
- viewer.rs
- rtp/header.rs
- SrtpCrypto
- vt.rs
- MediaLine
- red.rs
- app.rs
- DevicesUi
- Pt
- theme.rs
- sdp/mod.rs
- payload_params.rs
- MediaKind
- vla.rs
- super
- ReportList
- Changes
- vp9.rs
- .push
- mock_device.rs
- ProbeClusterConfig
- not_happening
- DelayController
- super
- Error
- janus.rs
- Media
- .new
- SessionAttribute
- insert.rs
- RtcSctp
- VideoRenderer
- cometdesk_bridge.py
- layout/data.rs
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
- VideoMode
- DepacketizingBuffer
- io/mod.rs
- detector.rs
- Reason
- Codec
- packet/tele.rs
- fuzz.rs
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
- BitRead
- Event
- CodecDepacketizer
- SctpInitData
- Secret
- DataSize
- event.rs
- PacketError
- pipeline.rs
- steps
- steps
- H266Level
- Bridge
- ci.yml
- keymap.rs
- Fingerprint
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
- RestrictionId
- FormatParams
- link_capacity_estimator.rs
- .notify
- VideoChange
- H265FragmentationUnitHeader
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
- notify.rs
- bench_app.py
- PowerAction
- .new
- jobs
- wol.rs
- .update_params
- src/receive.rs
- Av1Packetizer
- json
- matrix
- matrix
- pacer/control.rs
- UserEvent
- H265Payload
- H266Profile
- psfbapp.rs
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
- ObuType
- Clipboard bridge design
- glkvm-rs native console project
- Binary HID frame encoding (U1)
- .empty
- jobs
- with
- ExtensionsForm
- asyncio
- Feature
- ethernet
- cometdesk-media
- str0m Sans-IO WebRTC library
- H264ProfileLevel
- H265Depacketizer
- value_history.rs
- ClipDirection
- WheelAccumulator
- ExtensionValues
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
- Serializer
- BufferedThresholdConfig
- ConsoleAction
- api/mod.rs
- build-agents.sh
- macos-crypto-test
- on
- Instant
- Av1Depacketizer
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

## God Nodes (most connected - your core abstractions)
1. `Console` - 118 edges
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
- `Login / auth (URL-encoded, 2FA, two-step)` --references--> `glkvm::client REST client`  [INFERRED]
  docs/protocol.md → PLAN.md
- `GUI control socket (GLKVM_CONTROL)` --references--> `glkvm-gui eframe/egui app`  [INFERRED]
  docs/automation.md → PLAN.md

## Import Cycles
- 1-file cycle: `apps/cometdesk-agent/src/main.rs -> apps/cometdesk-agent/src/main.rs`
- 1-file cycle: `vendor/str0m/src/lib.rs -> vendor/str0m/src/lib.rs`
- 1-file cycle: `crates/cometdesk/src/layout/mod.rs -> crates/cometdesk/src/layout/mod.rs`
- 1-file cycle: `vendor/str0m/src/bwe/time.rs -> vendor/str0m/src/bwe/time.rs`
- 1-file cycle: `vendor/str0m/src/rtp/rtcp/list.rs -> vendor/str0m/src/rtp/rtcp/list.rs`
- 1-file cycle: `vendor/str0m/src/rtp/rtcp/twcc.rs -> vendor/str0m/src/rtp/rtcp/twcc.rs`
- 1-file cycle: `vendor/str0m/src/pacer/leaky.rs -> vendor/str0m/src/pacer/leaky.rs`
- 1-file cycle: `vendor/str0m/src/crypto/srtp.rs -> vendor/str0m/src/crypto/srtp.rs`
- 1-file cycle: `vendor/str0m/src/streams/register.rs -> vendor/str0m/src/streams/register.rs`
- 1-file cycle: `vendor/str0m/src/bwe/loss_controller.rs -> vendor/str0m/src/bwe/loss_controller.rs`
- 1-file cycle: `vendor/str0m/src/packet/contiguity_vp8.rs -> vendor/str0m/src/packet/contiguity_vp8.rs`
- 1-file cycle: `vendor/str0m/src/packet/contiguity_vp9.rs -> vendor/str0m/src/packet/contiguity_vp9.rs`
- 1-file cycle: `vendor/str0m/src/streams/register_nack.rs -> vendor/str0m/src/streams/register_nack.rs`
- 1-file cycle: `vendor/str0m/src/bwe/delay/rate_control.rs -> vendor/str0m/src/bwe/delay/rate_control.rs`
- 1-file cycle: `vendor/str0m/src/bwe/delay/arrival_group.rs -> vendor/str0m/src/bwe/delay/arrival_group.rs`
- 1-file cycle: `vendor/str0m/src/util/value_history.rs -> vendor/str0m/src/util/value_history.rs`
- 1-file cycle: `vendor/str0m/src/rtp/error.rs -> vendor/str0m/src/rtp/error.rs`
- 1-file cycle: `vendor/str0m/src/bwe/macros.rs -> vendor/str0m/src/bwe/macros.rs`
- 2-file cycle: `vendor/str0m/src/bwe/delay/control.rs -> vendor/str0m/src/bwe/mod.rs -> vendor/str0m/src/bwe/delay/control.rs`
- 2-file cycle: `apps/cometdesk-gui/src/app.rs -> apps/cometdesk-gui/src/console.rs -> apps/cometdesk-gui/src/app.rs`

## Hyperedges (group relationships)
- **Clipboard bridge end-to-end pipeline** — docs_clipboard_bridge_agent, docs_clipboard_bridge_relay, docs_clipboard_bridge_noise [EXTRACTED 1.00]
- **glkvm-media transport sources** — plan_direct_h264, plan_pion_webrtc, plan_janus [EXTRACTED 1.00]

## Communities (451 total, 226 thin omitted)

### Community 0 - "H266 Packetizer"
Cohesion: 0.06
Nodes (86): annexb(), ANNEXB_NALUSTART_CODE, depacketize_all(), detect_h266_keyframe(), H266Depacketizer, H266FRAGMENTATION_UNIT_HEADER_SIZE, H266FragmentationUnitHeader, H266NALU_AGGREGATION_PACKET_TYPE (+78 more)

### Community 1 - "Stream TX/Mid"
Cohesion: 0.04
Nodes (23): RtcpReportIntervals, PacketReceipt, RX_LOOKUP_CLEANUP_INTERVAL, RX_LOOKUP_EXPIRY, RxLookup, DEFAULT_RTX_CACHE_DURATION, DEFAULT_RTX_RATIO_CAP, MIN_SPURIOUS_PADDING_SIZE (+15 more)

### Community 2 - "cometdesk-cli/src/main.rs"
Cohesion: 0.03
Nodes (73): BridgeCmd, Remove, Setup, Status, ButtonArg, Power, PowerLong, Reset (+65 more)

### Community 3 - "cometdesk-clip/src/lib.rs"
Cohesion: 0.06
Nodes (53): CHUNK, clipboard_file_streams_into_staging(), cut(), Endpoint, Error, Crypto, Frame, Handshake (+45 more)

### Community 4 - "Media Model"
Cohesion: 0.03
Nodes (41): KeyframeRequestKind, Fir, Pli, blocked_buffer(), deadlines_cover_payload_types_rids_and_resets(), MAX_PENDING_PAYLOADS, Media, MID_PROBE (+33 more)

### Community 5 - "SSRC/Streams API"
Cohesion: 0.03
Nodes (19): DirectApi<'a>, MidRid, Ssrc, Streams, StreamTimeoutConfig, LastSenderInfo, paused_timestamp_repair_moves_time_forward(), receiver_report_uses_supplied_intervals() (+11 more)

### Community 6 - "Console"
Cohesion: 0.07
Nodes (4): Layout, Console, RegionPick, Typing

### Community 7 - "Probe Control"
Cohesion: 0.10
Nodes (31): allocation_probe_fires_when_desired_increases_in_alr(), allocation_probe_is_triggered_in_alr_when_allocation_increases(), allocation_probe_requires_desired_increase_in_alr(), ALR_ENDED_TIMEOUT, BandwidthLimitedCause, DelayBasedLimited, DelayBasedLimitedDelayIncreased, LossLimitedBwe (+23 more)

### Community 8 - "register_nack.rs"
Cohesion: 0.07
Nodes (38): all_packets_have_same_frequency(), expected(), expected_overflow(), expected_received_loss(), jitter_at_0(), jitter_at_20(), jitter_rtp_timestamp_rollover(), active_window_sliding() (+30 more)

### Community 9 - "StreamTxStats"
Cohesion: 0.14
Nodes (6): Losses, Disabled, Enabled, StreamTxStats, calculate_rtt(), ValueHistory

### Community 10 - "client.rs"
Cohesion: 0.05
Nodes (24): api_message(), ConfigResult, Connection, Credentials, Enabled, Envelope, ErrorResult, header() (+16 more)

### Community 11 - "bridge.rs"
Cohesion: 0.06
Nodes (35): health_text(), agents_dir(), BridgeSetup, Inner, AGENT_NAMES, BRIDGE_VERSION, BRIDGE_WS, BridgeSide (+27 more)

### Community 12 - "Rtc Core"
Cohesion: 0.04
Nodes (18): Bwe, DirectApi, Channel, from_feature_flags(), ChannelBufferedAmountLow, ChannelClose, ChannelOpen, IceConnectionStateChange (+10 more)

### Community 13 - "SDP Data"
Cohesion: 0.06
Nodes (45): bundle_mids_returns_correct_mids(), FingerprintFmt, g722_red_emits_wire_clock_rate(), h265_and_h264_coexist(), h265_chrome_profile_only_sdp(), h265_empty_fmtp_not_serialized(), h265_in_complete_sdp(), h265_incomplete_params_all_permutations() (+37 more)

### Community 14 - "Loss Controller"
Cohesion: 0.07
Nodes (26): AsValid, ChannelParameters, Config, HoldInfo, LogNormalDistribution, loss_spike_recovery(), LossBasedBweResult, LossController (+18 more)

### Community 15 - "clipsync.rs"
Cohesion: 0.05
Nodes (54): AckRequest, AckResult, ActiveFile, BACKOFF_MAX, BACKOFF_MIN, Batch, BridgeState, AgentOutdated (+46 more)

### Community 16 - "Crypto Config"
Cohesion: 0.03
Nodes (12): BweConfig, default_mtu_is_datagram_mtu_target(), enable_comfort_noise_updates_codec_config(), RtcConfig, set_mtu_accepts_equal_bounds(), set_mtu_panics_on_inverted_range(), set_mtu_panics_on_target_above_max(), set_mtu_panics_on_target_below_min() (+4 more)

### Community 17 - "time_tricks.rs"
Cohesion: 0.08
Nodes (13): (Option<Instant>, T), Soonest, BEGINNING_OF_TIME, epoch_to_beginning(), F32, from_ntp_64_val1(), from_ntp_64_zero(), InstantExt (+5 more)

### Community 18 - "VP8 Payload"
Cohesion: 0.05
Nodes (29): KEY_IDX_MASK, packetize_respects_mtu(), PICTURE_ID_MAX, PICTURE_ID_SHORT_MASK, PICTURE_ID_SHORT_MAX, read_vp8_descriptor_byte(), rewrite_payload(), test_vp8_descriptor_exposes_fields() (+21 more)

### Community 19 - "events.rs"
Cohesion: 0.06
Nodes (33): AtxLeds, AtxState, BACKOFF_MAX, BACKOFF_MIN, ConnState, Closed, Connected, Connecting (+25 more)

### Community 20 - "SDP Negotiation"
Cohesion: 0.09
Nodes (34): add_ice_details(), AddMedia, answer_uses_negotiated_dtls_role(), apply_direct_changes(), as_new_medias_contiguous_indices_with_non_media_changes(), Change, AddApp, AddChannel (+26 more)

### Community 21 - "str0m/src/lib.rs"
Cohesion: 0.03
Nodes (49): Bwe<'a>, BweKind, Remb, Twcc, Event, AppSpecificFeedback, ChannelData, Closed (+41 more)

### Community 22 - "src/hid.rs"
Cohesion: 0.07
Nodes (33): map_button(), abs_mapping_is_monotonic(), click_flushes_pending_move_first(), Coalescer, DEFAULT_MOVE_INTERVAL, duplicate_position_is_not_resent(), encode_button(), encode_key() (+25 more)

### Community 23 - "Pacer Tests"
Cohesion: 0.10
Nodes (28): assert_poll_success(), duration_ms(), enqueue_packet_noisy(), handle_timeout_noisy(), Inner, make_packet(), MAX_BITRATE, MAX_DEBT_IN_TIME (+20 more)

### Community 24 - "clipboard.rs"
Cohesion: 0.07
Nodes (29): grab(), region(), screenshot(), ClipCommand, RestoreSoon, Set, SetForInsert, IMAGE_EVERY (+21 more)

### Community 25 - "src/info.rs"
Cohesion: 0.04
Nodes (38): battery(), CACHE, DESKTOP_SWITCHDESKTOP, displays(), EVERY, exec(), focus(), IMC_GETOPENSTATUS (+30 more)

### Community 26 - "store.rs"
Cohesion: 0.11
Nodes (26): BLOB_PREFIX, CACHE, clip_secret(), Credentials, credentials_round_trip_and_read_legacy_password(), delete_clip_secret(), delete_password(), device() (+18 more)

### Community 27 - "tls.rs"
Cohesion: 0.10
Nodes (6): client_config(), fingerprint(), parse_ignores_comments_and_blank_lines(), parse_pins(), save_pins(), TofuVerifier

### Community 28 - "Errors"
Cohesion: 0.06
Nodes (24): Channel<'a>, RtcError, ChangesOutOfOrder, Dtls, FeedbackNotEnabled, Ice, Io, Net (+16 more)

### Community 29 - "Time Types"
Cohesion: 0.08
Nodes (12): instant_ord(), super_instant_test(), TimeDelta, Negative, NegativeInfinity, Positive, PositiveInfinity, .ZERO (+4 more)

### Community 30 - "Session Mgmt"
Cohesion: 0.06
Nodes (8): FecRtp, Mid, SessionId, MAX_BLANK_PADDING_PAYLOAD_SIZE, SRTCP_OVERHEAD, SRTP_BLOCK_SIZE, SRTP_OVERHEAD, Session

### Community 32 - "Session"
Cohesion: 0.09
Nodes (34): show(), bridge_remove(), bridge_setup(), bridge_status(), click(), confirm(), edid(), FIXTURE_ROUTES (+26 more)

### Community 33 - "rtc.rs"
Cohesion: 0.06
Nodes (28): AudioState, Closed, Connecting, Disconnected, Streaming, AbortOnDrop, BACKOFF_MAX, BACKOFF_MIN (+20 more)

### Community 34 - "Trendline BWE"
Cohesion: 0.11
Nodes (22): InterGroupDelayDelta, DELAY_COUNT_RANGE, delay_variation(), duration_ms(), K_DOWN, K_UP, MAX_ADOPT_OFFSET_MS, OVER_USE_THRESHOLD_DEFAULT_MS (+14 more)

### Community 35 - "Probe Estimator"
Cohesion: 0.07
Nodes (30): log_probe_bitrate_estimate!, invalid_receive_send_ratio_is_rejected(), lost_probe_packets_do_not_affect_estimate(), MAX_ACTIVE_PROBES, MAX_PROBE_INTERVAL, MAX_VALID_RATIO, MIN_CLUSTER_SIZE, MIN_RATIO_FOR_UNSATURATED_LINK (+22 more)

### Community 36 - "SDP Parser"
Cohesion: 0.08
Nodes (47): Input, Receive, Timeout, Output, Transmit, any_value(), attribute_line(), attribute_line_flag() (+39 more)

### Community 37 - "src/direct.rs"
Cohesion: 0.05
Nodes (49): BACKOFF_MAX, BACKOFF_MIN, BinMessage, Frame, Pong, Unknown, choose_format(), DirectOptions (+41 more)

### Community 38 - "Media Time"
Cohesion: 0.05
Nodes (19): duration_from_units(), payload_duration_uses_rtp_clock_rate(), units_from_duration(), Duration, Frequency, .EIGHT_KHZ, .FIXED_POINT_6_18, .FORTY_EIGHT_KHZ (+11 more)

### Community 39 - "SRTP"
Cohesion: 0.07
Nodes (34): CTR_BUFFER_PADDING, DECRYPTED_PAYLOAD, error_details(), KEY, LABEL_RTCP_AES, LABEL_RTCP_AUTHENTICATION_KEY, LABEL_RTCP_SALT, LABEL_RTP_AES (+26 more)

### Community 40 - "instant"
Cohesion: 0.06
Nodes (17): LeakyBucketPacer, PacerImpl, LeakyBucket, Null, PacerReason, FirstEver, Handle, Immediate (+9 more)

### Community 41 - "CodecConfig"
Cohesion: 0.12
Nodes (12): CodecConfig, comfort_noise_supports_dynamic_payload_types_at_higher_clock_rates(), conflicting_remote_red_mappings_do_not_panic(), enable_comfort_noise_uses_static_8khz_payload_type(), enable_red_links_all_audio_codecs_with_distinct_pts(), enable_red_links_opus(), red_dropped_when_remote_lacks_red(), red_kept_when_both_sides_enable() (+4 more)

### Community 42 - "MediaAttribute"
Cohesion: 0.06
Nodes (33): CodecSpec, is_dir(), MediaAttribute, Candidate, EndOfCandidates, ExtMap, Fingerprint, Fmtp (+25 more)

### Community 43 - "flexfec.rs"
Cohesion: 0.17
Nodes (27): a_rebuilt_packet_completes_another_repair(), complete_group_counts_as_consistent(), Decoder, encode(), expect(), extend(), feed(), flags() (+19 more)

### Community 45 - "rate_control.rs"
Cohesion: 0.14
Nodes (14): BETA, DEFAULT_BACKOFF_TIME, duration_ms(), make_control(), MAX_ESTIMATE_RATIO, MULTIPLICATIVE_INCREASE_COEF, OBSERVED_BIT_RATE_SMOOTHING_FACTOR, OBSERVED_BITRATE_RESET_THRESHOLD_STD (+6 more)

### Community 46 - "h264.rs"
Cohesion: 0.06
Nodes (33): ANNEXB_NALUSTART_CODE, AUD_NALU_TYPE, FILLER_NALU_TYPE, FU_END_BITMASK, FU_START_BITMASK, FUA_HEADER_SIZE, FUA_NALU_TYPE, FUB_NALU_TYPE (+25 more)

### Community 47 - "audio.rs"
Cohesion: 0.07
Nodes (18): AudioSession, CHANNELS, device_name(), MAX_BUFFER, MAX_FRAME, output_devices(), PACKET_QUEUE, Player (+10 more)

### Community 48 - "input.rs"
Cohesion: 0.12
Nodes (25): autorepeat_is_dropped(), caps_lock_is_a_normal_key_off_macos(), caps_lock_toggle_is_a_full_press_each_time(), cursor_in_black_bars_is_dropped_and_blocks_clicks(), cursor_maps_corners_to_device_range(), focus_loss_releases_everything(), InputMapper, keys_pressed_under_cmd_are_released_with_cmd() (+17 more)

### Community 49 - "Result"
Cohesion: 0.15
Nodes (37): packetize_respects_mtu(), Result, test_h265_aggregation_packet(), test_h265_aggregation_with_donl_sequences(), test_h265_annexb_roundtrip_with_depacketizer(), test_h265_ap_buffer_overflow_fallback(), test_h265_ap_roundtrip_with_depacketizer(), test_h265_donl_aggregation_round_trip() (+29 more)

### Community 50 - "av1.rs"
Cohesion: 0.08
Nodes (33): AGGREGATION_HEADER_SIZE, discards_temporal_delimiter_and_tile_list_obu(), many_packets_with_border_aligned_obus(), MAX_NUM_OBUS_TO_OMTI_SIZE, OBU_EXTENSION_PRESENT_MASK, obu_payload_size_set_after_extension_when_absent(), obu_payload_size_set_after_extension_when_present(), obu_payload_size_set_when_absent() (+25 more)

### Community 51 - "channel.rs"
Cohesion: 0.12
Nodes (14): association_lost_releases_held_stream_ids(), channel_id_allocation(), ChannelAllocation, ChannelData, ChannelHandler, ChannelId, negotiated_stream_id_cannot_overlap_held_generation(), negotiated_stream_keeps_config_while_waiting_for_reset() (+6 more)

### Community 52 - "rtcp/mod.rs"
Cohesion: 0.03
Nodes (80): Goodbye, Fir, FirEntry, FeedbackMessageType, NotUsed, PayloadFeedback, ReceptionReport, SourceCount (+72 more)

### Community 54 - "streamer.rs"
Cohesion: 0.10
Nodes (22): clamps_and_builds_query(), Encoder, H264State, Hdmi, is_jpeg(), limits(), presets_stay_inside_limits(), QualityPreset (+14 more)

### Community 55 - "codec.rs"
Cohesion: 0.24
Nodes (3): g722_16khz_media_time_halves_on_the_wire(), g722_rtp_clock_rate_is_8khz(), g722_spec()

### Community 57 - "cometdesk-agent/src/main.rs"
Cohesion: 0.09
Nodes (22): BACKOFF_MAX, BACKOFF_MIN, BATTERY_LOW, DEFAULT_RELAY, detach_console(), fail(), IDLE_TIMEOUT, LOG_FILE (+14 more)

### Community 59 - "CodecPacketizer"
Cohesion: 0.06
Nodes (18): CodecPacketizer, Av1, Boxed, ComfortNoise, G711, G722, H264, H265 (+10 more)

### Community 60 - "session.rs"
Cohesion: 0.04
Nodes (27): MediaData, OutgoingPacket, PacketSink, RedSink, RedSink<'a>, StreamTx, red_same_pt_blocks(), RedBlock (+19 more)

### Community 61 - "viewer.rs"
Cohesion: 0.10
Nodes (10): App, Bench, BENCH_WARMUP, build_event_loop(), process_cpu_time(), run(), UserEvent, Frame (+2 more)

### Community 62 - "rtp/header.rs"
Cohesion: 0.10
Nodes (7): DATAGRAM_MAX_PACKET_SIZE, mk_extend!, test_generate_one_length_padding_packet(), test_parse_rtp_headers(), test_parse_rtp_headers_two_byte_form(), test_parse_rtp_headers_with_csrc(), truncate_off_srtp_padding()

### Community 63 - "SrtpCrypto"
Cohesion: 0.10
Nodes (9): AeadKey, AesKey, CipherCtx, DummySrtpCryptoImpl, SrtpCrypto, AwsLc, OpenSsl, WinCrypto (+1 more)

### Community 64 - "vt.rs"
Cohesion: 0.05
Nodes (14): DecodeError, NeedKeyframe, Os, decodes_keyframe_then_delta_synchronously(), DELTA, delta_frame_without_parameter_sets_needs_keyframe(), fixture(), KEY (+6 more)

### Community 65 - "MediaLine"
Cohesion: 0.09
Nodes (4): Bandwidth, MediaLine, Session, SsrcInfo

### Community 66 - "red.rs"
Cohesion: 0.12
Nodes (22): decode_rejects_too_many_redundant_blocks(), encode_skips_oversized_blocks(), F_BIT, MAX_BLOCK_LEN, MAX_RED_RECOVERY_DEPTH, MAX_REDUNDANT_BLOCKS, MAX_TS_OFFSET, never_panics_on_arbitrary_input() (+14 more)

### Community 67 - "app.rs"
Cohesion: 0.15
Nodes (3): App, hide_menu_bar_in_fullscreen(), set_dock_icon()

### Community 69 - "DevicesUi"
Cohesion: 0.12
Nodes (11): connect(), ConnectError, NeedTotp, Other, WrongPassword, ConnectRequest, DevicesUi, Form (+3 more)

### Community 70 - "Pt"
Cohesion: 0.10
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

### Community 75 - "MediaKind"
Cohesion: 0.07
Nodes (12): AsSdpMediaLine, MediaType, (Mid, usize), MediaKind, Audio, Video, MediaType, Application (+4 more)

### Community 76 - "vla.rs"
Cohesion: 0.12
Nodes (28): assert_ser_deser(), div_round_up(), parse_leb_u63(), read_bit(), read_bits(), read_lower_4bits_ignoring_leading_zeros(), ResolutionAndFramerate, serialize() (+20 more)

### Community 77 - "super"
Cohesion: 0.05
Nodes (30): crop_png(), crops_by_fraction(), Enabled, EdidResult, HidState, Jiggler, Keyboard, KeyboardLeds (+22 more)

### Community 78 - "ReportList"
Cohesion: 0.10
Nodes (9): &'a ReportList<T>, Iter, Iter<'a, T>, IterOwned, IterOwned<T>, ReportList<T>, test_lists_from_iter(), test_max_length_into_iter() (+1 more)

### Community 79 - "Changes"
Cohesion: 0.13
Nodes (15): add_new_lines(), add_pending_changes(), apply_answer(), apply_offer(), as_media_lines(), as_sdp(), AsSdpParams, AsSdpParams<'a, 'b> (+7 more)

### Community 80 - "vp9.rs"
Cohesion: 0.11
Nodes (15): detect_vp9_keyframe_bitstream(), MAX_SPATIAL_LAYERS, MAX_VP9REF_PICS, packetize_respects_mtu(), test_vp9_packet_unmarshal(), test_vp9_packetizer_flexible_mode(), test_vp9_packetizer_fragmented_keyframe(), test_vp9_packetizer_keyframe() (+7 more)

### Community 81 - ".push"
Cohesion: 0.22
Nodes (26): bounded_queue_meta(), full_queue_ignores_older_packets_and_duplicates(), incomplete_frames_cannot_grow_queue_forever(), padding_after_incomplete_vp8_frame_does_not_grow_without_bound(), padding_after_initial_incomplete_vp8_frame_does_not_grow_without_bound(), padding_after_waiting_vp8_frame_does_not_grow_without_bound(), padding_only_run_does_not_grow_the_queue_without_bound(), padding_only_run_with_loss_stays_bounded_and_reports_discontinuity() (+18 more)

### Community 82 - "mock_device.rs"
Cohesion: 0.17
Nodes (16): acceptor(), changed_certificate_is_refused(), concurrent_requests_share_one_relogin(), Config, creds(), expired_token_triggers_one_relogin(), Mock, PASSWORD (+8 more)

### Community 83 - "ProbeClusterConfig"
Cohesion: 0.11
Nodes (17): low_rate_padding_reaches_minimum_packet_count(), min_probe_delta_is_enforced_in_next_probe_time(), probe_cluster_complete_even_with_zero_duration(), probe_cluster_complete_when_both_criteria_met(), probe_cluster_not_complete_after_bytes_but_not_packets(), probe_cluster_not_complete_after_packets_but_not_bytes(), probe_cluster_not_complete_before_start(), probe_cluster_tracks_bytes_and_packets() (+9 more)

### Community 84 - "not_happening"
Cohesion: 0.12
Nodes (12): QueuePriority, Empty, Media, Padding, QueueSnapshot, peek_pop_after_timestamp(), peek_pop_no_timestamp(), SendQueue (+4 more)

### Community 85 - "DelayController"
Cohesion: 0.10
Nodes (8): DelayController, MAX_RTT_HISTORY_WINDOW, MAX_TWCC_GAP, RTT_SMOOTHING_FACTOR, UPDATE_INTERVAL, log_bitrate_estimate!, log_delay_variation!, MovingAverage

### Community 86 - "super"
Cohesion: 0.08
Nodes (23): Av1CodecExtra, detect_av1_keyframe(), ComfortNoiseDepacketizer, ComfortNoisePacketizer, depacketizer_rejects_empty_cn_payload(), packetizer_does_not_emit_empty_cn_payload(), packetizer_rejects_payload_larger_than_mtu(), detect_h264_keyframe() (+15 more)

### Community 87 - "Error"
Cohesion: 0.07
Nodes (24): CHUNK, flag(), MsdDrive, MsdImage, MsdPart, MsdState, MsdStorage, Session (+16 more)

### Community 88 - "janus.rs"
Cohesion: 0.11
Nodes (18): KEEPALIVE, msg(), negotiate(), parse_reply(), PLUGIN, PROTOCOL, Reply, Candidate (+10 more)

### Community 89 - "Media"
Cohesion: 0.14
Nodes (4): Media, Shared, STALE, Upload

### Community 90 - ".new"
Cohesion: 0.14
Nodes (10): detect_h265_keyframe(), H265NALUHeader, test_detect_h265_keyframe(), test_h265_aggregated_exact_layout(), test_h265_irap_detection(), test_h265_mtu_variation(), test_h265_nalu_header(), test_h265_paci_packetizer() (+2 more)

### Community 91 - "SessionAttribute"
Cohesion: 0.06
Nodes (21): FingerprintFmt<'a>, Proto, Sctp, Srtp, SessionAttribute, AllowMixedExts, Candidate, EndOfCandidates (+13 more)

### Community 92 - "insert.rs"
Cohesion: 0.10
Nodes (17): insert(), key(), paste_key(), send_unicode(), paste_keys(), InsertReply, Failed, Inserted (+9 more)

### Community 95 - "VideoRenderer"
Cohesion: 0.11
Nodes (6): Nv12, RenderError, ZeroCopyUnavailable, SHADER, VideoRenderer, write_plane()

### Community 96 - "cometdesk_bridge.py"
Cohesion: 0.13
Nodes (12): _agent_path(), firmware_version(), handle_agent_download(), handle_agent_upload(), list_agents(), main(), _read(), _restrict_socket() (+4 more)

### Community 97 - "layout/data.rs"
Cohesion: 0.38
Nodes (4): CODES, LAYOUTS, S, Table

### Community 98 - "rtx_cache_buf.rs"
Cohesion: 0.11
Nodes (22): after(), buffer_cmp(), Entry, evict_all(), evict_oldest(), evict_with_gap(), EvictingBuffer, EvictingBuffer<T> (+14 more)

### Community 99 - "arrival_group.rs"
Cohesion: 0.13
Nodes (15): ArrivalGroup, ArrivalGroupAccumulator, Belongs, NewGroup, Skipped, Yes, BURST_TIME_INTERVAL, duration_us() (+7 more)

### Community 100 - "SendSideBandwidthEstimator"
Cohesion: 0.09
Nodes (16): log_inherent_loss!, log_loss!, log_loss_based_bitrate_estimate!, log_loss_bw_limit_in_window!, log_pacer_media_debt!, log_pacer_padding_debt!, log_rate_control_applied_change!, log_rate_control_observed_bitrate! (+8 more)

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
Cohesion: 0.07
Nodes (25): ANNEXB_NALUSTART_CODE, H265FRAGMENTATION_UNIT_HEADER_SIZE, H265NALU_AGGREGATION_PACKET_TYPE, H265NALU_AUD_NALU_TYPE, H265NALU_BLA_N_LP, H265NALU_BLA_W_LP, H265NALU_BLA_W_RADL, H265NALU_CRA_NUT (+17 more)

### Community 105 - "twcc.rs"
Cohesion: 0.24
Nodes (20): empty_twcc(), negative_deltas(), register_write_parse_large_delta(), register_write_parse_mixed_delta(), register_write_parse_small_delta(), register_write_parse_small_delta_missing(), report_padded_to_even_word(), report_truncated_to_max_byte_size() (+12 more)

### Community 106 - "annexb.rs"
Cohesion: 0.11
Nodes (25): probe(), access_unit_to_avcc(), annexb(), AUD, classify(), emulation_prevention_bytes_are_not_start_codes(), find_start_code(), first_start() (+17 more)

### Community 107 - "crypto/mod.rs"
Cohesion: 0.09
Nodes (5): test_default_provider(), Derived, AeadAes128Gcm, AeadAes256Gcm, Aes128CmSha1_80

### Community 108 - "RenderPath"
Cohesion: 0.09
Nodes (5): Renderer, RenderPath, Auto, Upload, ZeroCopy

### Community 109 - "console.rs"
Cohesion: 0.08
Nodes (14): CAPTURE_REPLY_TIMEOUT, Connected, FrameStats, HIDDEN_GRACE, KEY_GAP, NOTICE_TIME, read_video_info(), VideoInfo (+6 more)

### Community 110 - "zerocopy.rs"
Cohesion: 0.14
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
Cohesion: 0.08
Nodes (7): h265_remote_no_fmtp(), h265_remote_profile_only(), h266_remote_no_fmtp(), h266_remote_profile_only(), PayloadParams, .EXACT_MATCH_SCORE, .PROFILE_ONLY_MATCH_SCORE

### Community 115 - "FormatParam"
Cohesion: 0.14
Nodes (23): FormatParam, Apt, BarePtList, BareRange, H265ProfileTierLevel, H266ProfileTierLevel, LevelAsymmetryAllowed, LevelIdx (+15 more)

### Community 116 - "TargetOs"
Cohesion: 0.10
Nodes (16): agent_name(), Confirm, BridgeSetup, Click, Power, Session, Type, Video (+8 more)

### Community 118 - "VideoMode"
Cohesion: 0.10
Nodes (15): LatencyMode, .ALL, LowestLatency, SmoothDisplay, Session, VencMode, .ALL, Normal (+7 more)

### Community 119 - "DepacketizingBuffer"
Cohesion: 0.16
Nodes (6): DepacketizingBuffer, Entry, LastProcessed, Advanced, Expired, TestDepack

### Community 120 - "io/mod.rs"
Cohesion: 0.07
Nodes (19): NetError, Io, Stun, DatagramRecv, DatagramRecv<'a>, DatagramRecvInner, Dtls, Rtcp (+11 more)

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
Cohesion: 0.08
Nodes (10): Depacketizer, depacketizer_emits_one_report_at_a_time(), depacketizer_rejects_empty_and_partial_reports(), NEXT, parse_all_requires_whole_reports(), payload_ignores_reserved_bit_and_masks_volume(), REPORT, REPORT_LEN (+2 more)

### Community 126 - "fuzz.rs"
Cohesion: 0.13
Nodes (7): depack(), rtp_header(), rtp_packet(), rng_primary!, Rng, random_config(), random_extmap()

### Community 128 - "Community 128"
Cohesion: 0.13
Nodes (13): (), HMAC_KEY_LEN, HMAC_TAG_LEN, IV_LEN, KEY_LEN, RTCP_AAD_LEN, rtcp_iv(), rtp_iv() (+5 more)

### Community 129 - "HidCommand"
Cohesion: 0.12
Nodes (9): HidSender, HidCommand, Button, Key, MoveAbs, ReleaseAll, Wheel, wheel_delta() (+1 more)

### Community 130 - "RtcpFb"
Cohesion: 0.05
Nodes (33): BITRATE_MAX, MANTISSA_MAX, Remb, REMB_OFFSET, test_receiver_estimated_maximum_bitrate_marshal(), test_receiver_estimated_maximum_bitrate_overflow(), test_receiver_estimated_maximum_bitrate_truncate(), test_receiver_estimated_maximum_bitrate_unmarshal() (+25 more)

### Community 131 - "PacketChunk"
Cohesion: 0.15
Nodes (13): build_interims(), ChunkInterim, Missing, Received, PacketChunk, VectorDouble, VectorSingle, PacketStatus (+5 more)

### Community 133 - "sctp/mod.rs"
Cohesion: 0.11
Nodes (10): DEFAULT_MAX_BUFFERED_AMOUNT, DEFAULT_REMOTE_MAX_MESSAGE_SIZE, malformed_remote_snap_does_not_disable_local_opt_in(), max_payload_size_matches_mtu_minus_overhead(), partial_snap_init_requires_both_chunks(), ppi_adjust_buf(), SCTP_OVERHEAD, STREAM_OPEN_RETRY_INTERVAL (+2 more)

### Community 134 - "pion.rs"
Cohesion: 0.13
Nodes (16): add_flexfec(), config_message(), decode_sdp(), encode_sdp(), fec_level(), negotiate(), parse_signal(), sdp_travels_as_base64_json() (+8 more)

### Community 135 - "Msg"
Cohesion: 0.09
Nodes (23): Msg, Ack, Capture, CaptureFailed, Chunk, FileCancel, FileData, FileEnd (+15 more)

### Community 136 - "app/control.rs"
Cohesion: 0.20
Nodes (13): App, button(), key_name(), num(), ok(), Out, Later, Now (+5 more)

### Community 137 - "TwccSendRecord"
Cohesion: 0.17
Nodes (9): no_acked_duplicates_when_reordered(), Run, test_send_register_apply_report_for_old_seq_numbers(), test_twcc_register_send_records(), test_twcc_send_register_loss(), TwccRecvReport, TwccSendRecord, TwccSendRecordsIter (+1 more)

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
Cohesion: 0.10
Nodes (16): Extension, AbsoluteCaptureTime, AbsoluteSendTime, AudioLevel, ColorSpace, FrameMarking, PlayoutDelay, RepairedRtpStreamId (+8 more)

### Community 143 - "Event"
Cohesion: 0.07
Nodes (27): command(), run(), Event, Ack, Capture, CaptureFailed, Clip, FileCancel (+19 more)

### Community 144 - "CodecDepacketizer"
Cohesion: 0.20
Nodes (15): CodecDepacketizer, Av1, Boxed, ComfortNoise, G711, H264, H265, H266 (+7 more)

### Community 145 - "SctpInitData"
Cohesion: 0.14
Nodes (4): LOCAL_MAX_MESSAGE_SIZE, b64_encode(), SctpInitData, webrtc_transport_config()

### Community 146 - "Secret"
Cohesion: 0.14
Nodes (7): load_secret(), remove_secret(), save_secret(), secret_path(), ALPHABET, code_round_trip(), Secret

### Community 147 - "DataSize"
Cohesion: 0.16
Nodes (11): AckedBitrateEstimator, ESTIMATE_FLOOR, SMALL_SAMPLE_THRESHOLD, SMALL_SAMPLE_UNCERTAINTY, test_correct_estimate_after_initial_window(), test_long_gap_preserves_overflow_via_modulo_after_initial(), test_no_estimate_before_initial_window_has_passed(), UNCERTAINTY (+3 more)

### Community 148 - "event.rs"
Cohesion: 0.15
Nodes (4): SdpSimulcastLayer, Simulcast, SimulcastLayer, SimulcastLayerBuilder

### Community 150 - "PacketError"
Cohesion: 0.06
Nodes (29): PacketError, ErrAv1CorruptedPacket, ErrH265CorruptedPacket, ErrH265PACIPHESTooLong, ErrH266CorruptedPacket, ErrInvalidH265PacketType, ErrPayloadTooLarge, ErrRedCorruptedPacket (+21 more)

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
Cohesion: 0.15
Nodes (5): TwccSeq, Receipt, ReceiveWindow, TwccPacketId, TwccRecvRegister

### Community 165 - "Twcc"
Cohesion: 0.15
Nodes (4): padding_of_rtcp(), Twcc, TwccIter, TwccSendRecordsIter<'a>

### Community 166 - "ext.rs"
Cohesion: 0.13
Nodes (7): EXT_URI, IdHasher, MapEntry, MAX_ID, MAX_ID_ONE_BYTE_FORM, NTP_F32, VideoTiming

### Community 168 - "get_h266"
Cohesion: 0.28
Nodes (15): collision_preserves_configured_primary_and_rtx_pts(), dynamic_comfort_noise_collision_preserves_configured_pt(), get_h266(), h266_config(), h266_remote_ptl(), test_h266_no_fmtp_remote_clears(), test_h266_profile_id_cleared_on_ptl_match(), test_h266_profile_only_remote_mirrors() (+7 more)

### Community 169 - "buffer_rx.rs"
Cohesion: 0.23
Nodes (15): end_on_defacto(), end_on_marker(), gap_after_emit(), gap_after_padding(), late_packet_in_front_of_waiting_frame(), MAX_BUFFERED_PACKETS, out_of_order_packets_with_hold_0(), packets_after_hold_out() (+7 more)

### Community 170 - "RestrictionId"
Cohesion: 0.15
Nodes (4): RestrictionId, Simulcast, SimulcastGroups, SimulcastLayer

### Community 171 - "FormatParams"
Cohesion: 0.20
Nodes (7): FormatParams, .DEFAULT_TELEPHONE_EVENT_MAX, h265_combined_params_roundtrip(), h266_combined_params_roundtrip(), sprop_max_don_diff_roundtrip(), sprop_stereo_roundtrip(), telephone_event_ranges_roundtrip()

### Community 172 - "link_capacity_estimator.rs"
Cohesion: 0.28
Nodes (8): estimate_expires_after_reset_window(), ignores_invalid_probes(), LinkCapacityEstimator, .DEFAULT_RESET_WINDOW, reset_clears_estimate(), starts_with_no_estimate(), stores_probe_result(), takes_maximum_of_multiple_probes()

### Community 174 - "VideoChange"
Cohesion: 0.16
Nodes (13): VideoChange, Edid, Latency, Mode, Orientation, Quality, Venc, Orientation (+5 more)

### Community 175 - "H265FragmentationUnitHeader"
Cohesion: 0.15
Nodes (7): H265FragmentationUnitHeader, H265FragmentationUnitPacket, reconstruct_from_fu_packets(), test_h265_fragmentation_start_end_flags(), test_h265_packetizer_exact_fu_boundary_mtu(), test_h265_packetizer_fu_fragmentation_roundtrip_payload(), test_h265_sdp_driven_donl_all_packet_types()

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
Cohesion: 0.19
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

### Community 189 - "H265Profile"
Cohesion: 0.15
Nodes (12): H265Profile, FormatRangeExtensions, HighThroughput, HighThroughputScreenContentCoding, Main, Main10, MainStillPicture, MultiviewMain (+4 more)

### Community 190 - "notify.rs"
Cohesion: 0.20
Nodes (12): INSTANCE_PORT, BURST, dropped(), IO_TIMEOUT, MAX_REQUEST, Note, read_note(), REFILL (+4 more)

### Community 191 - "bench_app.py"
Cohesion: 0.26
Nodes (4): cpu_times(), main(), parse_cputime(), top_power()

### Community 192 - "PowerAction"
Cohesion: 0.15
Nodes (11): AtxButton, Power, PowerLong, Reset, PowerAction, Off, OffHard, On (+3 more)

### Community 193 - ".new"
Cohesion: 0.30
Nodes (3): derive_key(), SrtpKey, SrtpKey<ML, SL>

### Community 194 - "jobs"
Cohesion: 0.17
Nodes (11): concurrency, cancel-in-progress, group, env, TOOLCHAINS, jobs, platform-build-test, name (+3 more)

### Community 195 - "wol.rs"
Cohesion: 0.17
Nodes (5): Mac, mac_formats(), Session, WolDevice, WolList

### Community 196 - ".update_params"
Cohesion: 0.32
Nodes (13): get_h265(), h265_config(), h265_remote_ptl(), test_h265_default_level_is_6_0(), test_h265_no_fmtp_remote_clears(), test_h265_profile_id_cleared_on_ptl_match(), test_h265_profile_only_remote_mirrors(), test_h265_recvonly_local_level_lower() (+5 more)

### Community 197 - "src/receive.rs"
Cohesion: 0.06
Nodes (18): Outbox, log!, FILE_CHUNK, MAX_CLIP_FILES_BYTES, Purpose, Clipboard, Save, downloads() (+10 more)

### Community 198 - "Av1Packetizer"
Cohesion: 0.27
Nodes (5): Av1Packetizer, leb_128_size(), Obu, Packet, parse_obus()

### Community 199 - "json"
Cohesion: 0.29
Nodes (3): Client, default_socket(), main()

### Community 200 - "matrix"
Cohesion: 0.18
Nodes (11): name, os, name, os, test, crypto, dir, rust (+3 more)

### Community 201 - "matrix"
Cohesion: 0.22
Nodes (11): mobile-build, strategy, l_crypto, r_crypto, name, runs-on, strategy, timeout-minutes (+3 more)

### Community 203 - "pacer/control.rs"
Cohesion: 0.36
Nodes (7): no_padding_without_active_media(), overuse_suppresses_padding(), PacerControl, PACING_FACTOR, PacingResult, padding_enabled_with_active_media(), PADDING_TARGET

### Community 204 - "UserEvent"
Cohesion: 0.18
Nodes (9): UserEvent, ClipSecret, Connected, Control, Frame, Notice, Password, Refresh (+1 more)

### Community 205 - "H265Payload"
Cohesion: 0.20
Nodes (6): H265Payload, H265AggregationPacket, H265FragmentationUnitPacket, H265PACIPacket, H265SingleNALUnitPacket, H265SingleNALUnitPacket

### Community 206 - "H266Profile"
Cohesion: 0.18
Nodes (9): H266Profile, Main10, Main10_444, Main10_444StillPicture, Main10StillPicture, MultilayerMain10, MultilayerMain10_444, MultilayerMain10_444StillPicture (+1 more)

### Community 207 - "psfbapp.rs"
Cohesion: 0.36
Nodes (5): AppSpecificFeedback, test_empty_payload(), test_header(), test_non_aligned_payload(), test_round_trip()

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

### Community 218 - "ObuType"
Cohesion: 0.18
Nodes (10): ObuType, Frame, FrameHeader, MetaData, Padding, RedundantFrameHeader, SequenceHeader, TemporalDelimiter (+2 more)

### Community 219 - "Clipboard bridge design"
Cohesion: 0.33
Nodes (5): glkvm-agent on target, Insert clipboard vs type (protocol v2), Noise NNpsk0 bridge protocol (glkvm-clip), glkvm-bridge relay (kvmd extra), Type clipboard (hid/print)

### Community 220 - "glkvm-rs native console project"
Cohesion: 0.40
Nodes (4): Login / auth (URL-encoded, 2FA, two-step), Capability gating by model/API, glkvm::client REST client, Trust-on-first-use TLS cert pinning

### Community 221 - "Binary HID frame encoding (U1)"
Cohesion: 0.22
Nodes (7): U1: HID binary frames on /api/ws, HID input coalescing, glkvm::events (/api/ws), Binary HID frame encoding (U1), Single ordered HID queue, Overlook prior art (Swift client), release_all() safety behavior

### Community 222 - ".empty"
Cohesion: 0.42
Nodes (7): abs_capture_time_extended_form(), abs_capture_time_short_form(), abs_capture_time_two_byte_form(), abs_send_time(), abs_send_time_two_byte_form(), audio_level_round_trip(), playout_delay()

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

### Community 227 - "Feature"
Cohesion: 0.20
Nodes (10): Feature, Atx, H264, H265, Hid, Msd, RelativeMouse, VideoQuality (+2 more)

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

### Community 251 - "BufferedThresholdConfig"
Cohesion: 0.50
Nodes (4): BufferedThresholdConfig, Configured, Desired, Unconfigured

### Community 252 - "ConsoleAction"
Cohesion: 0.40
Nodes (5): ConsoleAction, Disconnect, None, TargetChanged, ToggleFullscreen

### Community 254 - "build-agents.sh"
Cohesion: 0.67
Nodes (3): build_linux(), CARGO_PROFILE_RELEASE_STRIP, build-agents.sh script

### Community 255 - "macos-crypto-test"
Cohesion: 0.50
Nodes (4): macos-crypto-test, name, runs-on, timeout-minutes

### Community 256 - "on"
Cohesion: 0.50
Nodes (4): on, schedule, workflow_dispatch, cron

### Community 260 - "Measurement targets M1-M7"
Cohesion: 0.67
Nodes (3): Phase 0 baseline measurements, Measurement targets M1-M7, Phases 0-3 roadmap

## Knowledge Gaps
- **784 isolated node(s):** `cometdesk-agent`, `Set`, `SetForInsert`, `RestoreSoon`, `Mismatch` (+779 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 2524 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **226 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Session` connect `Session` to `cometdesk-cli/src/main.rs`, `Console`, `pion.rs`, `client.rs`, `bridge.rs`, `clipsync.rs`, `events.rs`, `src/direct.rs`, `device.rs`, `streamer.rs`, `app.rs`, `wol.rs`, `super`, `mock_device.rs`, `Error`, `janus.rs`, `Media`, `annexb.rs`, `console.rs`, `shell.rs`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **What connects `cometdesk-agent`, `Set`, `SetForInsert` to the rest of the system?**
  _784 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `H266 Packetizer` be split into smaller, more focused modules?**
  _Cohesion score 0.06307102858826996 - nodes in this community are weakly interconnected._
- **Why does `Error` connect `Error` to `rtc.rs`, `app.rs`, `wol.rs`, `pion.rs`, `client.rs`, `bridge.rs`, `super`, `clipsync.rs`, `shell.rs`, `mock_device.rs`, `streamer.rs`, `clipboard.rs`, `cometdesk-agent/src/main.rs`, `janus.rs`?**
  _High betweenness centrality (0.025) - this node is a cross-community bridge._
- **Should `Stream TX/Mid` be split into smaller, more focused modules?**
  _Cohesion score 0.04319717895974141 - nodes in this community are weakly interconnected._
- **Why does `main()` connect `bench_app.py` to `gen-layouts.py`, `test_client.py`, `Time Types`, `json`?**
  _High betweenness centrality (0.023) - this node is a cross-community bridge._
- **Should `cometdesk-cli/src/main.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.028727770177838577 - nodes in this community are weakly interconnected._