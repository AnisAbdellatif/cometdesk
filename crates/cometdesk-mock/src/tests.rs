//! Unit tests for the mock's request reader.

use tokio::io::{AsyncWriteExt, duplex};

use crate::http::read_request;

/// Feeds `wire` to `read_request` through an in-memory pipe.
async fn parse(wire: Vec<u8>) -> (Option<crate::http::Request>, Vec<u8>) {
    let (mut client, mut server) = duplex(64 * 1024);
    let writer = tokio::spawn(async move {
        let _ = client.write_all(&wire).await;
        client.shutdown().await.ok();
        client
    });
    let mut buf = Vec::new();
    let req = read_request(&mut server, &mut buf).await.unwrap();
    writer.abort();
    (req, buf)
}

#[tokio::test]
async fn chunked_body_is_decoded_and_pipelined_bytes_stay() {
    let wire = b"POST /api/x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\nGET /next HTTP/1.1\r\n\r\n";
    let (req, rest) = parse(wire.to_vec()).await;
    let req = req.unwrap();
    assert_eq!((req.body.as_str(), req.body_len), ("abcde", 5));
    assert_eq!(rest, b"GET /next HTTP/1.1\r\n\r\n");
}

#[tokio::test]
async fn truncated_chunked_body_is_none() {
    let (req, _) =
        parse(b"POST /a HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel".to_vec()).await;
    assert!(req.is_none());
}

#[tokio::test]
async fn large_body_is_counted_not_stored() {
    let n = 5 << 20;
    let mut wire = format!("POST /a HTTP/1.1\r\nContent-Length: {n}\r\n\r\n").into_bytes();
    wire.extend(vec![b'x'; n]);
    let (req, rest) = parse(wire).await;
    let req = req.unwrap();
    assert_eq!(req.body_len, n);
    assert!(req.body.len() <= 1 << 20, "kept {}", req.body.len());
    assert!(rest.is_empty());
}

#[tokio::test]
async fn oversized_headers_are_refused() {
    let mut wire = b"GET /a HTTP/1.1\r\n".to_vec();
    wire.extend(vec![b'a'; 200 * 1024]);
    let (req, _) = parse(wire).await;
    assert!(req.is_none());
}
