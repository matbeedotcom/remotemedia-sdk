//! Routing-channel helpers for `RuntimeData::Text`.
//!
//! Text frames carry an optional routing channel (`"tts"` = speakable,
//! `"ui"` = written/display, …). On the Python↔Rust IPC wire the
//! channel is inlined into the payload via a `[0x00][len:u8][channel]`
//! header, keeping the legacy "raw UTF-8" format intact for untagged
//! text. These helpers parse / emit that header at the `&str` level so
//! Rust nodes, the WebRTC adapter, and the control-bus broadcast can
//! all dispatch on channel without reaching into the multiprocess IPC
//! module (which would drag in the `multiprocess` feature gate).
//!
//! Byte-level equivalents live in `crate::python::multiprocess::data_transfer`
//! when the `multiprocess` feature is enabled; both agree on the wire
//! format.

/// Default channel name applied when a text payload carries no channel
/// header.
pub const TEXT_CHANNEL_DEFAULT: &str = "tts";

/// Parse a possibly-tagged text string into `(channel, content)`.
///
/// Layout:
/// - `[0x00][channel_len:u8][channel:utf8][text:utf8]` → `(channel, text)`
/// - `<anything else>` → `("tts", input)` (legacy untagged path)
pub fn split_text_str(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == 0x00 {
        let channel_len = bytes[1] as usize;
        if 2 + channel_len <= bytes.len() {
            if let Ok(channel) = std::str::from_utf8(&bytes[2..2 + channel_len]) {
                if !channel.is_empty() {
                    // The remaining bytes are the tail of the original
                    // UTF-8 string, so they're still valid UTF-8.
                    if let Ok(content) = std::str::from_utf8(&bytes[2 + channel_len..]) {
                        return (channel, content);
                    }
                }
            }
        }
    }
    (TEXT_CHANNEL_DEFAULT, s)
}

/// Build a tagged text string from a `(channel, text)` pair. `"tts"` /
/// empty channel returns the text unchanged (legacy format).
pub fn tag_text_str(text: &str, channel: &str) -> String {
    if channel.is_empty() || channel == TEXT_CHANNEL_DEFAULT {
        return text.to_string();
    }
    let mut channel_bytes = channel.as_bytes();
    if channel_bytes.len() > u8::MAX as usize {
        channel_bytes = &channel_bytes[..u8::MAX as usize];
    }
    let mut out = Vec::with_capacity(2 + channel_bytes.len() + text.len());
    out.push(0x00);
    out.push(channel_bytes.len() as u8);
    out.extend_from_slice(channel_bytes);
    out.extend_from_slice(text.as_bytes());
    // All parts are valid UTF-8 (U+0000 + ASCII channel + UTF-8 text).
    String::from_utf8(out).expect("tagged text payload is valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_channel_is_untagged() {
        let tagged = tag_text_str("hello", TEXT_CHANNEL_DEFAULT);
        assert_eq!(tagged, "hello");
        let (ch, content) = split_text_str(&tagged);
        assert_eq!(ch, TEXT_CHANNEL_DEFAULT);
        assert_eq!(content, "hello");
    }

    #[test]
    fn roundtrip_ui_channel() {
        let tagged = tag_text_str("# heading\n", "ui");
        let (ch, content) = split_text_str(&tagged);
        assert_eq!(ch, "ui");
        assert_eq!(content, "# heading\n");
    }

    #[test]
    fn legacy_untagged_text_defaults_to_tts() {
        let (ch, content) = split_text_str("plain legacy text");
        assert_eq!(ch, TEXT_CHANNEL_DEFAULT);
        assert_eq!(content, "plain legacy text");
    }

    #[test]
    fn malformed_header_falls_back_cleanly() {
        // 0x00 prefix but channel_len (99) points past the end of the
        // payload — split should fall back to the default channel
        // rather than slice out-of-bounds.
        let mut v = vec![0x00u8, 99];
        v.extend_from_slice(b"short");
        let bad = String::from_utf8(v).unwrap();
        let (ch, _) = split_text_str(&bad);
        assert_eq!(ch, TEXT_CHANNEL_DEFAULT);
    }
}
