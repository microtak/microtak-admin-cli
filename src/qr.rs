//! Enrollment QR codes in the format TAK clients actually scan.
//!
//! Payload: the standard ATAK enrollment deep link,
//! `tak://com.atakmap.app/enroll?host=<host>&username=<name>&token=<token>`
//! -- scanning it makes ATAK / OmniTAK enroll over HTTPS against `host`
//! presenting `token` as the password for `username` (microtak-server
//! accepts an invite token that way, bound to that device name). OmniTAK
//! also reads `enrollmentport=`, `port=` (streaming) and `apiport=` (Marti
//! API); they're only added when they differ from the TAK defaults (8446,
//! 8089, 8443), keeping the code small and the link ATAK-compatible.

use anyhow::{bail, Context, Result};
use qrcode::render::unicode;
use qrcode::QrCode;

pub const DEFAULT_ENROLLMENT_PORT: u16 = 8446;
pub const DEFAULT_STREAMING_PORT: u16 = 8089;
pub const DEFAULT_API_PORT: u16 = 8443;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentLink {
    pub host: String,
    pub enrollment_port: u16,
    pub streaming_port: u16,
    pub api_port: u16,
    pub username: String,
    pub token: String,
}

impl EnrollmentLink {
    /// Build from the server's enrollment URL (`https://host[:port]`).
    pub fn from_enrollment_url(
        enrollment_url: &str,
        username: &str,
        token: &str,
        streaming_port: u16,
        api_port: u16,
    ) -> Result<Self> {
        let (host, enrollment_port) = split_https_url(enrollment_url)?;
        Ok(Self {
            host,
            enrollment_port,
            streaming_port,
            api_port,
            username: username.to_string(),
            token: token.to_string(),
        })
    }

    pub fn to_uri(&self) -> String {
        let mut uri = format!(
            "tak://com.atakmap.app/enroll?host={}&username={}&token={}",
            encode(&self.host),
            encode(&self.username),
            encode(&self.token)
        );
        if self.enrollment_port != DEFAULT_ENROLLMENT_PORT {
            uri.push_str(&format!("&enrollmentport={}", self.enrollment_port));
        }
        if self.streaming_port != DEFAULT_STREAMING_PORT {
            uri.push_str(&format!("&port={}", self.streaming_port));
        }
        if self.api_port != DEFAULT_API_PORT {
            uri.push_str(&format!("&apiport={}", self.api_port));
        }
        uri
    }
}

/// `https://host[:port][/]` → (host, port); port defaults to 443.
fn split_https_url(url: &str) -> Result<(String, u16)> {
    let lower = url.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("https://").map(|_| &url["https://".len()..]) else {
        bail!("the enrollment URL must be https:// (got '{url}')");
    };
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        bail!("the enrollment URL has no host (got '{url}')");
    }
    if authority.starts_with('[') {
        // TAK clients split host:port on the last ':' -- a bare IPv6
        // literal can't survive that. Use a DNS name or IPv4 address.
        bail!("IPv6 literals aren't supported in enrollment QR codes -- use a DNS name or IPv4 address");
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .parse()
                .with_context(|| format!("invalid port in enrollment URL '{url}'"))?;
            Ok((host.to_string(), port))
        }
        None => Ok((authority.to_string(), 443)),
    }
}

/// Percent-encode everything outside RFC 3986's unreserved set.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
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

    fn link(url: &str) -> EnrollmentLink {
        EnrollmentLink::from_enrollment_url(url, "phone-1", "abc123", 8089, 8443).unwrap()
    }

    #[test]
    fn default_ports_give_the_plain_atak_link() {
        assert_eq!(
            link("https://192.168.1.10:8446").to_uri(),
            "tak://com.atakmap.app/enroll?host=192.168.1.10&username=phone-1&token=abc123"
        );
    }

    /// Behind a reverse proxy enrollment is on 443 (no port in the URL):
    /// the link must say so, or the client would try 8446.
    #[test]
    fn a_non_default_enrollment_port_is_included() {
        assert_eq!(
            link("https://tak.example.com").to_uri(),
            "tak://com.atakmap.app/enroll?host=tak.example.com&username=phone-1&token=abc123&enrollmentport=443"
        );
    }

    #[test]
    fn non_default_streaming_and_api_ports_are_included() {
        let uri = EnrollmentLink::from_enrollment_url("https://h:8446/", "u", "t", 18089, 18443)
            .unwrap()
            .to_uri();
        assert!(uri.ends_with("&port=18089&apiport=18443"), "{uri}");
    }

    #[test]
    fn values_are_percent_encoded() {
        let uri = EnrollmentLink::from_enrollment_url("https://h:8446", "team a&b", "t=1", 8089, 8443)
            .unwrap()
            .to_uri();
        assert!(uri.contains("username=team%20a%26b&token=t%3D1"), "{uri}");
    }

    #[test]
    fn refuses_plain_http_and_ipv6_literals() {
        assert!(EnrollmentLink::from_enrollment_url("http://h:8446", "u", "t", 8089, 8443).is_err());
        assert!(EnrollmentLink::from_enrollment_url("https://[fd00::1]:8446", "u", "t", 8089, 8443).is_err());
    }

    /// A real QR encode/render round trip -- not just the payload string,
    /// but that `qrcode` can encode a full-length link and the renderer
    /// produces a real grid.
    #[test]
    fn a_real_token_payload_encodes_and_renders() {
        let uri = EnrollmentLink::from_enrollment_url(
            "https://192.168.1.50:8446",
            "phone-1",
            &"a".repeat(64),
            8089,
            8443,
        )
        .unwrap()
        .to_uri();
        let code = QrCode::new(uri.as_bytes()).unwrap();
        let rendered = code.render::<unicode::Dense1x2>().build();
        assert!(rendered.lines().count() > 5, "expected a real multi-row QR grid");
    }
}
