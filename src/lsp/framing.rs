use std::io::{self, Read, Write};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

pub fn encode_message(value: &Value) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(value).context("serialize LSP message")?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(anyhow!(
            "LSP message too large: {} bytes (max {MAX_MESSAGE_BYTES})",
            body.len()
        ));
    }

    let mut frame = Vec::with_capacity(body.len() + 64);
    write!(frame, "Content-Length: {}\r\n\r\n", body.len()).context("write header")?;
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn write_message_sync(writer: &mut impl Write, value: &Value) -> Result<()> {
    let frame = encode_message(value)?;
    writer.write_all(&frame).context("write LSP frame")?;
    writer.flush().context("flush LSP frame")?;
    Ok(())
}

pub async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> Result<()> {
    let frame = encode_message(value)?;
    writer.write_all(&frame).await.context("write LSP frame")?;
    writer.flush().await.context("flush LSP frame")?;
    Ok(())
}

pub fn read_message_sync(reader: &mut impl Read) -> Result<Option<Value>> {
    let content_length = read_content_length_sync(reader)?;
    let Some(len) = content_length else {
        return Ok(None);
    };

    if len > MAX_MESSAGE_BYTES {
        return Err(anyhow!(
            "LSP message too large: content-length {len} (max {MAX_MESSAGE_BYTES})"
        ));
    }

    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .context("read LSP message body")?;

    let value = serde_json::from_slice(&body).context("parse LSP JSON body")?;
    Ok(Some(value))
}

async fn read_content_length_async<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<usize>> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let bytes = read_line_async(reader, &mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .context("parse Content-Length header")?,
                );
            }
        }
    }

    Ok(content_length)
}

pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<Value>> {
    let content_length = read_content_length_async(reader).await?;
    let Some(len) = content_length else {
        return Ok(None);
    };

    if len > MAX_MESSAGE_BYTES {
        return Err(anyhow!(
            "LSP message too large: content-length {len} (max {MAX_MESSAGE_BYTES})"
        ));
    }

    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .await
        .context("read LSP message body")?;

    let value = serde_json::from_slice(&body).context("parse LSP JSON body")?;
    Ok(Some(value))
}

fn read_content_length_sync(reader: &mut impl Read) -> Result<Option<usize>> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let bytes = read_line_sync(reader, &mut line)?;
        if bytes == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .context("parse Content-Length header")?,
                );
            }
        }
    }

    Ok(content_length)
}

fn read_line_sync(reader: &mut impl Read, line: &mut String) -> io::Result<usize> {
    line.clear();
    let mut byte = [0u8; 1];
    let mut total = 0usize;

    loop {
        let n = reader.read(&mut byte)?;
        if n == 0 {
            break;
        }

        total += n;
        let ch = byte[0] as char;
        line.push(ch);
        if ch == '\n' {
            break;
        }
    }

    Ok(total)
}

async fn read_line_async<R: AsyncRead + Unpin>(
    reader: &mut R,
    line: &mut String,
) -> io::Result<usize> {
    line.clear();
    let mut byte = [0u8; 1];
    let mut total = 0usize;

    loop {
        let n = reader.read(&mut byte).await?;
        if n == 0 {
            break;
        }

        total += n;
        let ch = byte[0] as char;
        line.push(ch);
        if ch == '\n' {
            break;
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn encode_includes_content_length_header() {
        let msg = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let frame = encode_message(&msg).unwrap();
        let text = String::from_utf8(frame).unwrap();
        assert!(text.starts_with("Content-Length:"));
        assert!(text.contains("\r\n\r\n"));
    }

    #[test]
    fn round_trip_sync() {
        let original = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "shutdown",
            "params": null
        });

        let frame = encode_message(&original).unwrap();
        let mut cursor = Cursor::new(frame);
        let decoded = read_message_sync(&mut cursor).unwrap().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn partial_header_then_body() {
        let msg = json!({"jsonrpc":"2.0","method":"exit"});
        let frame = encode_message(&msg).unwrap();

        let split = frame
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap()
            + 4;
        let mut reader = Cursor::new(&frame[..split]);
        assert!(read_message_sync(&mut reader).is_err());

        let mut reader = Cursor::new(frame);
        let decoded = read_message_sync(&mut reader).unwrap().unwrap();
        assert_eq!(decoded, msg);
    }

    #[tokio::test]
    async fn round_trip_async() {
        let original = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });

        let frame = encode_message(&original).unwrap();
        let mut cursor = Cursor::new(frame);
        let decoded = read_message(&mut cursor).await.unwrap().unwrap();
        assert_eq!(decoded, original);
    }
}
