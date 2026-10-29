//! H.264 / H.265 Annex B parsing: start-code splitting, parameter sets, AVCC conversion.
//!
//! The device sends Annex B access units (NAL units separated by `00 00 01` or
//! `00 00 00 01`). VideoToolbox wants length-prefixed NAL units (AVCC/HVCC) plus a format
//! description built from the parameter sets.

use crate::Codec;

/// Iterates over NAL unit payloads (without start codes) in an Annex B buffer.
pub fn nal_units(data: &[u8]) -> NalUnits<'_> {
    NalUnits {
        data,
        pos: first_start(data).unwrap_or(data.len()),
    }
}

pub struct NalUnits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for NalUnits<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        loop {
            if self.pos >= self.data.len() {
                return None;
            }
            let start = self.pos;
            let (end, next) = match find_start_code(&self.data[start..]) {
                Some((at, len)) => (start + at, start + at + len),
                None => (self.data.len(), self.data.len()),
            };
            self.pos = next;
            // Trailing zero bytes belong to the next start code (`00 00 00 01`).
            let mut nal = &self.data[start..end];
            while let [rest @ .., 0] = nal {
                nal = rest;
            }
            if !nal.is_empty() {
                return Some(nal);
            }
        }
    }
}

/// Position just past the first start code.
fn first_start(data: &[u8]) -> Option<usize> {
    find_start_code(data).map(|(at, len)| at + len)
}

/// Finds `00 00 01`, returning (offset, length of start code = 3).
fn find_start_code(data: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 3 <= data.len() {
        // Skip quickly: a start code needs data[i+2] == 1 and two zeros before it.
        if data[i + 2] > 1 {
            i += 3;
        } else if data[i + 2] == 1 && data[i] == 0 && data[i + 1] == 0 {
            return Some((i, 3));
        } else {
            i += 1;
        }
    }
    None
}

/// NAL unit type for the given codec.
pub fn nal_type(codec: Codec, nal: &[u8]) -> u8 {
    match (codec, nal.first()) {
        (_, None) => 0,
        (Codec::H264, Some(b)) => b & 0x1f,
        (Codec::H265, Some(b)) => (b >> 1) & 0x3f,
    }
}

/// Classification needed by the decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalKind {
    /// Parameter set (H.264 SPS/PPS, H.265 VPS/SPS/PPS).
    ParamSet,
    /// Slice data that goes to the decoder.
    Slice { idr: bool },
    /// Access-unit delimiter, SEI, filler, ...: not needed for decoding.
    Other,
}

pub fn classify(codec: Codec, nal: &[u8]) -> NalKind {
    let t = nal_type(codec, nal);
    match codec {
        Codec::H264 => match t {
            7 | 8 => NalKind::ParamSet,
            5 => NalKind::Slice { idr: true },
            1..=4 => NalKind::Slice { idr: false },
            _ => NalKind::Other,
        },
        Codec::H265 => match t {
            32..=34 => NalKind::ParamSet,
            16..=21 => NalKind::Slice { idr: true },
            0..=9 => NalKind::Slice { idr: false },
            _ => NalKind::Other,
        },
    }
}

/// One access unit split for VideoToolbox.
#[derive(Debug, Default)]
pub struct Split<'a> {
    /// Parameter sets found in this access unit, in stream order.
    pub param_sets: Vec<&'a [u8]>,
    /// Slices as 4-byte big-endian length-prefixed NAL units.
    pub avcc: Vec<u8>,
    pub idr: bool,
}

/// Splits an Annex B access unit into parameter sets and an AVCC slice payload.
/// `avcc` is reused to avoid an allocation per frame.
pub fn split_access_unit<'a>(codec: Codec, data: &'a [u8], mut avcc: Vec<u8>) -> Split<'a> {
    avcc.clear();
    let mut out = Split {
        param_sets: Vec::new(),
        avcc: Vec::new(),
        idr: false,
    };
    for nal in nal_units(data) {
        match classify(codec, nal) {
            NalKind::ParamSet => out.param_sets.push(nal),
            NalKind::Slice { idr } => {
                out.idr |= idr;
                avcc.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                avcc.extend_from_slice(nal);
            }
            NalKind::Other => {}
        }
    }
    out.avcc = avcc;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS: &[u8] = &[0x67, 0x42, 0xe0, 0x1f, 0xaa];
    const PPS: &[u8] = &[0x68, 0xce, 0x3c, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x00, 0x03, 0x01];
    const P: &[u8] = &[0x41, 0x9a, 0x02];
    const AUD: &[u8] = &[0x09, 0xf0];

    fn annexb(parts: &[&[u8]], long: bool) -> Vec<u8> {
        let mut v = vec![];
        for p in parts {
            if long {
                v.extend_from_slice(&[0, 0, 0, 1]);
            } else {
                v.extend_from_slice(&[0, 0, 1]);
            }
            v.extend_from_slice(p);
        }
        v
    }

    #[test]
    fn splits_long_and_short_start_codes() {
        for long in [false, true] {
            let data = annexb(&[SPS, PPS, IDR], long);
            let nals: Vec<_> = nal_units(&data).collect();
            assert_eq!(nals, vec![SPS, PPS, IDR], "long={long}");
        }
    }

    #[test]
    fn mixed_start_codes_and_leading_garbage() {
        let mut data = vec![0xff, 0x00];
        data.extend(annexb(&[SPS], true));
        data.extend(annexb(&[P], false));
        let nals: Vec<_> = nal_units(&data).collect();
        assert_eq!(nals, vec![SPS, P]);
    }

    #[test]
    fn emulation_prevention_bytes_are_not_start_codes() {
        // 00 00 03 01 inside IDR must not split.
        let data = annexb(&[IDR], true);
        assert_eq!(nal_units(&data).count(), 1);
    }

    #[test]
    fn empty_and_no_start_code() {
        assert_eq!(nal_units(&[]).count(), 0);
        assert_eq!(nal_units(&[1, 2, 3, 4]).count(), 0);
        assert_eq!(nal_units(&[0, 0, 1]).count(), 0);
    }

    #[test]
    fn h264_classification() {
        assert_eq!(classify(Codec::H264, SPS), NalKind::ParamSet);
        assert_eq!(classify(Codec::H264, PPS), NalKind::ParamSet);
        assert_eq!(classify(Codec::H264, IDR), NalKind::Slice { idr: true });
        assert_eq!(classify(Codec::H264, P), NalKind::Slice { idr: false });
        assert_eq!(classify(Codec::H264, AUD), NalKind::Other);
    }

    #[test]
    fn h265_classification() {
        // NAL header: type in bits 1..=6 of the first byte.
        let vps = [32 << 1, 1];
        let idr = [19 << 1, 1];
        let trail = [1 << 1, 1];
        let sei = [39 << 1, 1];
        assert_eq!(classify(Codec::H265, &vps), NalKind::ParamSet);
        assert_eq!(classify(Codec::H265, &idr), NalKind::Slice { idr: true });
        assert_eq!(classify(Codec::H265, &trail), NalKind::Slice { idr: false });
        assert_eq!(classify(Codec::H265, &sei), NalKind::Other);
    }

    #[test]
    fn access_unit_to_avcc() {
        let data = annexb(&[AUD, SPS, PPS, IDR], true);
        let s = split_access_unit(Codec::H264, &data, Vec::new());
        assert_eq!(s.param_sets, vec![SPS, PPS]);
        assert!(s.idr);
        let mut expected = (IDR.len() as u32).to_be_bytes().to_vec();
        expected.extend_from_slice(IDR);
        assert_eq!(s.avcc, expected);
    }

    #[test]
    fn multi_slice_frame() {
        let data = annexb(&[P, P], false);
        let s = split_access_unit(Codec::H264, &data, Vec::with_capacity(64));
        assert!(!s.idr);
        assert!(s.param_sets.is_empty());
        assert_eq!(s.avcc.len(), 2 * (4 + P.len()));
    }
}
