//! Rust SDK for GL.iNet GLKVM devices (GLKVM KVMD firmware).
//!
//! - [`Session`]: authenticated REST access and WebSocket connections to one device.
//! - [`events`]: the `/api/ws` session: device state events and the HID input path.
//! - [`hid`]: the binary HID wire format and input helpers (coalescing, wheel accumulation).
//! - [`tls`]: trust-on-first-use certificate pinning for the device's self-signed cert.

pub mod client;
pub mod env;
pub mod error;
pub mod events;
pub mod hid;
pub mod tls;

pub use client::{Credentials, Session, SystemConfig};
pub use error::{Error, Result};
