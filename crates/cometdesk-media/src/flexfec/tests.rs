//! Unit tests for the FlexFEC-03 packet repair decoder.
//! Entry points: exercised via `cargo test -p cometdesk-media`.
//! To change decoder logic, read `mod.rs`.

use super::*;

struct Pkt {
    seq: u16,
    ts: u32,
    marker: bool,
    tail: Vec<u8>,
    payload: Vec<u8>,
}

fn pkt(seq: u16, len: usize) -> Pkt {
    Pkt {
        seq,
        ts: 90_000 + seq as u32 * 1500,
        marker: seq.is_multiple_of(5),
        // One-byte header extension holding a transport-cc sequence number, as on the wire.
        tail: vec![0xbe, 0xde, 0, 1, 0x31, (seq >> 8) as u8, seq as u8, 0],
        payload: (0..len)
            .map(|i| (seq as usize * 7 + i * 13) as u8)
            .collect(),
    }
}

fn flags(_: &Pkt) -> u8 {
    0x10
}

fn marker_pt(p: &Pkt) -> u8 {
    96 | if p.marker { 0x80 } else { 0 }
}

/// Builds a repair packet over `pkts` (offsets below 46), laid out like gl-pion's.
fn encode(base: u16, pkts: &[(u16, &Pkt)]) -> Vec<u8> {
    let mut flag = 0;
    let mut mpt = 0;
    let mut len = 0u16;
    let mut ts = 0u32;
    let mut body = Vec::new();
    let (mut w1, mut w2) = (0u16, 0u32);
    for (off, p) in pkts {
        flag ^= flags(p);
        mpt ^= marker_pt(p);
        let b: Vec<u8> = p.tail.iter().chain(&p.payload).copied().collect();
        len ^= b.len() as u16;
        ts ^= p.ts;
        xor_into(&mut body, &b[4..]);
        if *off < 15 {
            w1 |= 1 << (14 - off);
        } else {
            w2 |= 1 << (30 - (off - 15));
        }
    }
    let mut out = vec![flag, mpt];
    out.extend(len.to_be_bytes());
    out.extend(ts.to_be_bytes());
    out.extend([1, 0, 0, 0]);
    out.extend(0xdead_beefu32.to_be_bytes());
    out.extend(base.to_be_bytes());
    out.extend((w1 | 0x8000).to_be_bytes());
    out.extend(w2.to_be_bytes());
    if w2 != 0 {
        out.extend([0; 4]);
    }
    out.extend(body);
    out
}

fn feed(d: &mut Decoder, p: &Pkt) -> Vec<Recovered> {
    d.media(flags(p), marker_pt(p), p.seq, p.ts, &p.tail, &p.payload)
}

fn expect(p: &Pkt, r: &Recovered) {
    assert_eq!(r.sequence_number, p.seq);
    assert_eq!(r.timestamp, p.ts);
    assert_eq!(r.marker, p.marker);
    assert_eq!(r.payload_type, 96);
    assert_eq!(r.payload, p.payload);
}

#[test]
fn rebuilds_one_lost_packet() {
    let pkts: Vec<Pkt> = (100..104)
        .map(|s| pkt(s, 50 + s as usize * 3 % 40))
        .collect();
    let covered: Vec<(u16, &Pkt)> = pkts
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u16, p))
        .collect();
    let fec = encode(100, &covered);
    let mut d = Decoder::new();
    for (i, p) in pkts.iter().enumerate() {
        if i != 2 {
            assert!(feed(&mut d, p).is_empty());
        }
    }
    let out = d.repair(&fec);
    assert_eq!(out.len(), 1);
    expect(&pkts[2], &out[0]);
    assert_eq!(d.stats().recovered, 1);
}

#[test]
fn repair_before_the_last_media_packet() {
    let pkts: Vec<Pkt> = (10..14).map(|s| pkt(s, 80)).collect();
    let covered: Vec<(u16, &Pkt)> = pkts
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u16, p))
        .collect();
    let fec = encode(10, &covered);
    let mut d = Decoder::new();
    feed(&mut d, &pkts[0]);
    feed(&mut d, &pkts[1]);
    assert!(d.repair(&fec).is_empty());
    let out = feed(&mut d, &pkts[3]);
    assert_eq!(out.len(), 1);
    expect(&pkts[2], &out[0]);
}

#[test]
fn two_lost_packets_of_one_group_need_two_repairs() {
    // Interleaved: repair A covers 0,2 and repair B covers 1,3.
    let pkts: Vec<Pkt> = (0..4).map(|s| pkt(s, 60)).collect();
    let a = encode(0, &[(0, &pkts[0]), (2, &pkts[2])]);
    let b = encode(0, &[(1, &pkts[1]), (3, &pkts[3])]);
    let mut d = Decoder::new();
    feed(&mut d, &pkts[0]);
    feed(&mut d, &pkts[3]);
    let mut out = d.repair(&a);
    out.extend(d.repair(&b));
    assert_eq!(out.len(), 2);
    out.sort_by_key(|r| r.sequence_number);
    expect(&pkts[1], &out[0]);
    expect(&pkts[2], &out[1]);
}

#[test]
fn a_rebuilt_packet_completes_another_repair() {
    // One repair over 0,1,2; another over 2,3. Lose 1 and 2: the second repair cannot
    // help until the first has rebuilt... it can't: both are short by two. Lose only 2
    // and 3 instead: repair one rebuilds 2 (needs 0 and 1), then repair two rebuilds 3.
    let pkts: Vec<Pkt> = (0..4).map(|s| pkt(s, 40)).collect();
    let one = encode(0, &[(0, &pkts[0]), (1, &pkts[1]), (2, &pkts[2])]);
    let two = encode(0, &[(2, &pkts[2]), (3, &pkts[3])]);
    let mut d = Decoder::new();
    feed(&mut d, &pkts[0]);
    feed(&mut d, &pkts[1]);
    assert!(d.repair(&two).is_empty());
    let out = d.repair(&one);
    assert_eq!(out.len(), 2);
    expect(&pkts[2], &out[0]);
    expect(&pkts[3], &out[1]);
}

#[test]
fn long_masks_and_sequence_wraparound() {
    let base = 65_530u16;
    let pkts: Vec<Pkt> = (0..40)
        .map(|i| pkt(base.wrapping_add(i), 30 + i as usize))
        .collect();
    let covered: Vec<(u16, &Pkt)> = pkts
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u16, p))
        .collect();
    let fec = encode(base, &covered);
    let mut d = Decoder::new();
    for (i, p) in pkts.iter().enumerate() {
        if i != 30 {
            feed(&mut d, p);
        }
    }
    let out = d.repair(&fec);
    assert_eq!(out.len(), 1);
    expect(&pkts[30], &out[0]);
}

#[test]
fn complete_group_counts_as_consistent() {
    let pkts: Vec<Pkt> = (0..3).map(|s| pkt(s, 33)).collect();
    let covered: Vec<(u16, &Pkt)> = pkts
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u16, p))
        .collect();
    let fec = encode(0, &covered);
    let mut d = Decoder::new();
    for p in &pkts {
        feed(&mut d, p);
    }
    d.repair(&fec);
    assert_eq!(d.stats().inconsistent, 0);
    assert_eq!(d.stats().repair, 1);
}

#[test]
fn mask_of_gl_pion_with_a_misleading_k_bit() {
    // Offsets 3, 7, 11 in the first word with k = 0, offset 15 in the second.
    let mut p = vec![0u8; 28];
    p[8] = 1;
    p[18..20].copy_from_slice(&0x0888u16.to_be_bytes());
    p[20..24].copy_from_slice(&0xc000_0000u32.to_be_bytes());
    let r = parse_repair(&p, 0).unwrap();
    assert_eq!(r.covered, [3, 7, 11, 15]);
    assert!(r.payload.is_empty());
}

#[test]
fn region_start_skips_csrc_and_extension_preamble() {
    assert_eq!(region_start(0x00), 0); // no CSRC, no extension
    assert_eq!(region_start(0x10), 4); // extension only
    assert_eq!(region_start(0x02), 8); // two CSRCs, no extension
    assert_eq!(region_start(0x12), 12); // two CSRCs and an extension
}

#[test]
fn rebuilds_a_lost_packet_with_csrcs() {
    // Packets carry a 4-byte CSRC plus a header extension; the XOR must skip both.
    fn pkt_csrc(seq: u16) -> Pkt {
        let mut p = pkt(seq, 64);
        // One CSRC (4 bytes) ahead of the one-byte header extension.
        p.tail = [vec![0xca, 0xfe, 0xba, 0xbe], p.tail].concat();
        p
    }
    // flags: X bit + CSRC count 1.
    let flags = |_: &Pkt| 0x11u8;
    let feed = |d: &mut Decoder, p: &Pkt| {
        d.media(flags(p), marker_pt(p), p.seq, p.ts, &p.tail, &p.payload)
    };
    let encode = |base: u16, pkts: &[(u16, &Pkt)]| -> Vec<u8> {
        let (mut flag, mut mpt, mut len, mut ts) = (0u8, 0u8, 0u16, 0u32);
        let mut body = Vec::new();
        let (mut w1, mut w2) = (0u16, 0u32);
        for (off, p) in pkts {
            flag ^= flags(p);
            mpt ^= marker_pt(p);
            let b: Vec<u8> = p.tail.iter().chain(&p.payload).copied().collect();
            len ^= b.len() as u16;
            ts ^= p.ts;
            xor_into(&mut body, &b[region_start(flags(p))..]);
            if *off < 15 {
                w1 |= 1 << (14 - off);
            } else {
                w2 |= 1 << (30 - (off - 15));
            }
        }
        let mut out = vec![flag, mpt];
        out.extend(len.to_be_bytes());
        out.extend(ts.to_be_bytes());
        out.extend([1, 0, 0, 0]);
        out.extend(0xdead_beefu32.to_be_bytes());
        out.extend(base.to_be_bytes());
        out.extend((w1 | 0x8000).to_be_bytes());
        out.extend(w2.to_be_bytes());
        out.extend(body);
        out
    };
    let pkts: Vec<Pkt> = (200..204).map(pkt_csrc).collect();
    let covered: Vec<(u16, &Pkt)> = pkts
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u16, p))
        .collect();
    let fec = encode(200, &covered);
    let mut d = Decoder::new();
    for (i, p) in pkts.iter().enumerate() {
        if i != 1 {
            feed(&mut d, p);
        }
    }
    let out = d.repair(&fec);
    assert_eq!(out.len(), 1);
    expect(&pkts[1], &out[0]);
}

#[test]
fn garbage_is_ignored() {
    let mut d = Decoder::new();
    feed(&mut d, &pkt(1, 10));
    assert!(d.repair(&[0u8; 5]).is_empty());
    assert!(d.repair(&[0xc0; 40]).is_empty());
}
