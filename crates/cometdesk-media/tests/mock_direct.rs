//! Direct video over `/api/media/ws` against the `cometdesk-mock` device: no hardware needed.

use std::sync::Arc;
use std::time::Duration;

use cometdesk::events::{ConnState, EventOptions, EventSession};
use cometdesk::tls::PinStore;
use cometdesk::{Credentials, Session};
use cometdesk_media::Codec;
use cometdesk_media::annexb::{nal_type, nal_units};
use cometdesk_media::direct::{self, DirectOptions};
use cometdesk_mock::{Config, MockDevice, PASSWORD, USER};
use tokio::time::timeout;

async fn session(dev: &MockDevice) -> Session {
    Session::login(
        &dev.host(),
        &Credentials::new(USER, PASSWORD),
        Arc::new(PinStore::in_memory()),
    )
    .await
    .unwrap()
}

async fn streaming_events(dev: &MockDevice) -> EventSession {
    let es = EventSession::spawn(session(dev).await, EventOptions::default());
    let mut conn = es.conn_state();
    timeout(
        Duration::from_secs(5),
        conn.wait_for(|c| *c == ConnState::Connected),
    )
    .await
    .unwrap()
    .unwrap();
    es
}

fn types(data: &[u8]) -> Vec<u8> {
    nal_units(data).map(|n| nal_type(Codec::H264, n)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_arrive_with_parameter_sets_on_keyframes() {
    let dev = MockDevice::start(Config::default()).await;
    let _es = streaming_events(&dev).await;
    let media = direct::start(session(&dev).await, DirectOptions::default());
    let (mut video, _media) = media.take_video();
    let first = timeout(Duration::from_secs(5), video.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(first.keyframe);
    let t = types(&first.data);
    assert!(
        t.contains(&7) && t.contains(&8) && t.contains(&5),
        "SPS/PPS/IDR expected: {t:?}"
    );
    let mut deltas = 0;
    while deltas < 5 {
        let f = timeout(Duration::from_secs(5), video.recv())
            .await
            .unwrap()
            .unwrap();
        if !f.keyframe {
            assert!(types(&f.data).contains(&1), "delta frames are slices");
            deltas += 1;
        }
    }
    assert!(dev.media_frames() >= 6);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_frames_without_a_streaming_event_session() {
    let dev = MockDevice::start(Config::default()).await;
    let media = direct::start(session(&dev).await, DirectOptions::default());
    let (mut video, _media) = media.take_video();
    assert!(
        timeout(Duration::from_millis(800), video.recv())
            .await
            .is_err()
    );
    assert_eq!(dev.media_frames(), 0, "U3: media alone produces no frames");
    assert_eq!(dev.media_sessions(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keyframe_request_restarts_at_an_idr() {
    let dev = MockDevice::start(Config::default()).await;
    let _es = streaming_events(&dev).await;
    let media = direct::start(session(&dev).await, DirectOptions::default());
    let control = media.control.clone();
    let (mut video, _media) = media.take_video();
    // Skip into the GOP, then ask for a keyframe: the next IDR must follow promptly.
    for _ in 0..5 {
        timeout(Duration::from_secs(5), video.recv())
            .await
            .unwrap()
            .unwrap();
    }
    control.request_keyframe();
    timeout(Duration::from_secs(2), async {
        loop {
            let f = video.recv().await.unwrap();
            if f.keyframe {
                break;
            }
        }
    })
    .await
    .expect("keyframe after request");
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn videotoolbox_decodes_the_mock_stream() {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dev = MockDevice::start(Config::default()).await;
    let _es = streaming_events(&dev).await;
    let media = direct::start(session(&dev).await, DirectOptions::default());
    let (mut video, _media) = media.take_video();
    let decoded = Arc::new(AtomicUsize::new(0));
    let size = Arc::new(Mutex::new((0, 0)));
    let (d2, s2) = (decoded.clone(), size.clone());
    let mut dec = cometdesk_media::vt::VtDecoder::new(move |r| {
        let f = r.expect("decode");
        *s2.lock().unwrap() = (f.width(), f.height());
        d2.fetch_add(1, Ordering::Relaxed);
    });
    for _ in 0..70 {
        let f = timeout(Duration::from_secs(5), video.recv())
            .await
            .unwrap()
            .unwrap();
        dec.decode(&f).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        decoded.load(Ordering::Relaxed) >= 60,
        "decoded {}",
        decoded.load(Ordering::Relaxed)
    );
    assert_eq!(*size.lock().unwrap(), (640, 360));
}
