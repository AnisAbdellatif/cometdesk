//! Rust SDK for GL.iNet GLKVM devices (GLKVM KVMD firmware).
//!
//! - [`Session`]: login (incl. 2FA and two-step approval), REST and WebSocket access.
//! - [`api`]: typed REST APIs (ATX, Wake-on-LAN, HID, streamer, info, capabilities).
//! - [`events`]: the `/api/ws` session: device state events and the HID input path.
//! - [`hid`]: the binary HID wire format and input helpers (coalescing, wheel accumulation).
//! - [`tls`]: trust-on-first-use certificate pinning for the device's self-signed cert.

pub mod api;
pub mod client;
pub mod env;
pub mod error;
pub mod events;
pub mod hid;
pub mod tls;

pub use client::{Connection, Credentials, LoginState, PendingLogin, Session, SystemConfig};
pub use error::{Error, Result};
