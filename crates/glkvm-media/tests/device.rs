//! Tests against a real device. Ignored by default; run with credentials in `.env`:
//!
//! ```text
//! cargo test -p glkvm-media --test device -- --ignored --test-threads=1
//! ```

#![cfg(target_os = "macos")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glkvm::events::{ConnState, EventOptions, EventSession};
use glkvm::tls::PinStore;
use glkvm::{Credentials, Session};
use glkvm_media::direct::{self, DirectOptions};
use glkvm_media::vt::VtDecoder;

async fn login() -> Session {
    glkvm::env::load_dotenv();
    let host = std::env::var("GLKVM_HOST").expect("GLKVM_HOST");
    let creds = Credentials::new(
        std::env::var("GLKVM_USER").unwrap_or_else(|_| "admin".into()),
        std::env::var("GLKVM_PASSWORD").expect("GLKVM_PASSWORD"),
    );
    let pins = Arc::new(PinStore::load_default().unwrap());
    Session::login(&host, &creds, pins).await.expect("login")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a device"]
async fn event_socket_connects_and_answers_pings() {
    let session = login().await;
    let events = EventSession::spawn(
        session,
        EventOptions {
            stream: false,
            ..Default::default()
        },
    );
    let mut conn = events.conn_state();
    tokio::time::timeout(
        Duration::from_secs(5),
        conn.wait_for(|s| *s == ConnState::Connected),
    )
    .await
    .expect("connect timeout")
    .unwrap();
    // Two ping intervals with pongs keep the session up.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(*conn.borrow(), ConnState::Connected);
    events.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a device"]
async fn direct_video_decodes_in_hardware() {
    let session = login().await;
    // Keeps the streamer running.
    let events = EventSession::spawn(session.clone(), EventOptions::default());
    let media = direct::start(session, DirectOptions::default());
    let (mut video, media) = media.take_video();

    let decoded = Arc::new(AtomicUsize::new(0));
    let size = Arc::new(Mutex::new((0, 0)));
    let (d2, s2) = (decoded.clone(), size.clone());
    let mut decoder = VtDecoder::new(move |r| {
        let f = r.expect("decode");
        *s2.lock().unwrap() = (f.width(), f.height());
        d2.fetch_add(1, Ordering::Relaxed);
    });

    let start = Instant::now();
    let mut first_key = None;
    let mut decode_time = Duration::ZERO;
    while start.elapsed() < Duration::from_secs(10) && decoded.load(Ordering::Relaxed) < 120 {
        let Ok(Some(frame)) = tokio::time::timeout(Duration::from_secs(5), video.recv()).await
        else {
            panic!("no video: {:?}", *media.state.borrow());
        };
        if first_key.is_none() {
            assert!(frame.keyframe, "first frame must be a keyframe");
            first_key = Some(start.elapsed());
        }
        let t = Instant::now();
        decoder.decode(&frame).expect("decode");
        decode_time += t.elapsed();
    }
    let n = decoded.load(Ordering::Relaxed);
    let (w, h) = *size.lock().unwrap();
    let per_frame = decode_time / n.max(1) as u32;
    println!("first keyframe after {first_key:?}; {n} frames {w}x{h}; decode {per_frame:?}/frame");
    assert!(n >= 120, "only {n} frames decoded");
    assert!(w >= 640 && h >= 360);
    assert!(
        per_frame < Duration::from_millis(8),
        "decode too slow: {per_frame:?}"
    );
    drop(media);
    events.close().await;
}

async fn first_keyframe(transport: glkvm_media::Transport) {
    let session = login().await;
    // Keeps the streamer running.
    let events = EventSession::spawn(session.clone(), EventOptions::default());
    let mut media = glkvm_media::start(
        session,
        glkvm_media::SourceOptions {
            transport,
            ..Default::default()
        },
    );
    let frame = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let f = media.video.recv().await.expect("source ended");
            if f.keyframe {
                return f;
            }
        }
    })
    .await
    .expect("no keyframe in 15 s");
    assert!(!frame.data.is_empty());
    events.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a device"]
async fn pion_delivers_a_keyframe() {
    first_keyframe(glkvm_media::Transport::Pion).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a device with gl_webrtc off"]
async fn janus_delivers_a_keyframe() {
    first_keyframe(glkvm_media::Transport::Janus).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a device and an audio source on the target"]
async fn pion_delivers_audio() {
    audio_arrives(glkvm_media::pion::start_audio).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a device in webrtc mode and an audio source on the target"]
async fn janus_delivers_audio() {
    audio_arrives(glkvm_media::janus::start_audio).await;
}

async fn audio_arrives(start: fn(Session) -> glkvm_media::audio::AudioSession) {
    let session = login().await;
    let _events = EventSession::spawn(session.clone(), EventOptions::default());
    let audio = start(session);
    let (mut packets, audio) = audio.take_packets();
    let first = tokio::time::timeout(Duration::from_secs(10), packets.recv())
        .await
        .unwrap_or_else(|_| panic!("no audio: {:?}", *audio.state.borrow()))
        .expect("audio source ended");
    let mut dec = opus_decoder::OpusDecoder::new(48_000, 2).unwrap();
    let mut pcm = vec![0f32; 5760 * 2];
    assert!(dec.decode_float(&first, &mut pcm, false).unwrap() > 0);
}
