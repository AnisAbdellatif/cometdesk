//! FlexFEC-03 (RFC 8627) receiver: rebuilds lost media packets from repair packets.
//!
//! The device's Pion server sends one repair stream next to the video. Each repair packet is
//! the XOR of a set of media packets, named by a base sequence number and a bit mask (gl-pion
//! uses an interleaved mask, so a burst of lost packets hits different repair packets). A
//! repair packet whose covered packets are all present except one rebuilds that one; a rebuilt
//! packet can in turn complete another repair packet, so recovery runs to a fixed point.
//!
//! gl-pion's layout differs from a textbook reading of the RFC in two ways, both found by
//! comparing repair packets with the media packets they protect:
//! - The XOR of payload bytes starts after the 4-byte header-extension preamble (profile and
//!   length) when the media packet has one, so it covers the extension values (transport-cc
//!   sequence number) and the payload. The length field still counts everything after the
//!   12-byte fixed header.
//! - The mask is a 15-bit and a 31-bit word (6 bytes) and its `k` bits cannot be trusted.
//!   Groups are 16 packets wide, so the last interleaved column reaches offset 15 in the
//!   second word; then the first word says "no more words", the second one says "more", and
//!   a further 4 bytes of zeros precede the repair payload.
//!
//! This module is pure: the WebRTC engine feeds it decrypted packets and injects what it
//! returns.

use std::collections::BTreeMap;

/// Media packets and repair packets kept for recovery, counted in sequence numbers.
const WINDOW: u64 = 1024;
/// Repair packets kept while waiting for the rest of their group.
const MAX_REPAIR: usize = 128;

/// A media packet as the XOR sees it.
#[derive(Clone)]
struct Media {
    /// P, X and CC bits of the first RTP byte.
    flags: u8,
    /// Marker bit and payload type.
    marker_pt: u8,
    timestamp: u32,
    /// Bytes of CSRC list and header extension at the start of `body`.
    tail_len: usize,
    /// CSRC list, header extension and payload: everything after the fixed 12-byte header.
    body: Vec<u8>,
}

impl Media {
    /// The bytes that enter the XOR (see the module docs).
    fn region(&self) -> &[u8] {
        &self.body[region_start(self.flags).min(self.body.len())..]
    }
}

/// Where the XORed bytes start in the body of a packet with these flags: past the CSRC list
/// (the low nibble counts them, 4 bytes each) and the 4-byte header-extension preamble when
/// the X bit is set.
fn region_start(flags: u8) -> usize {
    (flags & 0x0f) as usize * 4 + if flags & 0x10 != 0 { 4 } else { 0 }
}

/// XORs `src` into `dst`, growing `dst` with zeros as needed.
fn xor_into(dst: &mut Vec<u8>, src: &[u8]) {
    if dst.len() < src.len() {
        dst.resize(src.len(), 0);
    }
    for (d, s) in dst.iter_mut().zip(src) {
        *d ^= s;
    }
}

/// A parsed repair packet.
struct Repair {
    /// Extended sequence number of the first covered position.
    base: u64,
    /// Offsets from `base` of the covered media packets.
    covered: Vec<u16>,
    ssrc: u32,
    flags: u8,
    marker_pt: u8,
    length: u16,
    timestamp: u32,
    payload: Vec<u8>,
}

/// A media packet rebuilt from repair data. Header extensions are lost with the packet.
#[derive(Debug, PartialEq, Eq)]
pub struct Recovered {
    pub ssrc: u32,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub marker: bool,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}

/// Counters for logging.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub repair: u64,
    pub recovered: u64,
    /// Complete groups whose XOR did not cancel out: the packet layout is not what we assume.
    pub inconsistent: u64,
}

pub struct Decoder {
    /// Highest extended sequence number seen on the media stream.
    top: Option<u64>,
    media: BTreeMap<u64, Media>,
    repair: Vec<Repair>,
    stats: Stats,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extends a 16-bit sequence number next to `reference`.
fn extend(reference: u64, seq: u16) -> u64 {
    let cycle = reference & !0xffff;
    let candidates = [cycle.wrapping_sub(0x10000), cycle, cycle + 0x10000];
    candidates
        .into_iter()
        .map(|c| c.wrapping_add(seq as u64))
        .min_by_key(|c| c.abs_diff(reference))
        .unwrap_or(seq as u64)
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            top: None,
            media: BTreeMap::new(),
            repair: Vec::new(),
            stats: Stats::default(),
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// A media packet arrived. `tail` is the CSRC list and header extension, `payload` what
    /// follows. Returns packets that this one made recoverable.
    pub fn media(
        &mut self,
        flags: u8,
        marker_pt: u8,
        sequence_number: u16,
        timestamp: u32,
        tail: &[u8],
        payload: &[u8],
    ) -> Vec<Recovered> {
        let seq = match self.top {
            Some(top) => extend(top, sequence_number),
            None => sequence_number as u64,
        };
        self.top = Some(self.top.map_or(seq, |t| t.max(seq)));
        let mut body = Vec::with_capacity(tail.len() + payload.len());
        body.extend_from_slice(tail);
        body.extend_from_slice(payload);
        self.media.insert(
            seq,
            Media {
                flags: flags & 0x3f,
                marker_pt,
                timestamp,
                tail_len: tail.len(),
                body,
            },
        );
        self.trim();
        self.solve()
    }

    /// A repair packet arrived (decrypted RTP payload).
    pub fn repair(&mut self, payload: &[u8]) -> Vec<Recovered> {
        let Some(top) = self.top else {
            return Vec::new();
        };
        let Some(r) = parse_repair(payload, top) else {
            return Vec::new();
        };
        self.stats.repair += 1;
        if r.base + 0x10000 < top {
            return Vec::new();
        }
        self.repair.push(r);
        self.trim();
        self.solve()
    }

    fn trim(&mut self) {
        if let Some(top) = self.top {
            let floor = top.saturating_sub(WINDOW);
            self.media = self.media.split_off(&floor);
            self.repair
                .retain(|r| r.base + (r.covered.last().copied().unwrap_or(0) as u64) >= floor);
        }
        if self.repair.len() > MAX_REPAIR {
            let excess = self.repair.len() - MAX_REPAIR;
            self.repair.drain(..excess);
        }
    }

    /// Applies every repair packet that is one packet short, until nothing changes.
    fn solve(&mut self) -> Vec<Recovered> {
        let mut out = Vec::new();
        loop {
            let mut progress = false;
            let mut keep = Vec::with_capacity(self.repair.len());
            for r in std::mem::take(&mut self.repair) {
                let missing: Vec<u64> = r
                    .covered
                    .iter()
                    .map(|o| r.base + *o as u64)
                    .filter(|s| !self.media.contains_key(s))
                    .collect();
                match missing.as_slice() {
                    [] => {
                        if !self.consistent(&r) {
                            self.stats.inconsistent += 1;
                        }
                    }
                    [one] => {
                        if let Some(rec) = self.rebuild(&r, *one) {
                            self.stats.recovered += 1;
                            out.push(rec);
                            progress = true;
                        }
                    }
                    _ => {
                        keep.push(r);
                    }
                }
            }
            self.repair = keep;
            if !progress {
                break;
            }
        }
        // Repair packets that stay unsolved are retried when more media arrives.
        out
    }

    /// With every covered packet present the XOR must cancel to zero.
    fn consistent(&self, r: &Repair) -> bool {
        let mut flags = r.flags;
        let mut marker_pt = r.marker_pt;
        let mut length = r.length;
        let mut timestamp = r.timestamp;
        let mut region = r.payload.clone();
        for o in &r.covered {
            let Some(m) = self.media.get(&(r.base + *o as u64)) else {
                return false;
            };
            flags ^= m.flags;
            marker_pt ^= m.marker_pt;
            length ^= m.body.len() as u16;
            timestamp ^= m.timestamp;
            xor_into(&mut region, m.region());
        }
        let ok = flags == 0
            && marker_pt == 0
            && length == 0
            && timestamp == 0
            && region.iter().all(|b| *b == 0);
        if !ok {
            tracing::debug!(
                flags, marker_pt, length, timestamp,
                nonzero = region.iter().filter(|b| **b != 0).count(),
                repair_len = r.payload.len(), covered = ?r.covered,
                "fec group does not cancel"
            );
        }
        ok
    }

    fn rebuild(&mut self, r: &Repair, missing: u64) -> Option<Recovered> {
        let mut flags = r.flags;
        let mut marker_pt = r.marker_pt;
        let mut length = r.length;
        let mut timestamp = r.timestamp;
        let mut region = r.payload.clone();
        let mut tail_len = None;
        for o in &r.covered {
            let s = r.base + *o as u64;
            if s == missing {
                continue;
            }
            let m = self.media.get(&s)?;
            flags ^= m.flags;
            marker_pt ^= m.marker_pt;
            length ^= m.body.len() as u16;
            timestamp ^= m.timestamp;
            xor_into(&mut region, m.region());
            tail_len.get_or_insert(m.tail_len);
        }
        // The extension of the lost packet is not recoverable (it sits outside the XOR in
        // part); assume it is as long as its neighbours', which holds for a stream whose
        // extensions are fixed.
        let tail_len = tail_len?;
        let length = length as usize;
        let start = region_start(flags);
        let payload_at = tail_len.checked_sub(start)?;
        let end = length.checked_sub(start)?;
        if end > region.len() || payload_at > end {
            return None;
        }
        let payload = region[payload_at..end].to_vec();
        let mut body = vec![0u8; tail_len];
        body.extend_from_slice(&payload);
        self.media.insert(
            missing,
            Media {
                flags,
                marker_pt,
                timestamp,
                tail_len,
                body,
            },
        );
        Some(Recovered {
            ssrc: r.ssrc,
            sequence_number: missing as u16,
            timestamp,
            marker: marker_pt & 0x80 != 0,
            payload_type: marker_pt & 0x7f,
            payload,
        })
    }
}

/// Parses a FlexFEC-03 repair packet payload (RFC 8627 §4.2) that protects one stream.
fn parse_repair(p: &[u8], top: u64) -> Option<Repair> {
    if p.len() < 24 {
        return None;
    }
    // R and F must be 0: no retransmission, flexible mask.
    if p[0] & 0xc0 != 0 {
        return None;
    }
    let flags = p[0] & 0x3f;
    let marker_pt = p[1];
    let length = u16::from_be_bytes([p[2], p[3]]);
    let timestamp = u32::from_be_bytes([p[4], p[5], p[6], p[7]]);
    if p[8] != 1 {
        return None;
    }
    let ssrc = u32::from_be_bytes([p[12], p[13], p[14], p[15]]);
    let base = extend(top, u16::from_be_bytes([p[16], p[17]]));
    // Mask: a 15-bit word and a 31-bit word, each led by a `k` bit that is ignored (see the
    // module docs). Bit 0 after the `k` bit is the packet at `base`.
    let w1 = u16::from_be_bytes([p[18], p[19]]) as u64;
    let w2 = u32::from_be_bytes([p[20], p[21], p[22], p[23]]) as u64;
    let covered: Vec<u16> = (0..15u16)
        .filter(|i| w1 >> (14 - i) & 1 == 1)
        .chain(
            (0..31u16)
                .filter(|i| w2 >> (30 - i) & 1 == 1)
                .map(|i| 15 + i),
        )
        .collect();
    // gl-pion appends another zero word when the second one carries mask bits.
    let at = if w2 & 0x7fff_ffff != 0 { 28 } else { 24 };
    Some(Repair {
        base,
        covered,
        ssrc,
        flags,
        marker_pt,
        length,
        timestamp,
        payload: p.get(at..)?.to_vec(),
    })
}

#[cfg(test)]
mod tests {
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
}
