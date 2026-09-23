//! Render an enrollment token as a QR code directly in the terminal.
//!
//! Payload is a plain `microtak-enroll:` URI carrying the enrollment
//! endpoint URL and the token as query parameters -- simple enough for
//! `microtak-admin-web` (or any future scanner-side tooling) to parse with
//! nothing fancier than splitting on `?`/`&`, and human-readable enough
//! that someone squinting at raw QR-decoder output can still tell what
//! it's for.

use anyhow::{Context, Result};
use qrcode::render::unicode;
use qrcode::QrCode;

pub fn enrollment_payload(enrollment_url: &str, token: &str) -> String {
    format!("microtak-enroll:?url={enrollment_url}&token={token}")
}

pub fn print_terminal_qr(payload: &str) -> Result<()> {
    let code = QrCode::new(payload.as_bytes()).context("encoding QR code")?;
    let rendered = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Dark)
        .light_color(unicode::Dense1x2::Light)
        .build();
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_carries_url_and_token() {
        let payload = enrollment_payload("http://server:8446", "abc123");
        assert_eq!(payload, "microtak-enroll:?url=http://server:8446&token=abc123");
    }

    /// A real QR encode/render round trip -- not just checking the payload
    /// string, but that `qrcode` can actually encode it and the renderer
    /// produces non-empty terminal output.
    #[test]
    fn a_real_token_payload_encodes_and_renders() {
        let payload = enrollment_payload("http://192.168.1.50:8446", &"a".repeat(64));
        let code = QrCode::new(payload.as_bytes()).unwrap();
        let rendered = code.render::<unicode::Dense1x2>().build();
        assert!(!rendered.is_empty());
        assert!(rendered.lines().count() > 5, "expected a real multi-row QR grid");
    }
}
