//! Protocol version numbers, frame size limits, and transfer caps.
//! Read this file before changing message sizes or bumping `PROTOCOL`.

/// Bumped on incompatible protocol changes; both ends refuse a different major.
pub const PROTOCOL: u16 = 8;
/// From this protocol on, a peer of another version can still be sent a new agent: the
/// session is *limited* to file transfers for [`Purpose::Update`]. Keep the first messages
/// (`Hello`, `Ping`, `Pong`) and the file messages where they are in [`Msg`], and only ever
/// append variants, so an older agent can still decode what such a session needs.
pub const UPDATE_PROTOCOL: u16 = 8;
/// Longest text for one [`Endpoint::send_insert`] (one Noise message).
pub const MAX_INSERT: usize = 60 * 1024;
/// Largest clipboard item (encoded: UTF-8 text or PNG).
pub const MAX_ITEM: usize = 16 * 1024 * 1024;
/// Largest file for [`Endpoint::send_file_offer`]; files are streamed, not held in memory.
pub const MAX_FILE: u64 = 64 * 1024 * 1024 * 1024;
/// Longest piece of file data for one [`Endpoint::send_file_data`].
pub const FILE_CHUNK: usize = 60 * 1024;
/// Copied files travel on their own, so each one and the whole batch stay small enough to
/// send without asking; larger ones go through "Send file".
pub const MAX_CLIP_FILES_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_CLIP_FILES: usize = 512;
/// Largest agent program for [`Purpose::Update`].
pub const MAX_UPDATE: u64 = 256 * 1024 * 1024;
/// A directory listing is cut to what fits one message.
pub(crate) const MAX_LISTING_BYTES: usize = 56 * 1024;
/// Notification text the agent forwards is cut to this many characters.
pub const MAX_NOTIFY_TITLE: usize = 100;
pub const MAX_NOTIFY_BODY: usize = 500;

pub(crate) const NOISE_PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";
pub(crate) const PROLOGUE: &[u8] = b"cometdesk-clip v1";
pub(crate) const MAX_NOISE_MSG: usize = 65535;
pub(crate) const TAG_LEN: usize = 16;
pub(crate) const CHUNK: usize = 60 * 1024;
