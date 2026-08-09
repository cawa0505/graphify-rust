//! Content-Length framing for MCP stdio transport (MCP 2025-11-25 spec).
//!
//! Messages are framed as `Content-Length: <n>\r\n\r\n` followed by exactly
//! `<n>` bytes of JSON-RPC body. All plugin subprocesses speak this framing;
//! graphify-mcp's own client-facing connection keeps its legacy newline-delimited
//! format (see design D2 in plugin-scan-v1).

use std::io::{self, Read, Write};

/// Maximum accepted message body size (8 MiB) to bound a malformed header.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Encodes a JSON-RPC body into a framed message.
pub fn encode(body: &str) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Writes a framed message to `writer`.
pub fn write_message<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
    writer.write_all(&encode(body))?;
    writer.flush()
}

/// Reads one framed message from `reader`, returning the JSON body.
///
/// Returns `Ok(None)` on clean EOF at a message boundary. A truncated body or
/// malformed header is an `io::Error`.
pub fn read_message<R: Read>(reader: &mut R) -> io::Result<Option<String>> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];

    // Read headers until an empty line terminates the header block.
    loop {
        let n = reader.read(&mut byte)?;
        if n == 0 {
            // EOF mid-header: only clean if we saw no header bytes at all.
            return if header.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF in message header",
                ))
            };
        }
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "header too large",
            ));
        }
    }

    let header_text = String::from_utf8_lossy(&header);
    let content_length = parse_content_length(&header_text)?;

    if content_length > MAX_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("body too large: {content_length}"),
        ));
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    String::from_utf8(body).map(Some).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("body is not valid UTF-8: {e}"),
        )
    })
}

/// Reads a stream of framed messages until EOF, returning every body.
#[cfg(test)]
fn read_all<R: Read>(reader: &mut R) -> io::Result<Vec<String>> {
    let mut bodies = Vec::new();
    while let Some(body) = read_message(reader)? {
        bodies.push(body);
    }
    Ok(bodies)
}

fn parse_content_length(header: &str) -> io::Result<usize> {
    let value = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim().eq_ignore_ascii_case("Content-Length")).then_some(value.trim())
        })
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
        })?;

    value.parse::<usize>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid Content-Length: {value}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_encode_framing() -> anyhow::Result<()> {
        let framed = encode(r#"{"jsonrpc":"2.0"}"#);
        let text = String::from_utf8(framed)?;
        assert!(
            text.starts_with("Content-Length: 17\r\n\r\n"),
            "got: {text}"
        );
        assert!(text.ends_with(r#"{"jsonrpc":"2.0"}"#));
        Ok(())
    }

    #[test]
    fn test_round_trip_single_message() -> anyhow::Result<()> {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let mut cursor = Cursor::new(encode(body));
        let decoded = read_message(&mut cursor)?;
        assert_eq!(decoded.as_deref(), Some(body));
        Ok(())
    }

    #[test]
    fn test_round_trip_multi_message() -> anyhow::Result<()> {
        let bodies = [
            r#"{"jsonrpc":"2.0","id":1,"method":"a"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"b","params":{"x":1}}"#,
        ];
        let mut stream = Vec::new();
        for b in &bodies {
            stream.extend_from_slice(&encode(b));
        }
        let decoded = read_all(&mut Cursor::new(stream))?;
        assert_eq!(decoded, bodies.to_vec());
        Ok(())
    }

    #[test]
    fn test_clean_eof_is_none() -> anyhow::Result<()> {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        assert!(read_message(&mut cursor)?.is_none());
        Ok(())
    }

    #[test]
    fn test_eof_mid_header_is_error() {
        let mut cursor = Cursor::new(b"Content-Length: 5\r\n".to_vec());
        assert!(
            read_message(&mut cursor).is_err(),
            "truncated header must error"
        );
    }

    #[test]
    fn test_missing_content_length_is_error() {
        let mut cursor = Cursor::new(b"X-Foo: bar\r\n\r\n{}".to_vec());
        assert!(
            read_message(&mut cursor).is_err(),
            "missing header must error"
        );
    }

    #[test]
    fn test_truncated_body_is_error() {
        let mut cursor = Cursor::new(b"Content-Length: 100\r\n\r\nshort".to_vec());
        assert!(
            read_message(&mut cursor).is_err(),
            "truncated body must error"
        );
    }
}
