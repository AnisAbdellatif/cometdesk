//! The pairing secret shared by one GUI device entry and one agent.
//!
//! Shown and typed as 52 lowercase base32 characters (RFC 4648 alphabet, no padding), so the
//! GUI can type it into the target over HID. Dashes and spaces are ignored when parsing.

use std::fmt;

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

#[derive(Clone, PartialEq, Eq)]
pub struct Secret([u8; 32]);

impl Secret {
    pub fn generate() -> Secret {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).expect("OS random number generator");
        Secret(b)
    }

    pub fn from_bytes(b: [u8; 32]) -> Secret {
        Secret(b)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_code(&self) -> String {
        let mut out = String::with_capacity(52);
        let (mut acc, mut bits) = (0u32, 0u32);
        for &byte in &self.0 {
            acc = (acc << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
            }
        }
        if bits > 0 {
            out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
        }
        out
    }

    pub fn from_code(code: &str) -> Option<Secret> {
        let mut bytes = Vec::with_capacity(32);
        let (mut acc, mut bits) = (0u32, 0u32);
        for c in code.chars().filter(|c| !matches!(c, '-' | ' ')) {
            let v = ALPHABET
                .iter()
                .position(|&a| a as char == c.to_ascii_lowercase())? as u32;
            acc = (acc << 5) | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                bytes.push((acc >> bits) as u8);
            }
        }
        Some(Secret(bytes.try_into().ok()?))
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_round_trip() {
        for _ in 0..50 {
            let s = Secret::generate();
            let code = s.to_code();
            assert_eq!(code.len(), 52);
            assert_eq!(Secret::from_code(&code), Some(s.clone()));
            let spaced: String = code
                .chars()
                .enumerate()
                .flat_map(|(i, c)| (i > 0 && i % 4 == 0).then_some('-').into_iter().chain([c]))
                .collect();
            assert_eq!(Secret::from_code(&spaced.to_uppercase()), Some(s));
        }
    }

    #[test]
    fn bad_codes_are_rejected() {
        assert_eq!(Secret::from_code("abc"), None);
        assert_eq!(Secret::from_code(&"1".repeat(52)), None);
    }
}
