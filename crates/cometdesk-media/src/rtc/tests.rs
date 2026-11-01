//! Unit tests for the shared WebRTC engine (`is_keyframe`, `configure_rtc`, candidate parsing).
//! Entry points: exercised via `cargo test -p cometdesk-media`.
//! To change the WebRTC engine, read `mod.rs` and `audio.rs`.

use super::*;
use str0m::media::{Direction, MediaKind};

#[test]
fn codec_aware_keyframe_detection() {
    // H.264 IDR slice (type 5)
    let h264_idr = [0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84];
    assert!(is_keyframe(Codec::H264, &h264_idr));
    assert!(!is_keyframe(Codec::H265, &h264_idr));

    // H.264 non-IDR slice (type 1)
    let h264_delta = [0x00, 0x00, 0x00, 0x01, 0x41, 0x9a];
    assert!(!is_keyframe(Codec::H264, &h264_delta));
    assert!(!is_keyframe(Codec::H265, &h264_delta));

    // H.264 SPS (7) + PPS (8) without IDR
    let h264_params = [
        0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x00, 0x00, 0x01, 0x68, 0xce,
    ];
    assert!(!is_keyframe(Codec::H264, &h264_params));

    // H.264 SPS + PPS + IDR
    let h264_full_key = [
        0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x00, 0x00, 0x01, 0x68, 0xce, 0x00, 0x00, 0x00,
        0x01, 0x65, 0x88,
    ];
    assert!(is_keyframe(Codec::H264, &h264_full_key));

    // H.265 IRAP NAL types 16..=21:
    // Type 19 (IDR_W_RADL): (19 << 1) = 0x26
    let h265_idr_19 = [0x00, 0x00, 0x00, 0x01, 0x26, 0x01];
    assert!(is_keyframe(Codec::H265, &h265_idr_19));
    assert!(!is_keyframe(Codec::H264, &h265_idr_19));

    // Type 20 (IDR_N_LP): (20 << 1) = 0x28
    let h265_idr_20 = [0x00, 0x00, 0x00, 0x01, 0x28, 0x01];
    assert!(is_keyframe(Codec::H265, &h265_idr_20));

    // Type 21 (CRA_NUT): (21 << 1) = 0x2A
    let h265_cra = [0x00, 0x00, 0x00, 0x01, 0x2A, 0x01];
    assert!(is_keyframe(Codec::H265, &h265_cra));

    // Type 16 (BLA_W_LP): (16 << 1) = 0x20
    let h265_bla = [0x00, 0x00, 0x00, 0x01, 0x20, 0x01];
    assert!(is_keyframe(Codec::H265, &h265_bla));

    // H.265 non-IRAP slice (TRAIL_R, type 1): (1 << 1) = 0x02
    let h265_delta = [0x00, 0x00, 0x00, 0x01, 0x02, 0x01];
    assert!(!is_keyframe(Codec::H265, &h265_delta));
    assert!(!is_keyframe(Codec::H264, &h265_delta));

    // H.265 VPS (32=0x40), SPS (33=0x42), PPS (34=0x44) without IRAP
    let h265_params = [
        0x00, 0x00, 0x00, 0x01, 0x40, 0x01, 0x00, 0x00, 0x00, 0x01, 0x42, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x44, 0x01,
    ];
    assert!(!is_keyframe(Codec::H265, &h265_params));

    // H.265 VPS + SPS + PPS + IDR
    let h265_full_key = [
        0x00, 0x00, 0x00, 0x01, 0x40, 0x01, 0x00, 0x00, 0x00, 0x01, 0x42, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x44, 0x01, 0x00, 0x00, 0x00, 0x01, 0x26, 0x01,
    ];
    assert!(is_keyframe(Codec::H265, &h265_full_key));
}

#[test]
fn sdp_offer_prefers_h265_with_h264_fallback() {
    let mut rtc = configure_rtc(REORDERING_FEC, Codec::H265);
    let mut api = rtc.sdp_api();
    api.add_media(MediaKind::Video, Direction::RecvOnly, None, None, None);
    let (offer, _) = api.apply().expect("sdp offer");
    let sdp = offer.to_sdp_string();

    // Must offer both H.265 and H.264
    assert!(
        sdp.contains("a=rtpmap:102 H265/90000"),
        "missing H.265 rtpmap:\n{sdp}"
    );
    assert!(
        sdp.contains("H264/90000"),
        "missing H.264 rtpmap fallback:\n{sdp}"
    );

    // m=video line must place H.265 before H.264
    let m_video = sdp
        .lines()
        .find(|l| l.starts_with("m=video"))
        .expect("m=video line");
    let pts: Vec<u8> = m_video
        .split(' ')
        .skip(3)
        .filter_map(|p| p.parse().ok())
        .collect();
    assert!(pts.contains(&102), "m=video missing PT 102 (H.265)");
    let pos_265 = pts.iter().position(|&p| p == 102).unwrap();
    // Any H.264 PT should appear after H.265
    for (pt, codec) in [(96, "96"), (98, "98")] {
        if let Some(pos_264) = pts.iter().position(|&p| p == pt) {
            assert!(
                pos_265 < pos_264,
                "H.265 (pos {pos_265}) must come before H.264 PT {codec} (pos {pos_264})"
            );
        }
    }
}

#[test]
fn sdp_offer_h264_only_when_not_preferred() {
    let mut rtc = configure_rtc(REORDERING_FEC, Codec::H264);
    let mut api = rtc.sdp_api();
    api.add_media(MediaKind::Video, Direction::RecvOnly, None, None, None);
    let (offer, _) = api.apply().expect("sdp offer");
    let sdp = offer.to_sdp_string();

    assert!(!sdp.contains("H265"), "should not offer H.265:\n{sdp}");
    assert!(sdp.contains("H264/90000"), "missing H.264 rtpmap:\n{sdp}");
}
