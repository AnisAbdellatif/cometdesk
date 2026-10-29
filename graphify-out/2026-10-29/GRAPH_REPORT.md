# Graph Report - glkvm-rs  (2026-10-29)

## Corpus Check
- 243 files · ~382,905 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 17 file(s) not represented in the graph (top: .toml 5, (none) 4, .conf 4)

## Summary
- 6921 nodes · 16126 edges · 271 communities (238 shown, 33 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 235 edges (avg confidence: 0.74)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- H266 Packetizer
- Stream TX/Mid
- CLI Commands
- Clipboard Endpoint
- Media Model
- SSRC/Streams API
- GUI Console
- Probe Control
- NACK Register
- Stats
- KVM Client
- RTP Extensions
- Rtc Core
- SDP Data
- Loss Controller
- Clip Bridge
- Crypto Config
- Session Utils
- VP8 Payload
- Event Sessions
- SDP Negotiation
- RTC Events
- Stream RX
- Pacer Tests
- Clipboard Content
- Session Info
- HID Input
- TLS Pinning
- Errors
- Time Types
- Session Mgmt
- Bridge Setup
- Device Commands
- Negotiated Media
- Trendline BWE
- Probe Estimator
- SDP Parser
- Direct Source
- Media Time
- SRTP
- Community 40
- Community 41
- Community 42
- Community 43
- Community 44
- Community 45
- Community 46
- Community 47
- Community 48
- Community 49
- Community 50
- Community 51
- Community 52
- Community 53
- Community 54
- Community 55
- Community 56
- Community 57
- Community 58
- Community 59
- Community 60
- Community 61
- Community 62
- Community 63
- Community 64
- Community 65
- Community 66
- Community 67
- Community 68
- Community 69
- Community 70
- Community 71
- Community 72
- Community 73
- Community 74
- Community 75
- Community 76
- Community 77
- Community 78
- Community 79
- Community 80
- Community 81
- Community 82
- Community 83
- Community 84
- Community 85
- Community 86
- Community 87
- Community 88
- Community 89
- Community 90
- Community 91
- Community 92
- Community 93
- Community 94
- Community 95
- Community 96
- Community 97
- Community 98
- Community 99
- Community 100
- Community 101
- Community 102
- Community 103
- Community 104
- Community 105
- Community 106
- Community 107
- Community 108
- Community 109
- Community 110
- Community 111
- Community 112
- Community 113
- Community 114
- Community 115
- Community 116
- Community 117
- Community 118
- Community 119
- Community 120
- Community 121
- Community 122
- Community 123
- Community 124
- Community 125
- Community 126
- Community 127
- Community 128
- Community 129
- Community 130
- Community 131
- Community 132
- Community 133
- Community 134
- Community 135
- Community 136
- Community 137
- Community 138
- Community 139
- Community 140
- Community 141
- Community 142
- Community 143
- Community 144
- Community 145
- Community 146
- Community 147
- Community 148
- Community 149
- Community 150
- Community 151
- Community 152
- Community 153
- Community 154
- Community 155
- Community 156
- Community 157
- Community 158
- Community 159
- Community 160
- Community 161
- Community 162
- Community 163
- Community 164
- Community 165
- Community 166
- Community 167
- Community 168
- Community 169
- Community 170
- Community 171
- Community 172
- Community 173
- Community 174
- Community 175
- Community 176
- Community 177
- Community 178
- Community 179
- Community 180
- Community 181
- Community 182
- Community 183
- Community 184
- Community 185
- Community 186
- Community 187
- Community 188
- Community 189
- Community 190
- Community 191
- Community 192
- Community 193
- Community 194
- Community 195
- Community 196
- Community 197
- Community 198
- Community 199
- Community 200
- Community 201
- Community 202
- Community 203
- Community 204
- Community 205
- Community 206
- Community 207
- Community 208
- Community 209
- Community 210
- Community 211
- Community 212
- Community 213
- Community 214
- Community 215
- Community 216
- Community 217
- Community 218
- Community 219
- Community 220
- Community 221
- Community 222
- Community 223
- Community 224
- Community 225
- Community 226
- Community 227
- Community 228
- Community 229
- Community 230
- Community 231
- Community 232
- Community 233
- Community 234
- Community 236
- Community 238
- Community 239
- Community 240
- Community 241
- Community 242
- Community 243
- Community 244
- Community 245
- Community 246
- Community 247
- Community 248
- Community 249
- Community 250
- Community 251
- Community 252
- Community 253
- Community 254
- Community 255
- Community 256
- Community 258
- Community 259
- Community 260
- Community 261
- Community 263
- Community 264
- Community 265
- Community 266
- Community 267
- Community 268
- Community 269
- Community 270

## God Nodes (most connected - your core abstractions)
1. `Ssrc` - 115 edges
2. `PacketError` - 114 edges
3. `Pt` - 110 edges
4. `Console` - 104 edges
5. `Mid` - 102 edges
6. `RtcConfig` - 86 edges
7. `SeqNo` - 86 edges
8. `Session` - 85 edges
9. `Session` - 84 edges
10. `Media` - 83 edges

## Surprising Connections (you probably didn't know these)
- `Insert clipboard vs type (protocol v2)` --semantically_similar_to--> `Type clipboard (hid/print)`  [INFERRED] [semantically similar]
  docs/clipboard-bridge.md → PLAN.md
- `supervise()` --calls--> `wake()`  [INFERRED]
  apps/glkvm-gui/src/clipsync.rs → crates/glkvm-cli/src/commands.rs
- `Login / auth (URL-encoded, 2FA, two-step)` --references--> `glkvm::client REST client`  [INFERRED]
  docs/protocol.md → PLAN.md
- `U3: streamer lifecycle` --references--> `glkvm-media video/audio sources`  [INFERRED]
  docs/protocol.md → PLAN.md
- `Zero-copy IOSurface/Metal rendering` --references--> `U2: Direct video /api/media/ws frames`  [INFERRED]
  PLAN.md → docs/protocol.md

## Import Cycles
- 1-file cycle: `apps/glkvm-agent/src/main.rs -> apps/glkvm-agent/src/main.rs`
- 1-file cycle: `vendor/str0m/src/lib.rs -> vendor/str0m/src/lib.rs`
- 1-file cycle: `vendor/str0m/src/rtp/error.rs -> vendor/str0m/src/rtp/error.rs`
- 1-file cycle: `crates/glkvm/src/layout/mod.rs -> crates/glkvm/src/layout/mod.rs`
- 1-file cycle: `vendor/str0m/src/streams/register.rs -> vendor/str0m/src/streams/register.rs`
- 1-file cycle: `vendor/str0m/src/bwe/time.rs -> vendor/str0m/src/bwe/time.rs`
- 1-file cycle: `vendor/str0m/src/bwe/delay/arrival_group.rs -> vendor/str0m/src/bwe/delay/arrival_group.rs`
- 1-file cycle: `vendor/str0m/src/bwe/delay/rate_control.rs -> vendor/str0m/src/bwe/delay/rate_control.rs`
- 1-file cycle: `vendor/str0m/src/bwe/macros.rs -> vendor/str0m/src/bwe/macros.rs`
- 1-file cycle: `vendor/str0m/src/bwe/loss_controller.rs -> vendor/str0m/src/bwe/loss_controller.rs`
- 1-file cycle: `vendor/str0m/src/rtp/rtcp/twcc.rs -> vendor/str0m/src/rtp/rtcp/twcc.rs`
- 1-file cycle: `vendor/str0m/src/pacer/leaky.rs -> vendor/str0m/src/pacer/leaky.rs`
- 1-file cycle: `vendor/str0m/src/crypto/srtp.rs -> vendor/str0m/src/crypto/srtp.rs`
- 1-file cycle: `vendor/str0m/src/rtp/rtcp/list.rs -> vendor/str0m/src/rtp/rtcp/list.rs`
- 1-file cycle: `vendor/str0m/src/packet/contiguity_vp8.rs -> vendor/str0m/src/packet/contiguity_vp8.rs`
- 1-file cycle: `vendor/str0m/src/packet/contiguity_vp9.rs -> vendor/str0m/src/packet/contiguity_vp9.rs`
- 1-file cycle: `vendor/str0m/src/streams/register_nack.rs -> vendor/str0m/src/streams/register_nack.rs`
- 1-file cycle: `vendor/str0m/src/util/value_history.rs -> vendor/str0m/src/util/value_history.rs`
- 2-file cycle: `vendor/str0m/src/bwe/delay/control.rs -> vendor/str0m/src/bwe/mod.rs -> vendor/str0m/src/bwe/delay/control.rs`
- 2-file cycle: `apps/glkvm-gui/src/app.rs -> apps/glkvm-gui/src/console.rs -> apps/glkvm-gui/src/app.rs`

## Hyperedges (group relationships)
- **glkvm-media transport sources** — plan_direct_h264, plan_pion_webrtc, plan_janus [EXTRACTED 1.00]
- **FlexFEC-03 loss-recovery flow** — plan_flexfec, vendor_str0m_glkvm_patch, docs_protocol_flexfec_layout, bench_results_m7_flexfec [INFERRED 0.85]
- **Clipboard bridge end-to-end pipeline** — docs_clipboard_bridge_agent, docs_clipboard_bridge_relay, docs_clipboard_bridge_noise [EXTRACTED 1.00]
- **KVM visual metaphor (display + pointer)** — apps_glkvm_gui_assets_logo_256_concept_monitor, apps_glkvm_gui_assets_logo_256_concept_cursor, apps_glkvm_gui_assets_logo_256_concept_kvm [INFERRED 0.80]

## Communities (271 total, 33 thin omitted)

### Community 0 - "H266 Packetizer"
Cohesion: 0.06
Nodes (86): annexb(), ANNEXB_NALUSTART_CODE, depacketize_all(), detect_h266_keyframe(), H266Depacketizer, H266FRAGMENTATION_UNIT_HEADER_SIZE, H266FragmentationUnitHeader, H266NALU_AGGREGATION_PACKET_TYPE (+78 more)

### Community 1 - "Stream TX/Mid"
Cohesion: 0.04
Nodes (32): NullPacer, PaddingRequest, QueuePriority, Empty, Media, Padding, QueueSnapshot, QueueState (+24 more)

### Community 2 - "CLI Commands"
Cohesion: 0.03
Nodes (74): BridgeCmd, Remove, Setup, Status, ButtonArg, Power, PowerLong, Reset (+66 more)

### Community 3 - "Clipboard Endpoint"
Cohesion: 0.06
Nodes (54): CHUNK, Endpoint, Error, Crypto, Frame, Handshake, Image, Message (+46 more)

### Community 4 - "Media Model"
Cohesion: 0.04
Nodes (20): MediaType, MediaAdded, blocked_buffer(), deadlines_cover_payload_types_rids_and_resets(), MAX_PENDING_PAYLOADS, Media, MID_PROBE, PayloaderEntry (+12 more)

### Community 5 - "SSRC/Streams API"
Cohesion: 0.05
Nodes (5): DirectApi<'a>, Mid, Ssrc, Streams, StreamTimeoutConfig

### Community 6 - "GUI Console"
Cohesion: 0.07
Nodes (3): agent_name(), Console, Typing

### Community 7 - "Probe Control"
Cohesion: 0.08
Nodes (38): ProbeKind, Exponential, IncreaseAlr, Initial, LargeDrop, PeriodicAlr, Stagnant, allocation_probe_fires_when_desired_increases_in_alr() (+30 more)

### Community 8 - "NACK Register"
Cohesion: 0.07
Nodes (38): all_packets_have_same_frequency(), expected(), expected_overflow(), expected_received_loss(), jitter_at_0(), jitter_at_20(), jitter_rtp_timestamp_rollover(), active_window_sliding() (+30 more)

### Community 9 - "Stats"
Cohesion: 0.05
Nodes (22): drv_identity_copy!, log_stat!, CandidatePairStats, CandidateStats, MediaEgressStats, MediaIngressStats, PeerStats, RemoteEgressStats (+14 more)

### Community 10 - "KVM Client"
Cohesion: 0.06
Nodes (22): api_message(), ConfigResult, Connection, Credentials, Enabled, Envelope, ErrorResult, header() (+14 more)

### Community 11 - "RTP Extensions"
Cohesion: 0.05
Nodes (30): long_event_uses_contiguous_segments(), MAX_DURATION, MAX_SEGMENT, MIN_DURATION, MIN_PAUSE, packets(), telephone_packets_have_separate_send_times(), TelephonePackets (+22 more)

### Community 12 - "Rtc Core"
Cohesion: 0.04
Nodes (18): Bwe, Bwe<'a>, DirectApi, SdpApi, ChannelBufferedAmountLow, ChannelClose, ChannelOpen, IceConnectionStateChange (+10 more)

### Community 13 - "SDP Data"
Cohesion: 0.06
Nodes (45): bundle_mids_returns_correct_mids(), FingerprintFmt, g722_red_emits_wire_clock_rate(), h265_and_h264_coexist(), h265_chrome_profile_only_sdp(), h265_empty_fmtp_not_serialized(), h265_in_complete_sdp(), h265_incomplete_params_all_permutations() (+37 more)

### Community 14 - "Loss Controller"
Cohesion: 0.10
Nodes (22): AsValid, ChannelParameters, Config, HoldInfo, LogNormalDistribution, loss_spike_recovery(), LossBasedBweResult, LossController (+14 more)

### Community 15 - "Clip Bridge"
Cohesion: 0.06
Nodes (37): BACKOFF_MAX, BACKOFF_MIN, BridgeState, AgentOutdated, Connected, Connecting, Error, Mismatch (+29 more)

### Community 17 - "Session Utils"
Cohesion: 0.05
Nodes (24): red_same_pt_blocks(), RedBlock, LastRrtr, main_payload_params(), make_max_seq_lookup(), MAX_DLRR_PER_REPORT, MAX_PENDING_RRTRS, NACK_MIN_INTERVAL (+16 more)

### Community 18 - "VP8 Payload"
Cohesion: 0.05
Nodes (29): KEY_IDX_MASK, packetize_respects_mtu(), PICTURE_ID_MAX, PICTURE_ID_SHORT_MASK, PICTURE_ID_SHORT_MAX, read_vp8_descriptor_byte(), rewrite_payload(), test_vp8_descriptor_exposes_fields() (+21 more)

### Community 19 - "Event Sessions"
Cohesion: 0.07
Nodes (31): BACKOFF_MAX, BACKOFF_MIN, ConnState, Closed, Connected, Connecting, Disconnected, Kicked (+23 more)

### Community 20 - "SDP Negotiation"
Cohesion: 0.09
Nodes (33): add_ice_details(), AddMedia, answer_uses_negotiated_dtls_role(), apply_direct_changes(), as_new_medias_contiguous_indices_with_non_media_changes(), Change, AddApp, AddChannel (+25 more)

### Community 21 - "RTC Events"
Cohesion: 0.04
Nodes (36): BweKind, Remb, Twcc, Event, AppSpecificFeedback, ChannelData, Closed, Connected (+28 more)

### Community 22 - "Stream RX"
Cohesion: 0.06
Nodes (15): RtcpReportIntervals, DlrrItem, StreamPaused, LastSenderInfo, paused_timestamp_repair_moves_time_forward(), receiver_report_uses_supplied_intervals(), red_locate_seq_limits_recovery_depth(), red_locate_seq_rejects_known_and_recovered_frames() (+7 more)

### Community 23 - "Pacer Tests"
Cohesion: 0.10
Nodes (28): assert_poll_success(), duration_ms(), enqueue_packet_noisy(), handle_timeout_noisy(), Inner, make_packet(), MAX_BITRATE, MAX_DEBT_IN_TIME (+20 more)

### Community 24 - "Clipboard Content"
Cohesion: 0.07
Nodes (28): grab(), region(), screenshot(), ClipCommand, RestoreSoon, Set, SetForInsert, IMAGE_EVERY (+20 more)

### Community 25 - "Session Info"
Cohesion: 0.05
Nodes (24): battery(), DESKTOP_SWITCHDESKTOP, displays(), EVERY, focus(), IMC_GETOPENSTATUS, ime(), is_terminal() (+16 more)

### Community 26 - "HID Input"
Cohesion: 0.07
Nodes (33): map_button(), abs_mapping_is_monotonic(), click_flushes_pending_move_first(), Coalescer, DEFAULT_MOVE_INTERVAL, duplicate_position_is_not_resent(), encode_button(), encode_key() (+25 more)

### Community 27 - "TLS Pinning"
Cohesion: 0.07
Nodes (17): changed_cert_is_rejected(), check_pin(), client_config(), config_dir(), fingerprint(), first_contact_pins_then_matches(), parse_ignores_comments_and_blank_lines(), parse_pins() (+9 more)

### Community 28 - "Errors"
Cohesion: 0.05
Nodes (27): Channel<'a>, RtcError, ChangesOutOfOrder, Dtls, FeedbackNotEnabled, Ice, Io, Net (+19 more)

### Community 29 - "Time Types"
Cohesion: 0.08
Nodes (12): instant_ord(), super_instant_test(), TimeDelta, Negative, NegativeInfinity, Positive, PositiveInfinity, .ZERO (+4 more)

### Community 30 - "Session Mgmt"
Cohesion: 0.07
Nodes (3): FecRtp, SessionId, Session

### Community 31 - "Bridge Setup"
Cohesion: 0.08
Nodes (29): health_text(), agents_dir(), BridgeSetup, Inner, AGENT_NAMES, BRIDGE_VERSION, BRIDGE_WS, BridgeSide (+21 more)

### Community 32 - "Device Commands"
Cohesion: 0.09
Nodes (33): bridge_remove(), bridge_setup(), bridge_status(), click(), confirm(), edid(), FIXTURE_ROUTES, info() (+25 more)

### Community 33 - "Negotiated Media"
Cohesion: 0.06
Nodes (29): AudioState, Closed, Connecting, Disconnected, Streaming, BACKOFF_MAX, BACKOFF_MIN, Ended (+21 more)

### Community 34 - "Trendline BWE"
Cohesion: 0.07
Nodes (33): InterGroupDelayDelta, DELAY_COUNT_RANGE, delay_variation(), duration_ms(), K_DOWN, K_UP, MAX_ADOPT_OFFSET_MS, OVER_USE_THRESHOLD_DEFAULT_MS (+25 more)

### Community 35 - "Probe Estimator"
Cohesion: 0.07
Nodes (29): invalid_receive_send_ratio_is_rejected(), lost_probe_packets_do_not_affect_estimate(), MAX_ACTIVE_PROBES, MAX_PROBE_INTERVAL, MAX_VALID_RATIO, MIN_CLUSTER_SIZE, MIN_RATIO_FOR_UNSATURATED_LINK, MIN_RECEIVED_BYTES_RATIO (+21 more)

### Community 36 - "SDP Parser"
Cohesion: 0.06
Nodes (24): bare_static_audio_pts_are_resolved(), media_attribute_line(), media_attribute_line_rid_pt(), media_attribute_line_rid_pt_restr(), media_attribute_line_rid_restr(), media_attribute_line_rid_simple(), media_attribute_line_simulcast(), media_attribute_line_simulcast_alt() (+16 more)

### Community 37 - "Direct Source"
Cohesion: 0.07
Nodes (34): BACKOFF_MAX, BACKOFF_MIN, BinMessage, Frame, Pong, Unknown, choose_format(), DirectOptions (+26 more)

### Community 38 - "Media Time"
Cohesion: 0.08
Nodes (5): Duration, Instant, MediaTime, .ZERO, ts_rebase()

### Community 39 - "SRTP"
Cohesion: 0.07
Nodes (34): CTR_BUFFER_PADDING, DECRYPTED_PAYLOAD, error_details(), KEY, LABEL_RTCP_AES, LABEL_RTCP_AUTHENTICATION_KEY, LABEL_RTCP_SALT, LABEL_RTP_AES (+26 more)

### Community 40 - "Community 40"
Cohesion: 0.07
Nodes (14): LeakyBucketPacer, PacerImpl, LeakyBucket, Null, PacerReason, FirstEver, Handle, Immediate (+6 more)

### Community 41 - "Community 41"
Cohesion: 0.11
Nodes (14): CodecConfig, comfort_noise_supports_dynamic_payload_types_at_higher_clock_rates(), conflicting_remote_red_mappings_do_not_panic(), enable_comfort_noise_uses_static_8khz_payload_type(), enable_red_links_all_audio_codecs_with_distinct_pts(), enable_red_links_opus(), red_dropped_when_remote_lacks_red(), red_kept_when_both_sides_enable() (+6 more)

### Community 42 - "Community 42"
Cohesion: 0.05
Nodes (34): MediaAttribute, Candidate, EndOfCandidates, ExtMap, Fingerprint, Fmtp, IceOptions, IcePwd (+26 more)

### Community 43 - "Community 43"
Cohesion: 0.11
Nodes (18): RtpPacket, after(), packet(), rtx_cache_0_duration(), rtx_cache_0_sized(), rtx_cache_100_sized_backwards(), rtx_cache_1_sized(), rtx_cache_eviction() (+10 more)

### Community 44 - "Community 44"
Cohesion: 0.08
Nodes (3): Fingerprint, Dtls, is_would_block()

### Community 45 - "Community 45"
Cohesion: 0.09
Nodes (22): BETA, DEFAULT_BACKOFF_TIME, duration_ms(), make_control(), MAX_ESTIMATE_RATIO, MULTIPLICATIVE_INCREASE_COEF, OBSERVED_BIT_RATE_SMOOTHING_FACTOR, OBSERVED_BITRATE_RESET_THRESHOLD_STD (+14 more)

### Community 46 - "Community 46"
Cohesion: 0.06
Nodes (33): ANNEXB_NALUSTART_CODE, AUD_NALU_TYPE, FILLER_NALU_TYPE, FU_END_BITMASK, FU_START_BITMASK, FUA_HEADER_SIZE, FUA_NALU_TYPE, FUB_NALU_TYPE (+25 more)

### Community 47 - "Community 47"
Cohesion: 0.07
Nodes (18): AudioSession, CHANNELS, device_name(), MAX_BUFFER, MAX_FRAME, output_devices(), PACKET_QUEUE, Player (+10 more)

### Community 48 - "Community 48"
Cohesion: 0.12
Nodes (25): autorepeat_is_dropped(), caps_lock_is_a_normal_key_off_macos(), caps_lock_toggle_is_a_full_press_each_time(), cursor_in_black_bars_is_dropped_and_blocks_clicks(), cursor_maps_corners_to_device_range(), focus_loss_releases_everything(), InputMapper, keys_pressed_under_cmd_are_released_with_cmd() (+17 more)

### Community 49 - "Community 49"
Cohesion: 0.15
Nodes (37): packetize_respects_mtu(), Result, test_h265_aggregation_packet(), test_h265_aggregation_with_donl_sequences(), test_h265_annexb_roundtrip_with_depacketizer(), test_h265_ap_buffer_overflow_fallback(), test_h265_ap_roundtrip_with_depacketizer(), test_h265_donl_aggregation_round_trip() (+29 more)

### Community 50 - "Community 50"
Cohesion: 0.08
Nodes (33): AGGREGATION_HEADER_SIZE, discards_temporal_delimiter_and_tile_list_obu(), many_packets_with_border_aligned_obus(), MAX_NUM_OBUS_TO_OMTI_SIZE, OBU_EXTENSION_PRESENT_MASK, obu_payload_size_set_after_extension_when_absent(), obu_payload_size_set_after_extension_when_present(), obu_payload_size_set_when_absent() (+25 more)

### Community 51 - "Community 51"
Cohesion: 0.11
Nodes (15): association_lost_releases_held_stream_ids(), Channel, channel_id_allocation(), ChannelAllocation, ChannelData, ChannelHandler, ChannelId, negotiated_stream_id_cannot_overlap_held_generation() (+7 more)

### Community 52 - "Community 52"
Cohesion: 0.08
Nodes (15): Goodbye, Fir, FirEntry, RtcpHeader, RtcpType, ApplicationDefined, ExtendedReport, Goodbye (+7 more)

### Community 53 - "Community 53"
Cohesion: 0.12
Nodes (23): BLOB_PREFIX, CACHE, clip_secret(), Credentials, credentials_round_trip_and_read_legacy_password(), delete_clip_secret(), delete_password(), device() (+15 more)

### Community 54 - "Community 54"
Cohesion: 0.17
Nodes (27): a_rebuilt_packet_completes_another_repair(), complete_group_counts_as_consistent(), Decoder, encode(), expect(), extend(), feed(), flags() (+19 more)

### Community 55 - "Community 55"
Cohesion: 0.06
Nodes (16): EdidResult, normalize(), g722_16khz_media_time_halves_on_the_wire(), g722_rtp_clock_rate_is_8khz(), g722_spec(), Frequency, .EIGHT_KHZ, .FIXED_POINT_6_18 (+8 more)

### Community 56 - "Community 56"
Cohesion: 0.07
Nodes (27): PacketError, ErrAv1CorruptedPacket, ErrH265CorruptedPacket, ErrH265PACIPHESTooLong, ErrH266CorruptedPacket, ErrInvalidH265PacketType, ErrPayloadTooLarge, ErrRedCorruptedPacket (+19 more)

### Community 57 - "Community 57"
Cohesion: 0.08
Nodes (22): BACKOFF_MAX, BACKOFF_MIN, DEFAULT_RELAY, detach_console(), fail(), IDLE_TIMEOUT, INSTANCE_PORT, LOG_FILE (+14 more)

### Community 58 - "Community 58"
Cohesion: 0.07
Nodes (25): CHUNK_CHARS, chunks(), chunks_break_after_newlines_and_respect_utf8(), combo(), combo_presses_then_releases_in_reverse(), COMBOS, CONFIRM_CHARS, confirm_reason() (+17 more)

### Community 59 - "Community 59"
Cohesion: 0.06
Nodes (18): CodecPacketizer, Av1, Boxed, ComfortNoise, G711, G722, H264, H265 (+10 more)

### Community 60 - "Community 60"
Cohesion: 0.08
Nodes (9): OutgoingPacket, PacketSink, Payloader, RedSink, RedSink<'a>, StreamTx, SeqNo, .MAX (+1 more)

### Community 61 - "Community 61"
Cohesion: 0.10
Nodes (10): App, Bench, BENCH_WARMUP, build_event_loop(), process_cpu_time(), run(), UserEvent, Frame (+2 more)

### Community 62 - "Community 62"
Cohesion: 0.07
Nodes (10): DATAGRAM_MAX_PACKET_SIZE, mk_extend!, RtpHeader, test_generate_one_length_padding_packet(), test_parse_rtp_headers(), test_parse_rtp_headers_two_byte_form(), test_parse_rtp_headers_with_csrc(), truncate_off_srtp_padding() (+2 more)

### Community 63 - "Community 63"
Cohesion: 0.10
Nodes (8): AeadKey, AesKey, DummySrtpCryptoImpl, SrtpCrypto, AwsLc, OpenSsl, WinCrypto, SrtpCryptoImpl

### Community 64 - "Community 64"
Cohesion: 0.06
Nodes (3): DELTA, KEY, output_callback()

### Community 65 - "Community 65"
Cohesion: 0.06
Nodes (12): FingerprintFmt<'a>, is_dir(), MediaLine, MediaType, Application, Audio, Unknown, Video (+4 more)

### Community 66 - "Community 66"
Cohesion: 0.12
Nodes (24): decode_rejects_too_many_redundant_blocks(), encode_skips_oversized_blocks(), F_BIT, MAX_BLOCK_LEN, MAX_RED_RECOVERY_DEPTH, MAX_REDUNDANT_BLOCKS, MAX_TS_OFFSET, never_panics_on_arbitrary_input() (+16 more)

### Community 67 - "Community 67"
Cohesion: 0.14
Nodes (5): App, connect(), hide_menu_bar_in_fullscreen(), set_dock_icon(), Connected

### Community 68 - "Community 68"
Cohesion: 0.09
Nodes (16): CapabilityResult, LatencyMode, .ALL, LowestLatency, SmoothDisplay, Session, VencMode, .ALL (+8 more)

### Community 69 - "Community 69"
Cohesion: 0.10
Nodes (28): probe(), access_unit_to_avcc(), annexb(), AUD, classify(), emulation_prevention_bytes_are_not_start_codes(), find_start_code(), first_start() (+20 more)

### Community 70 - "Community 70"
Cohesion: 0.10
Nodes (24): CodecSpec, PT_AV1, PT_AV1_RTX, PT_COMFORT_NOISE, PT_G722, PT_G722_RED, PT_H265, PT_H265_RTX (+16 more)

### Community 71 - "Community 71"
Cohesion: 0.09
Nodes (19): ACCENT, ACCENT_HOVER, apply(), banner(), BG, BORDER, CARD, CARD_HOVER (+11 more)

### Community 72 - "Community 72"
Cohesion: 0.11
Nodes (21): clamps_and_builds_query(), Encoder, H264State, Hdmi, is_jpeg(), limits(), presets_stay_inside_limits(), QualityPreset (+13 more)

### Community 73 - "Community 73"
Cohesion: 0.09
Nodes (10): VERSION, CodecSpec, RtpMap, Sdp, sdp_ser!, sdp(), SdpAnswer, SdpOffer (+2 more)

### Community 74 - "Community 74"
Cohesion: 0.14
Nodes (19): CodecSpec, [bool; 128], h264_codec_spec(), h265_codec_spec(), h265_codec_spec_profile_only(), h266_codec_spec(), h266_codec_spec_profile_only(), payload_params_red_defaults_none() (+11 more)

### Community 75 - "Community 75"
Cohesion: 0.12
Nodes (21): abs_time_delta(), fuzz_failures(), pack_4_rr(), pack_sr_4_rr(), padding_of_rtcp(), report(), roundtrip_sr_rr(), rr() (+13 more)

### Community 76 - "Community 76"
Cohesion: 0.12
Nodes (28): assert_ser_deser(), div_round_up(), parse_leb_u63(), read_bit(), read_bits(), read_lower_4bits_ignoring_leading_zeros(), ResolutionAndFramerate, serialize() (+20 more)

### Community 77 - "Community 77"
Cohesion: 0.08
Nodes (22): Capabilities, Enabled, Feature, Atx, H264, H265, Hid, Msd (+14 more)

### Community 78 - "Community 78"
Cohesion: 0.12
Nodes (10): &'a ReportList<T>, Iter, Iter<'a, T>, IterOwned, IterOwned<T>, ReportList, ReportList<T>, test_lists_from_iter() (+2 more)

### Community 79 - "Community 79"
Cohesion: 0.15
Nodes (13): add_new_lines(), add_pending_changes(), apply_answer(), apply_offer(), as_media_lines(), as_sdp(), Changes, create_offer() (+5 more)

### Community 80 - "Community 80"
Cohesion: 0.09
Nodes (18): detect_vp9_keyframe_bitstream(), MAX_SPATIAL_LAYERS, MAX_VP9REF_PICS, packetize_respects_mtu(), test_vp9_packet_unmarshal(), test_vp9_packetizer_flexible_mode(), test_vp9_packetizer_fragmented_keyframe(), test_vp9_packetizer_keyframe() (+10 more)

### Community 81 - "Community 81"
Cohesion: 0.21
Nodes (27): bounded_queue_meta(), full_queue_ignores_older_packets_and_duplicates(), incomplete_frames_cannot_grow_queue_forever(), padding_after_incomplete_vp8_frame_does_not_grow_without_bound(), padding_after_initial_incomplete_vp8_frame_does_not_grow_without_bound(), padding_after_waiting_vp8_frame_does_not_grow_without_bound(), padding_only_run_does_not_grow_the_queue_without_bound(), padding_only_run_with_loss_stays_bounded_and_reports_discontinuity() (+19 more)

### Community 82 - "Community 82"
Cohesion: 0.16
Nodes (16): acceptor(), changed_certificate_is_refused(), concurrent_requests_share_one_relogin(), Config, creds(), expired_token_triggers_one_relogin(), Mock, PASSWORD (+8 more)

### Community 83 - "Community 83"
Cohesion: 0.16
Nodes (10): low_rate_padding_reaches_minimum_packet_count(), min_probe_delta_is_enforced_in_next_probe_time(), probe_cluster_complete_even_with_zero_duration(), probe_cluster_complete_when_both_criteria_met(), probe_cluster_not_complete_after_bytes_but_not_packets(), probe_cluster_not_complete_after_packets_but_not_bytes(), probe_cluster_not_complete_before_start(), probe_cluster_tracks_bytes_and_packets() (+2 more)

### Community 84 - "Community 84"
Cohesion: 0.09
Nodes (18): FeedbackMessageType, NotUsed, PayloadFeedback, ReceptionReport, SourceCount, Subtype, TransportFeedback, PayloadType (+10 more)

### Community 85 - "Community 85"
Cohesion: 0.10
Nodes (8): DelayController, MAX_RTT_HISTORY_WINDOW, MAX_TWCC_GAP, RTT_SMOOTHING_FACTOR, UPDATE_INTERVAL, log_bitrate_estimate!, log_delay_variation!, MovingAverage

### Community 86 - "Community 86"
Cohesion: 0.08
Nodes (9): ComfortNoiseDepacketizer, ComfortNoisePacketizer, depacketizer_rejects_empty_cn_payload(), packetizer_does_not_emit_empty_cn_payload(), packetizer_rejects_payload_larger_than_mtu(), G711Depacketizer, Depacketizer, OpusDepacketizer (+1 more)

### Community 87 - "Community 87"
Cohesion: 0.14
Nodes (4): Media, Shared, STALE, Upload

### Community 88 - "Community 88"
Cohesion: 0.10
Nodes (19): KEEPALIVE, msg(), negotiate(), parse_reply(), PLUGIN, PROTOCOL, Reply, Candidate (+11 more)

### Community 89 - "Community 89"
Cohesion: 0.27
Nodes (23): Input, Receive, Timeout, Output, Transmit, any_value(), attribute_line(), attribute_line_flag() (+15 more)

### Community 90 - "Community 90"
Cohesion: 0.14
Nodes (10): detect_h265_keyframe(), H265NALUHeader, test_detect_h265_keyframe(), test_h265_aggregated_exact_layout(), test_h265_irap_detection(), test_h265_mtu_variation(), test_h265_nalu_header(), test_h265_paci_packetizer() (+2 more)

### Community 91 - "Community 91"
Cohesion: 0.08
Nodes (15): Bandwidth, Session, SessionAttribute, AllowMixedExts, Candidate, EndOfCandidates, Fingerprint, Group (+7 more)

### Community 92 - "Community 92"
Cohesion: 0.10
Nodes (17): insert(), key(), paste_key(), send_unicode(), paste_keys(), InsertReply, Failed, Inserted (+9 more)

### Community 93 - "Community 93"
Cohesion: 0.16
Nodes (6): ConnectRequest, DevicesUi, Form, Prompt, Password, Totp

### Community 95 - "Community 95"
Cohesion: 0.11
Nodes (6): Nv12, RenderError, ZeroCopyUnavailable, SHADER, VideoRenderer, write_plane()

### Community 96 - "Community 96"
Cohesion: 0.13
Nodes (12): _agent_path(), firmware_version(), handle_agent_download(), handle_agent_upload(), list_agents(), main(), _read(), _restrict_socket() (+4 more)

### Community 97 - "Community 97"
Cohesion: 0.12
Nodes (19): CODES, LAYOUTS, ALTGR, altgr_is_right_alt_alone(), control_characters_and_missing(), every_table_is_sorted_and_has_the_basics(), german_windows_altgr_and_swapped_keys(), Keys (+11 more)

### Community 98 - "Community 98"
Cohesion: 0.18
Nodes (12): buffer_cmp(), Entry, evict_all(), evict_oldest(), evict_with_gap(), EvictingBuffer, EvictingBuffer<T>, grow_and_reindex() (+4 more)

### Community 99 - "Community 99"
Cohesion: 0.15
Nodes (15): ArrivalGroup, ArrivalGroupAccumulator, Belongs, NewGroup, Skipped, Yes, BURST_TIME_INTERVAL, duration_us() (+7 more)

### Community 100 - "Community 100"
Cohesion: 0.15
Nodes (6): BITRATE_WINDOW, Bwe, in_startup_phase(), INITIAL_BITRATE_WINDOW, SendSideBandwidthEstimator, STARTUP_PHASE

### Community 101 - "Community 101"
Cohesion: 0.10
Nodes (7): H265ProfileTierLevel, .FALLBACK, H265Tier, High, Main, test_h265_profile_tier_level_from_fmtp(), test_h265_profile_tier_level_new()

### Community 102 - "Community 102"
Cohesion: 0.10
Nodes (7): H266ProfileTierLevel, .FALLBACK, H266Tier, High, Main, test_h266_profile_tier_level_from_fmtp(), test_h266_profile_tier_level_new()

### Community 103 - "Community 103"
Cohesion: 0.16
Nodes (19): close_is_reported_before_reset_complete_when_events_are_batched(), connect_client_server(), dcep_receiver_applies_reliability_params(), entry_by_id_mut(), entry_index(), err_stream_already_exist_in_band_returns_close(), insert_entry(), negotiated_reuse_does_not_report_open_for_old_closed_stream() (+11 more)

### Community 104 - "Community 104"
Cohesion: 0.07
Nodes (25): ANNEXB_NALUSTART_CODE, H265FRAGMENTATION_UNIT_HEADER_SIZE, H265NALU_AGGREGATION_PACKET_TYPE, H265NALU_AUD_NALU_TYPE, H265NALU_BLA_N_LP, H265NALU_BLA_W_LP, H265NALU_BLA_W_RADL, H265NALU_CRA_NUT (+17 more)

### Community 105 - "Community 105"
Cohesion: 0.24
Nodes (20): empty_twcc(), negative_deltas(), register_write_parse_large_delta(), register_write_parse_mixed_delta(), register_write_parse_small_delta(), register_write_parse_small_delta_missing(), report_padded_to_even_word(), report_truncated_to_max_byte_size() (+12 more)

### Community 106 - "Community 106"
Cohesion: 0.10
Nodes (17): pad_bytes_to_word(), cname_serialize_deserialize(), Descriptions, Sdes, SdesType, CNAME, EMAIL, END (+9 more)

### Community 107 - "Community 107"
Cohesion: 0.09
Nodes (6): from_feature_flags(), test_default_provider(), Derived, AeadAes128Gcm, AeadAes256Gcm, Aes128CmSha1_80

### Community 108 - "Community 108"
Cohesion: 0.09
Nodes (6): Renderer, Result, RenderPath, Auto, Upload, ZeroCopy

### Community 109 - "Community 109"
Cohesion: 0.10
Nodes (14): Audio, CAPTURE_REPLY_TIMEOUT, FrameStats, HIDDEN_GRACE, install_command(), install_commands_carry_the_code_and_fit_the_run_dialog(), KEY_GAP, NOTICE_TIME (+6 more)

### Community 110 - "Community 110"
Cohesion: 0.14
Nodes (7): decode_fixture(), device(), iosurface_planes_are_sampled_without_copy(), Keep, read_texel(), TextureCache, textures_outlive_the_frame()

### Community 111 - "Community 111"
Cohesion: 0.11
Nodes (10): EncodedFrame, MediaSession, QUEUE_DEPTH, SourceOptions, start(), Transport, .ALL, Direct (+2 more)

### Community 112 - "Community 112"
Cohesion: 0.13
Nodes (11): base64(), check_path(), DEFAULT_TIMEOUT, end_marker_skips_the_echoed_command(), find_end(), MAX_LINE, Output, PATH (+3 more)

### Community 113 - "Community 113"
Cohesion: 0.11
Nodes (6): RtpHeader, ExtensionMap, MapEntry, remap_exts_illegal(), remap_exts_swaparoo(), remap_exts_video()

### Community 114 - "Community 114"
Cohesion: 0.08
Nodes (7): h265_remote_no_fmtp(), h265_remote_profile_only(), h266_remote_no_fmtp(), h266_remote_profile_only(), PayloadParams, .EXACT_MATCH_SCORE, .PROFILE_ONLY_MATCH_SCORE

### Community 115 - "Community 115"
Cohesion: 0.14
Nodes (23): FormatParam, Apt, BarePtList, BareRange, H265ProfileTierLevel, H266ProfileTierLevel, LevelAsymmetryAllowed, LevelIdx (+15 more)

### Community 116 - "Community 116"
Cohesion: 0.11
Nodes (19): Confirm, BridgeSetup, Click, Power, Type, Video, VideoChange, Edid (+11 more)

### Community 118 - "Community 118"
Cohesion: 0.13
Nodes (19): Av1CodecExtra, detect_av1_keyframe(), detect_h264_keyframe(), H264CodecExtra, H265CodecExtra, H266CodecExtra, CodecExtra, Av1 (+11 more)

### Community 119 - "Community 119"
Cohesion: 0.16
Nodes (6): DepacketizingBuffer, Entry, LastProcessed, Advanced, Expired, TestDepack

### Community 120 - "Community 120"
Cohesion: 0.11
Nodes (9): DatagramRecv, DatagramRecvInner, Dtls, Rtcp, Rtp, Stun, MAX_DTLS_OVERHEAD, MAX_RTP_OVERHEAD (+1 more)

### Community 121 - "Community 121"
Cohesion: 0.21
Nodes (14): AlrDetector, AlrState, InAlr, NotInAlr, frequent_timeouts_preserve_media_budget(), test_alr_enter_with_low_usage(), test_alr_estimate_change(), test_alr_exit_with_high_usage() (+6 more)

### Community 122 - "Community 122"
Cohesion: 0.09
Nodes (18): Reason, BweDelayControl, BweProbeControl, BweProbeEstimator, Channel, DTLS, Feedback, Ice (+10 more)

### Community 123 - "Community 123"
Cohesion: 0.09
Nodes (17): Codec, Av1, CN, G722, H264, H265, H266, Null (+9 more)

### Community 124 - "Community 124"
Cohesion: 0.11
Nodes (10): depacketizer_emits_one_report_at_a_time(), depacketizer_rejects_empty_and_partial_reports(), NEXT, parse_all_requires_whole_reports(), payload_duration_uses_rtp_clock_rate(), payload_ignores_reserved_bit_and_masks_volume(), REPORT, REPORT_LEN (+2 more)

### Community 125 - "Community 125"
Cohesion: 0.12
Nodes (9): HidSender, HidCommand, Button, Key, MoveAbs, ReleaseAll, Wheel, wheel_delta() (+1 more)

### Community 126 - "Community 126"
Cohesion: 0.12
Nodes (7): depack(), rtp_header(), rtp_packet(), rng_primary!, Rng, random_config(), random_extmap()

### Community 127 - "Community 127"
Cohesion: 0.11
Nodes (13): AtxButton, Power, PowerLong, Reset, AtxLeds, AtxState, PowerAction, Off (+5 more)

### Community 128 - "Community 128"
Cohesion: 0.14
Nodes (14): (), CipherCtx, HMAC_KEY_LEN, HMAC_TAG_LEN, IV_LEN, KEY_LEN, RTCP_AAD_LEN, rtcp_iv() (+6 more)

### Community 129 - "Community 129"
Cohesion: 0.13
Nodes (5): WordSized, Ssrc, ReceiverReport, ReceptionReport, SenderReport

### Community 130 - "Community 130"
Cohesion: 0.11
Nodes (16): app_specific_feedback_not_confused_with_remb(), app_specific_feedback_passes_through_rtcpfb(), app_specific_feedback_write_and_parse_round_trip(), RtcpFb, AppSpecificFeedback, DlrrItem, Fir, Goodbye (+8 more)

### Community 131 - "Community 131"
Cohesion: 0.15
Nodes (13): build_interims(), ChunkInterim, Missing, Received, PacketChunk, VectorDouble, VectorSingle, PacketStatus (+5 more)

### Community 132 - "Community 132"
Cohesion: 0.20
Nodes (13): App, button(), key_name(), num(), ok(), Out, Later, Now (+5 more)

### Community 133 - "Community 133"
Cohesion: 0.12
Nodes (10): DEFAULT_MAX_BUFFERED_AMOUNT, DEFAULT_REMOTE_MAX_MESSAGE_SIZE, malformed_remote_snap_does_not_disable_local_opt_in(), max_payload_size_matches_mtu_minus_overhead(), partial_snap_init_requires_both_chunks(), ppi_adjust_buf(), SCTP_OVERHEAD, STREAM_OPEN_RETRY_INTERVAL (+2 more)

### Community 134 - "Community 134"
Cohesion: 0.15
Nodes (15): add_flexfec(), config_message(), decode_sdp(), encode_sdp(), fec_level(), negotiate(), parse_signal(), sdp_travels_as_base64_json() (+7 more)

### Community 135 - "Community 135"
Cohesion: 0.13
Nodes (16): HidState, Jiggler, Keyboard, KeyboardLeds, Keymaps, KeymapsResult, Mouse, Outputs (+8 more)

### Community 136 - "Community 136"
Cohesion: 0.23
Nodes (12): IntervalBudget, .WINDOW, test_alr_hysteresis_thresholds(), test_budget_ratio_clamped(), test_can_build_up_underuse_false(), test_can_build_up_underuse_true(), test_debt_recovery(), test_increase_budget_accumulates() (+4 more)

### Community 137 - "Community 137"
Cohesion: 0.17
Nodes (8): no_acked_duplicates_when_reordered(), Run, test_send_register_apply_report_for_old_seq_numbers(), test_twcc_register_send_records(), test_twcc_send_register_loss(), TwccRecvReport, TwccSendRecord, TwccSendRegister

### Community 138 - "Community 138"
Cohesion: 0.10
Nodes (16): Extension, AbsoluteCaptureTime, AbsoluteSendTime, AudioLevel, ColorSpace, FrameMarking, PlayoutDelay, RepairedRtpStreamId (+8 more)

### Community 139 - "Community 139"
Cohesion: 0.15
Nodes (14): Contiguity, None, Vp8, Vp9, FrameContiguityState, contiguous_l1t1_no_l1_contig_l0(), contiguous_l1t2(), contiguous_l1t3() (+6 more)

### Community 140 - "Community 140"
Cohesion: 0.10
Nodes (18): H264LevelIdc, Level1, Level1_1, Level1_2, Level1_3, Level1B, Level2, Level2_1 (+10 more)

### Community 142 - "Community 142"
Cohesion: 0.20
Nodes (15): CodecDepacketizer, Av1, Boxed, ComfortNoise, G711, H264, H265, H266 (+7 more)

### Community 143 - "Community 143"
Cohesion: 0.10
Nodes (18): Probe, Battery, Display, Focus, Info, Msg, Capture, CaptureFailed (+10 more)

### Community 144 - "Community 144"
Cohesion: 0.13
Nodes (8): CHUNK, flag(), MsdDrive, MsdImage, MsdPart, MsdState, MsdStorage, Session

### Community 145 - "Community 145"
Cohesion: 0.14
Nodes (4): LOCAL_MAX_MESSAGE_SIZE, b64_encode(), SctpInitData, webrtc_transport_config()

### Community 146 - "Community 146"
Cohesion: 0.14
Nodes (6): Dlrr, ExtendedReport, ReportBlock, Dlrr, Rrtr, Rrtr

### Community 147 - "Community 147"
Cohesion: 0.19
Nodes (11): AckedBitrateEstimator, ESTIMATE_FLOOR, SMALL_SAMPLE_THRESHOLD, SMALL_SAMPLE_UNCERTAINTY, test_correct_estimate_after_initial_window(), test_long_gap_preserves_overflow_via_modulo_after_initial(), test_no_estimate_before_initial_window_has_passed(), UNCERTAINTY (+3 more)

### Community 148 - "Community 148"
Cohesion: 0.15
Nodes (4): SdpSimulcastLayer, Simulcast, SimulcastLayer, SimulcastLayerBuilder

### Community 149 - "Community 149"
Cohesion: 0.14
Nodes (7): load_secret(), remove_secret(), save_secret(), secret_path(), ALPHABET, code_round_trip(), Secret

### Community 150 - "Community 150"
Cohesion: 0.10
Nodes (16): Error, Api, CertChanged, Forbidden, Http, Io, Json, Protocol (+8 more)

### Community 151 - "Community 151"
Cohesion: 0.15
Nodes (5): MediaControl, Decoded, FramePipeline, KEYFRAME_REQUEST_MIN_INTERVAL, Shared

### Community 152 - "Community 152"
Cohesion: 0.16
Nodes (19): runs-on, steps, RUSTDOCFLAGS, cargo-deny, pii, snowflake, steps, runs-on (+11 more)

### Community 153 - "Community 153"
Cohesion: 0.11
Nodes (18): env, runs-on, steps, timeout-minutes, FUZZ_TIMEOUT, jobs, cargo-fuzz, name (+10 more)

### Community 154 - "Community 154"
Cohesion: 0.12
Nodes (16): H266Level, Level1, Level15_5, Level2, Level2_1, Level3, Level3_1, Level4 (+8 more)

### Community 156 - "Community 156"
Cohesion: 0.12
Nodes (17): runs-on, steps, env, CARGO_TERM_COLOR, jobs, deny, test, name (+9 more)

### Community 157 - "Community 157"
Cohesion: 0.18
Nodes (7): all_key_codes(), code_name(), is_meta_code(), names_are_wire_safe(), names_match_firmware_when_available(), SAMPLE, translate()

### Community 158 - "Community 158"
Cohesion: 0.18
Nodes (4): Nack, nack_entry_iter(), NackEntry, NackEntryIterator

### Community 159 - "Community 159"
Cohesion: 0.26
Nodes (17): L_CRYPTO, R_CRYPTO, steps, steps, steps, steps, env, id (+9 more)

### Community 160 - "Community 160"
Cohesion: 0.17
Nodes (4): PacketResult, PacketResultsSummary, PartialObservation, &TwccSendRecord

### Community 161 - "Community 161"
Cohesion: 0.12
Nodes (5): Direction, Inactive, RecvOnly, SendOnly, SendRecv

### Community 162 - "Community 162"
Cohesion: 0.27
Nodes (12): contiguous_l1t1_no_l1_contig_l0(), contiguous_l1t2(), contiguous_l1t3(), contiguous_l1t3_no_l2_contig_l0(), contiguous_l1t3_no_l2_discontig_l0(), FrameContiguityState, get_codec_extra(), keyframe_accepts_restarted_picture_ids() (+4 more)

### Community 163 - "Community 163"
Cohesion: 0.13
Nodes (14): H265Level, Level1, Level2, Level2_1, Level3, Level3_1, Level4, Level4_1 (+6 more)

### Community 164 - "Community 164"
Cohesion: 0.15
Nodes (6): TwccSeq, Receipt, ReceiveWindow, TwccPacketId, TwccRecvRegister, TwccSendRecordsIter

### Community 165 - "Community 165"
Cohesion: 0.16
Nodes (3): Twcc, TwccIter, TwccSendRecordsIter<'a>

### Community 166 - "Community 166"
Cohesion: 0.13
Nodes (7): EXT_URI, IdHasher, MAX_ID, MAX_ID_ONE_BYTE_FORM, NTP_F32, VideoTiming, epoch_to_beginning()

### Community 167 - "Community 167"
Cohesion: 0.17
Nodes (5): Mac, mac_formats(), Session, WolDevice, WolList

### Community 168 - "Community 168"
Cohesion: 0.31
Nodes (14): collision_preserves_configured_primary_and_rtx_pts(), dynamic_comfort_noise_collision_preserves_configured_pt(), get_h266(), h266_config(), h266_remote_ptl(), test_h266_no_fmtp_remote_clears(), test_h266_profile_id_cleared_on_ptl_match(), test_h266_profile_only_remote_mirrors() (+6 more)

### Community 169 - "Community 169"
Cohesion: 0.23
Nodes (15): end_on_defacto(), end_on_marker(), gap_after_emit(), gap_after_padding(), late_packet_in_front_of_waiting_frame(), MAX_BUFFERED_PACKETS, out_of_order_packets_with_hold_0(), packets_after_hold_out() (+7 more)

### Community 170 - "Community 170"
Cohesion: 0.18
Nodes (6): decodes_keyframe_then_delta_synchronously(), delta_frame_without_parameter_sets_needs_keyframe(), fixture(), Plane, same_parameter_sets_reuse_the_session(), VideoFrame

### Community 171 - "Community 171"
Cohesion: 0.20
Nodes (7): FormatParams, .DEFAULT_TELEPHONE_EVENT_MAX, h265_combined_params_roundtrip(), h266_combined_params_roundtrip(), sprop_max_don_diff_roundtrip(), sprop_stereo_roundtrip(), telephone_event_ranges_roundtrip()

### Community 172 - "Community 172"
Cohesion: 0.28
Nodes (8): estimate_expires_after_reset_window(), ignores_invalid_probes(), LinkCapacityEstimator, .DEFAULT_RESET_WINDOW, reset_clears_estimate(), starts_with_no_estimate(), stores_probe_result(), takes_maximum_of_multiple_probes()

### Community 173 - "Community 173"
Cohesion: 0.14
Nodes (3): AsSdpMediaLine, (Mid, usize), Msid

### Community 174 - "Community 174"
Cohesion: 0.27
Nodes (5): Av1Packetizer, leb_128_size(), Obu, Packet, parse_obus()

### Community 175 - "Community 175"
Cohesion: 0.15
Nodes (7): H265FragmentationUnitHeader, H265FragmentationUnitPacket, reconstruct_from_fu_packets(), test_h265_fragmentation_start_end_flags(), test_h265_packetizer_exact_fu_boundary_mtu(), test_h265_packetizer_fu_fragmentation_roundtrip_payload(), test_h265_sdp_driven_donl_all_packet_types()

### Community 176 - "Community 176"
Cohesion: 0.21
Nodes (9): BITRATE_MAX, MANTISSA_MAX, Remb, REMB_OFFSET, test_receiver_estimated_maximum_bitrate_marshal(), test_receiver_estimated_maximum_bitrate_overflow(), test_receiver_estimated_maximum_bitrate_truncate(), test_receiver_estimated_maximum_bitrate_unmarshal() (+1 more)

### Community 177 - "Community 177"
Cohesion: 0.21
Nodes (6): AppKit, CoreGraphics, draw(), gradient(), png(), rgb()

### Community 178 - "Community 178"
Cohesion: 0.14
Nodes (13): UserEvent, ClipSecret, Connected, Control, Frame, Notice, Password, Refresh (+5 more)

### Community 179 - "Community 179"
Cohesion: 0.24
Nodes (10): audio_arrives(), direct_video_decodes_in_hardware(), event_socket_connects_and_answers_pings(), first_keyframe(), janus_delivers_a_keyframe(), janus_delivers_audio(), login(), pion_delivers_a_keyframe() (+2 more)

### Community 180 - "Community 180"
Cohesion: 0.16
Nodes (7): SctpError, DcepBadUtf8, DcepIncorrectMessageType, DcepOpenTooSmall, InvalidSnap, Proto, WriteBeforeEstablished

### Community 181 - "Community 181"
Cohesion: 0.19
Nodes (7): ChannelConfig, DcepOpen, Reliability, MaxPacketLifetime, MaxRetransmits, Reliable, (ReliabilityType, u32)

### Community 182 - "Community 182"
Cohesion: 0.14
Nodes (14): H264Profile, Baseline, CAVLC444Intra, ConstrainedBaseline, ConstrainedHigh, Extended, High, High10 (+6 more)

### Community 183 - "Community 183"
Cohesion: 0.32
Nodes (5): auth_token(), main(), recv_exact(), recv_frame(), send_frame()

### Community 184 - "Community 184"
Cohesion: 0.26
Nodes (6): fetch(), level(), main(), parse(), rust_char(), unescape()

### Community 185 - "Community 185"
Cohesion: 0.15
Nodes (12): concurrency, cancel-in-progress, group, env, TOOLCHAINS, name, on, pull_request (+4 more)

### Community 186 - "Community 186"
Cohesion: 0.18
Nodes (6): AsSdpParams, AsSdpParams<'a, 'b>, Setup, Active, ActPass, Passive

### Community 187 - "Community 187"
Cohesion: 0.18
Nodes (3): H265AggregationPacket, H265AggregationUnit, H265AggregationUnitFirst

### Community 189 - "Community 189"
Cohesion: 0.15
Nodes (12): H265Profile, FormatRangeExtensions, HighThroughput, HighThroughputScreenContentCoding, Main, Main10, MainStillPicture, MultiviewMain (+4 more)

### Community 190 - "Community 190"
Cohesion: 0.33
Nodes (8): abs_capture_time_extended_form(), abs_capture_time_short_form(), abs_capture_time_two_byte_form(), abs_send_time(), abs_send_time_two_byte_form(), audio_level_round_trip(), playout_delay(), remap_exts_audio()

### Community 191 - "Community 191"
Cohesion: 0.26
Nodes (4): cpu_times(), main(), parse_cputime(), top_power()

### Community 192 - "Community 192"
Cohesion: 0.29
Nodes (5): DecodeError, NeedKeyframe, Os, Sink, VtDecoder

### Community 193 - "Community 193"
Cohesion: 0.30
Nodes (3): derive_key(), SrtpKey, SrtpKey<ML, SL>

### Community 194 - "Community 194"
Cohesion: 0.17
Nodes (11): concurrency, cancel-in-progress, group, env, TOOLCHAINS, jobs, platform-build-test, name (+3 more)

### Community 195 - "Community 195"
Cohesion: 0.32
Nodes (7): default_mtu_is_datagram_mtu_target(), enable_comfort_noise_updates_codec_config(), set_mtu_accepts_equal_bounds(), set_mtu_panics_on_inverted_range(), set_mtu_panics_on_target_above_max(), set_mtu_panics_on_target_below_min(), set_mtu_round_trips()

### Community 196 - "Community 196"
Cohesion: 0.36
Nodes (12): get_h265(), h265_config(), h265_remote_ptl(), test_h265_no_fmtp_remote_clears(), test_h265_profile_id_cleared_on_ptl_match(), test_h265_profile_only_remote_mirrors(), test_h265_recvonly_local_level_lower(), test_h265_recvonly_remote_level_lower() (+4 more)

### Community 197 - "Community 197"
Cohesion: 0.29
Nodes (7): equals_inside_value_and_unbalanced_quote(), load_dotenv(), load_file(), parse(), quotes_comments_export_and_blank_lines(), unquote(), values_are_literal()

### Community 198 - "Community 198"
Cohesion: 0.25
Nodes (3): NetError, Io, Stun

### Community 199 - "Community 199"
Cohesion: 0.29
Nodes (3): Client, default_socket(), main()

### Community 200 - "Community 200"
Cohesion: 0.18
Nodes (11): name, os, name, os, test, crypto, dir, rust (+3 more)

### Community 201 - "Community 201"
Cohesion: 0.22
Nodes (11): mobile-build, strategy, l_crypto, r_crypto, name, runs-on, strategy, timeout-minutes (+3 more)

### Community 202 - "Community 202"
Cohesion: 0.27
Nodes (7): DatagramRecv<'a>, MultiplexKind, Dtls, Rtcp, Rtp, Stun, Receive<'a>

### Community 203 - "Community 203"
Cohesion: 0.36
Nodes (7): no_padding_without_active_media(), overuse_suppresses_padding(), PacerControl, PACING_FACTOR, PacingResult, padding_enabled_with_active_media(), PADDING_TARGET

### Community 204 - "Community 204"
Cohesion: 0.18
Nodes (10): ObuType, Frame, FrameHeader, MetaData, Padding, RedundantFrameHeader, SequenceHeader, TemporalDelimiter (+2 more)

### Community 205 - "Community 205"
Cohesion: 0.20
Nodes (6): H265Payload, H265AggregationPacket, H265FragmentationUnitPacket, H265PACIPacket, H265SingleNALUnitPacket, H265SingleNALUnitPacket

### Community 206 - "Community 206"
Cohesion: 0.18
Nodes (9): H266Profile, Main10, Main10_444, Main10_444StillPicture, Main10StillPicture, MultilayerMain10, MultilayerMain10_444, MultilayerMain10_444StillPicture (+1 more)

### Community 207 - "Community 207"
Cohesion: 0.36
Nodes (5): AppSpecificFeedback, test_empty_payload(), test_header(), test_non_aligned_payload(), test_round_trip()

### Community 208 - "Community 208"
Cohesion: 0.18
Nodes (8): SctpEvent, AssociationLost, BufferedAmountLow, Close, Data, Open, StreamResetComplete, Transmit

### Community 209 - "Community 209"
Cohesion: 0.33
Nodes (5): ESTIMATE_WINDOW, EstimateSmoother, in_tolerance(), TimeBitrate, TOLERANCE

### Community 210 - "Community 210"
Cohesion: 0.20
Nodes (6): SrtpProfile, AeadAes128Gcm, AeadAes256Gcm, Aes128CmSha1_80, .ALL, PassThrough

### Community 212 - "Community 212"
Cohesion: 0.20
Nodes (9): H264ProfileIdc, X2C, X42, X4D, X58, X64, X6E, X7A (+1 more)

### Community 216 - "Community 216"
Cohesion: 0.27
Nodes (6): &'a TwccSendRegister, Delta, Large, Small, test_twcc_iter_correct_deltas(), test_twcc_iter_limited_with_status_count()

### Community 217 - "Community 217"
Cohesion: 0.31
Nodes (9): M7 Pion+FlexFEC under loss results, Opus audio over Pion/Janus, gl-pion FlexFEC-03 repair layout, Pion/Janus signaling (base64 SDP), Opus audio playback (opus-decoder + cpal), FlexFEC-03 decode (glkvm-media/flexfec.rs), Pion WebRTC+FlexFEC transport (/pion/ws), str0m WebRTC engine (rtc.rs) (+1 more)

### Community 218 - "Community 218"
Cohesion: 0.22
Nodes (8): ethernet, driver, enabled, host_mac, kvm_mac, start, otg, devices

### Community 219 - "Community 219"
Cohesion: 0.25
Nodes (7): glkvm-agent on target, Insert clipboard vs type (protocol v2), Noise NNpsk0 bridge protocol (glkvm-clip), Clipboard bridge design, glkvm-bridge relay (kvmd extra), RNDIS USB Ethernet transport, Type clipboard (hid/print)

### Community 220 - "Community 220"
Cohesion: 0.22
Nodes (8): Login / auth (URL-encoded, 2FA, two-step), Capability gating by model/API, GLKVM KVMD device firmware, glkvm-rs native console project, PiKVM (upstream of GLKVM), glkvm::client REST client, RustDesk/rdev prior art, Trust-on-first-use TLS cert pinning

### Community 221 - "Community 221"
Cohesion: 0.22
Nodes (7): U1: HID binary frames on /api/ws, HID input coalescing, glkvm::events (/api/ws), Binary HID frame encoding (U1), Single ordered HID queue, Overlook prior art (Swift client), release_all() safety behavior

### Community 223 - "Community 223"
Cohesion: 0.22
Nodes (9): runs-on, steps, jobs, fuzz-build, lint, related-crates, runs-on, runs-on (+1 more)

### Community 224 - "Community 224"
Cohesion: 0.22
Nodes (9): with, arguments, check_diff, command, components, fetch-depth, line_width_rules, persist-credentials (+1 more)

### Community 225 - "Community 225"
Cohesion: 0.28
Nodes (3): ExtensionsForm, OneByte, TwoByte

### Community 226 - "Community 226"
Cohesion: 0.50
Nodes (4): client(), main(), recv_agent(), recv_client()

### Community 227 - "Community 227"
Cohesion: 0.25
Nodes (5): ClipDirection, .ALL, Both, FromTarget, ToTarget

### Community 228 - "Community 228"
Cohesion: 0.32
Nodes (3): trackpad_pixels_accumulate(), wheel_lines_are_whole_steps(), WheelAccumulator

### Community 229 - "Community 229"
Cohesion: 0.43
Nodes (8): glkvm, glkvm-agent, glkvm-cli, glkvm-clip, glkvm-gui, glkvm-media, glkvm-view, str0m

### Community 230 - "Community 230"
Cohesion: 0.36
Nodes (8): str0m 0.24.1 changelog, Rtc::inject_rtp, str0m glkvm FEC patch, str0m crypto backends, str0m RTP mode API, Sans-IO design pattern, Single-mutation / drain run-loop invariant, str0m Sans-IO WebRTC library

### Community 231 - "Community 231"
Cohesion: 0.39
Nodes (3): H264ProfileLevel, .FALLBACK, test_profile_level_parsing()

### Community 233 - "Community 233"
Cohesion: 0.39
Nodes (3): test(), ValueHistory<T>, with_value_test()

### Community 234 - "Community 234"
Cohesion: 0.29
Nodes (3): Foundation, annexB(), VideoToolbox

### Community 238 - "Community 238"
Cohesion: 0.40
Nodes (5): Phase 2 console app CPU/energy results, GUI control socket (GLKVM_CONTROL), glkvmctl.py automation client, glkvm-gui eframe/egui app, Zero-copy IOSurface/Metal rendering

### Community 239 - "Community 239"
Cohesion: 0.33
Nodes (5): daemon, description, name, path, place

### Community 240 - "Community 240"
Cohesion: 0.33
Nodes (6): U2: Direct video /api/media/ws frames, U3: streamer lifecycle, Direct H.264 transport (/api/media/ws), glkvm-media video/audio sources, Janus WebRTC transport (/janus/ws), VideoToolbox H.264 decoder

### Community 241 - "Community 241"
Cohesion: 0.40
Nodes (3): .PROFILES, BitPattern, test_iop_pattern_matching()

### Community 242 - "Community 242"
Cohesion: 0.33
Nodes (5): RtcSctpState, AwaitAssociationEstablished, AwaitRemoteAssociation, Established, Uninited

### Community 243 - "Community 243"
Cohesion: 0.40
Nodes (5): ConsoleAction, Disconnect, None, TargetChanged, ToggleFullscreen

### Community 244 - "Community 244"
Cohesion: 0.40
Nodes (5): linux-crypto-test, name, runs-on, strategy, timeout-minutes

### Community 245 - "Community 245"
Cohesion: 0.40
Nodes (5): windows-crypto-test, name, runs-on, strategy, timeout-minutes

### Community 246 - "Community 246"
Cohesion: 0.40
Nodes (5): platform, features, name, os, target

### Community 251 - "Community 251"
Cohesion: 0.50
Nodes (4): BufferedThresholdConfig, Configured, Desired, Unconfigured

### Community 252 - "Community 252"
Cohesion: 0.83
Nodes (4): glkvm-gui logo (logo-256.png), Mouse cursor / pointer input, KVM remote control, Remote display / monitor

### Community 254 - "Community 254"
Cohesion: 0.67
Nodes (3): build_linux(), CARGO_PROFILE_RELEASE_STRIP, build-agents.sh script

### Community 255 - "Community 255"
Cohesion: 0.50
Nodes (4): macos-crypto-test, name, runs-on, timeout-minutes

### Community 256 - "Community 256"
Cohesion: 0.50
Nodes (4): on, schedule, workflow_dispatch, cron

### Community 260 - "Community 260"
Cohesion: 0.67
Nodes (3): Phase 0 baseline measurements, Measurement targets M1-M7, Phases 0-3 roadmap

## Knowledge Gaps
- **752 isolated node(s):** `glkvm-agent`, `Set`, `SetForInsert`, `RestoreSoon`, `Mismatch` (+747 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 2251 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **33 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Console` connect `GUI Console` to `Device Commands`, `Community 67`, `Community 109`, `Community 237`, `Clip Bridge`, `Community 111`, `Community 77`, `Community 178`, `Event Sessions`, `Community 116`, `Community 117`, `Community 53`, `Community 87`, `Community 48`, `Session Info`, `Community 58`, `Community 125`, `Bridge Setup`?**
  _High betweenness centrality (0.032) - this node is a cross-community bridge._
- **What connects `glkvm-agent`, `Set`, `SetForInsert` to the rest of the system?**
  _752 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `H266 Packetizer` be split into smaller, more focused modules?**
  _Cohesion score 0.06307102858826996 - nodes in this community are weakly interconnected._
- **Why does `Event` connect `RTC Events` to `Community 89`, `Negotiated Media`, `Media Model`, `Community 133`, `Stats`, `Community 43`, `Rtc Core`, `Clip Bridge`, `Session Utils`, `Community 83`, `Community 51`, `Stream RX`, `Community 57`, `Session Mgmt`?**
  _High betweenness centrality (0.027) - this node is a cross-community bridge._
- **Should `Stream TX/Mid` be split into smaller, more focused modules?**
  _Cohesion score 0.03902165795369679 - nodes in this community are weakly interconnected._
- **Why does `()` connect `Community 128` to `Community 210`, `Community 84`, `Community 63`?**
  _High betweenness centrality (0.027) - this node is a cross-community bridge._
- **Should `CLI Commands` be split into smaller, more focused modules?**
  _Cohesion score 0.028334669874365143 - nodes in this community are weakly interconnected._