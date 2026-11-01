//! Session, handshake, framing, clipboard item, and text insertion tests for `cometdesk-clip`.
//! Device control and file transfer tests live in `control`.

mod control;

use crate::wire::Msg;
use crate::*;

fn pair(secret_a: &Secret, secret_b: &Secret) -> (Endpoint, Endpoint) {
    let c = Endpoint::new(Role::Client, secret_a, Hello::local("1", "mac".into())).unwrap();
    let a = Endpoint::new(Role::Agent, secret_b, Hello::local("1", "laptop".into())).unwrap();
    (c, a)
}

/// Moves bytes both ways in small pieces until quiet; returns (client events, agent events).
fn pump(c: &mut Endpoint, a: &mut Endpoint) -> Result<(Vec<Event>, Vec<Event>)> {
    let (mut ce, mut ae) = (Vec::new(), Vec::new());
    while c.has_outgoing() || a.has_outgoing() {
        for piece in c.take_outgoing().chunks(7) {
            ae.extend(a.receive(piece)?);
        }
        for piece in a.take_outgoing().chunks(7) {
            ce.extend(c.receive(piece)?);
        }
    }
    Ok((ce, ae))
}

#[test]
fn handshake_hello_and_items_both_ways() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    let (ce, ae) = pump(&mut c, &mut a).unwrap();
    assert!(matches!(&ce[..], [Event::Ready(h)] if h.name == "laptop"));
    assert!(matches!(&ae[..], [Event::Ready(h)] if h.name == "mac"));

    c.set_policy(Policy {
        agent_may_send: false,
    })
    .unwrap();
    c.send_clip(&Content::Text("héllo\nworld".into())).unwrap();
    let img = Content::Image {
        width: 3,
        height: 2,
        rgba: (0..24).collect(),
    };
    a.send_clip(&img).unwrap();
    let (ce, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        vec![
            Event::Policy(Policy {
                agent_may_send: false
            }),
            Event::Clip(Content::Text("héllo\nworld".into()))
        ]
    );
    assert_eq!(ce, vec![Event::Clip(img)]);
}

#[test]
fn insert_round_trip() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let id = c.send_insert("a|b@c\n€", PastePref::CtrlShiftV).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ae,
        vec![Event::Insert {
            id,
            text: "a|b@c\n€".into(),
            paste: PastePref::CtrlShiftV
        }]
    );
    a.send_insert_reply(id, InsertReply::PasteNow(PasteKey::CtrlV))
        .unwrap();
    let (ce, _) = pump(&mut c, &mut a).unwrap();
    assert_eq!(
        ce,
        vec![Event::InsertReply {
            id,
            reply: InsertReply::PasteNow(PasteKey::CtrlV)
        }]
    );
    c.send_insert_pasted(id).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(ae, vec![Event::InsertPasted { id }]);
    assert!(matches!(
        c.send_insert(&"x".repeat(MAX_INSERT + 1), PastePref::Auto),
        Err(Error::TooLarge(_))
    ));
    // The agent cannot ask the GUI to insert.
    a.send(&Msg::Insert {
        id: 9,
        text: "x".into(),
        paste: PastePref::Auto,
    })
    .unwrap();
    assert!(pump(&mut c, &mut a).is_err());
}

#[test]
fn large_item_is_chunked() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    let text = "x".repeat(1_000_000);
    c.send_clip(&Content::Text(text.clone())).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert_eq!(ae, vec![Event::Clip(Content::Text(text))]);
}

#[test]
fn wrong_secret_fails_the_handshake() {
    let (mut c, mut a) = pair(&Secret::generate(), &Secret::generate());
    assert!(matches!(pump(&mut c, &mut a), Err(Error::Handshake)));
}

#[test]
fn ping_is_answered() {
    let s = Secret::generate();
    let (mut c, mut a) = pair(&s, &s);
    pump(&mut c, &mut a).unwrap();
    c.ping().unwrap();
    let (ce, ae) = pump(&mut c, &mut a).unwrap();
    assert!(ce.is_empty() && ae.is_empty());
}

#[test]
fn items_before_handshake_are_refused() {
    let s = Secret::generate();
    let (mut c, _a) = pair(&s, &s);
    assert!(matches!(
        c.send_clip(&Content::Text("x".into())),
        Err(Error::NotReady)
    ));
}

#[test]
fn garbage_frame_is_an_error() {
    let s = Secret::generate();
    let (_c, mut a) = pair(&s, &s);
    assert!(a.receive(&[0, 0, 0, 0]).is_err());
    let (_c, mut a) = pair(&s, &s);
    assert!(a.receive(&[0, 0, 0, 3, 1, 2, 3]).is_err());
}

#[test]
fn other_protocol_gets_updates_only() {
    let s = Secret::generate();
    let newer = Hello {
        protocol: PROTOCOL + 1,
        ..Hello::local("2", "mac".into())
    };
    let mut c = Endpoint::new(Role::Client, &s, newer).unwrap();
    let mut a = Endpoint::new(Role::Agent, &s, Hello::local("1", "laptop".into())).unwrap();
    let (ce, ae) = pump(&mut c, &mut a).unwrap();
    // (This end's own constant is still 8, so only the agent sees a mismatch.)
    assert!(matches!(ae[..], [Event::Mismatch(_)]) && matches!(ce[..], [Event::Ready(_)]));
    // Clipboard traffic is ignored; an update transfer goes through.
    c.send_clip(&Content::Text("hi".into())).unwrap();
    let id = c.send_file_offer("agent", 3, Purpose::Update).unwrap();
    c.send_file_data(id, b"abc").unwrap();
    c.send_file_end(id).unwrap();
    let (_, ae) = pump(&mut c, &mut a).unwrap();
    assert!(matches!(
        ae[..],
        [
            Event::FileOffer {
                purpose: Purpose::Update,
                ..
            },
            Event::FileData { .. },
            Event::FileEnd { intact: true, .. }
        ]
    ));
    // Other transfers are refused quietly.
    let id = c.send_file_offer("x", 1, Purpose::Save).unwrap();
    c.send_file_data(id, b"x").unwrap();
    assert!(pump(&mut c, &mut a).unwrap().1.is_empty());
}
