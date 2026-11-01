//! Target info, power/layout/volume/display control, file transfer, screenshot, and notification tests.

use crate::*;

use super::{pair, pump};

#[test]
fn info_goes_from_agent_to_gui_only() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let info = Info {
        keyboard: Some("xkb:de".into()),
        os_version: Some("Ubuntu 24.04".into()),
        session: SessionState::Locked,
        uptime_secs: Some(5),
        ..Info::default()
    };
    a.send_info(&info).unwrap();
    c.send_info(&info).unwrap();
    let (ce, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(ce, vec![Event::Info(info)]);
    assert!(ae.is_empty());
}

#[test]
fn power_and_layout_round_trip_and_wrong_role() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let id = c.send_power(PowerAction::Restart).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        vec![Event::Power {
            id,
            action: PowerAction::Restart
        }]
    );
    a.send_ack(id, Err("denied".into())).unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ce,
        vec![Event::Ack {
            id,
            result: Err("denied".into())
        }]
    );
    let id = c.send_set_layout("xkb:de").unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        vec![Event::SetLayout {
            id,
            layout: "xkb:de".into()
        }]
    );
    a.send_power(PowerAction::Lock).unwrap();
    assert!(pump(&mut c, &mut a).is_err());
}

#[test]
fn file_round_trip_detects_corruption() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let id = c.send_file_offer("a.bin", 6, Purpose::Save).unwrap();
    c.send_file_data(id, b"abc").unwrap();
    c.send_file_data(id, b"def").unwrap();
    c.send_file_end(id).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        vec![
            Event::FileOffer {
                id,
                name: "a.bin".into(),
                size: 6,
                purpose: Purpose::Save
            },
            Event::FileData {
                id,
                data: b"abc".to_vec()
            },
            Event::FileData {
                id,
                data: b"def".to_vec()
            },
            Event::FileEnd { id, intact: true },
        ]
    );
    a.send_file_result(id, Ok("/tmp/a.bin".into())).unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ce,
        vec![Event::FileResult {
            id,
            result: Ok("/tmp/a.bin".into())
        }]
    );
    // Data for a cancelled file is not sent, and one that is already in flight is ignored.
    let id = c.send_file_offer("b", 1, Purpose::Clipboard).unwrap();
    c.send_file_cancel(id).unwrap();
    c.send_file_data(id, b"x").unwrap_err();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(ae.len(), 2);
    // Both directions work: the agent can send files too.
    let id = a.send_file_offer("c", 0, Purpose::Clipboard).unwrap();
    a.send_file_end(id).unwrap();
    a.send_files_ready(&[id]).unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(ce.last(), Some(&Event::FilesReady { ids: vec![id] }));
}

#[test]
fn clipboard_file_streams_into_staging() {
    let dir = std::env::temp_dir().join(format!("cometdesk-clip-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("data.bin");
    let bytes: Vec<u8> = (0..200_000u32).map(|i| (i * 7) as u8).collect();
    std::fs::write(&src, &bytes).unwrap();

    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let mut rx = FileReceiver::new(dir.join("staging"));
    let mut tx = FileSender::start(&mut c, &src, "data.bin", Purpose::Clipboard).unwrap();
    let mut done = false;
    while !done {
        done = matches!(tx.pump(&mut c).unwrap(), Pump::Finished);
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        for ev in ae {
            match ev {
                Event::FileOffer {
                    id,
                    name,
                    size,
                    purpose,
                } => rx.offer(id, &name, size, purpose).unwrap(),
                Event::FileData { id, data } => rx.data(id, &data).unwrap(),
                Event::FileEnd { id, intact } => {
                    rx.end(id, intact).unwrap();
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }
    let staged = rx.take_staged(&[tx.id]).unwrap();
    assert_eq!(staged[0].file_name().unwrap(), "data.bin");
    assert_eq!(std::fs::read(&staged[0]).unwrap(), bytes);
    assert!(rx.take_staged(&[tx.id]).is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn notifications_go_to_the_gui_and_are_cut() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    a.send_notify(&"t".repeat(300), "done\u{7}").unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ce,
        vec![Event::Notify {
            title: "t".repeat(MAX_NOTIFY_TITLE),
            body: "done ".into()
        }]
    );
    c.send_notify("x", "y").unwrap();
    assert!(pump(&mut c, &mut a).is_err());
}

#[test]
fn load_does_not_trigger_a_resend() {
    let a = Info::default();
    let b = Info {
        load: Some(Load {
            cpu_percent: Some(9),
            ..Load::default()
        }),
        ..Info::default()
    };
    assert!(a.same_as(&b));
}

#[test]
fn screenshot_round_trip_and_failure() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let id = c.send_capture(Some(1)).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        vec![Event::Capture {
            id,
            display: Some(1)
        }]
    );
    let png: Vec<u8> = (0..150_000u32).map(|i| i as u8).collect();
    a.send_screenshot(id, &png).unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(ce, vec![Event::Screenshot { id, png }]);
    a.send_capture_failed(id, "locked".into()).unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ce,
        vec![Event::CaptureFailed {
            id,
            reason: "locked".into()
        }]
    );
}

#[test]
fn volume_display_listing_and_fetch_round_trip() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let v = c.send_set_volume(Some(250), Some(true)).unwrap();
    let d = c.send_set_display(1, Some((1920, 1080)), true).unwrap();
    let l = c.send_list_dir(DirTarget::Home).unwrap();
    let f = c.send_fetch("/home/u", "a.txt").unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        [
            Event::SetVolume {
                id: v,
                level: Some(100),
                muted: Some(true)
            },
            Event::SetDisplay {
                id: d,
                display: 1,
                mode: Some((1920, 1080)),
                primary: true
            },
            Event::ListDir {
                id: l,
                target: DirTarget::Home
            },
            Event::Fetch {
                id: f,
                dir: "/home/u".into(),
                name: "a.txt".into()
            },
        ]
    );
    // A huge folder is cut to one message and says so.
    let entries = (0..5000)
        .map(|i| DirEntry {
            name: format!("file-number-{i}.txt"),
            dir: false,
            size: 1,
        })
        .collect();
    a.send_dir_list(
        l,
        Ok(Listing {
            path: "/home/u".into(),
            entries,
            truncated: false,
        }),
    )
    .unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    let [
        Event::DirList {
            result: Ok(list), ..
        },
    ] = &ce[..]
    else {
        panic!("{ce:?}")
    };
    assert!(list.truncated && !list.entries.is_empty() && list.entries.len() < 5000);
    // Requests only go GUI → agent.
    assert!(a.send_fetch("/", "x").is_ok());
    assert!(c.receive(&a.take_outgoing()).is_err());
}
