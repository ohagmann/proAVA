//! Base64 (RFC 4648, standard alphabet) for images in long texts.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let value = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes, ignoring whitespace; None for anything that is not Base64.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    let mut padding = false;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                padding = true;
                continue;
            }
            byte if byte.is_ascii_whitespace() => continue,
            _ => return None,
        };
        if padding {
            return None;
        }
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        for data in [&b"M"[..], b"Ma", b"Man", b"Mann", &[0, 255, 128, 7, 9]] {
            assert_eq!(super::decode(&super::encode(data)).unwrap(), data);
        }
        assert_eq!(super::encode(b"Man"), "TWFu");
        assert_eq!(super::decode("TW\nE=").unwrap(), b"Ma");
        assert!(super::decode("T*Fu").is_none());
    }
}
