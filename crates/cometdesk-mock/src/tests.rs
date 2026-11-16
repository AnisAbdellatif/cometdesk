//! Unit tests for the mock's wire helpers.

use crate::http::decode_chunked;

#[test]
fn chunked_decodes_complete_messages_only() {
    assert_eq!(
        decode_chunked(b"5\r\nhello\r\n0\r\n\r\n"),
        Some(b"hello".to_vec())
    );
    assert_eq!(
        decode_chunked(b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"),
        Some(b"abcde".to_vec())
    );
    assert_eq!(decode_chunked(b"5\r\nhel"), None);
    assert_eq!(decode_chunked(b"5\r\nhello\r\n"), None);
    assert_eq!(decode_chunked(b"zz\r\nhello\r\n0\r\n\r\n"), None);
}
