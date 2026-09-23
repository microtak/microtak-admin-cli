//! Render a printable enrollment handout -- QR code plus manual
//! (type-it-by-hand) enrollment instructions -- as a single PDF file.
//!
//! Fonts are embedded at compile time (Liberation Sans, SIL Open Font
//! License -- see `assets/fonts/LICENSE-OFL.txt`) so the resulting binary
//! needs no font files on the target machine to generate a PDF.

use anyhow::{Context, Result};
use genpdf::elements::{Break, Image, OrderedList, Paragraph};
use genpdf::fonts::{FontData, FontFamily};
use genpdf::style::Style;
use genpdf::{Alignment, Document, Element, Scale, SimplePageDecorator};
use image::{DynamicImage, GrayImage, Luma};
use qrcode::{Color, QrCode};

const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");
const FONT_ITALIC: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Italic.ttf");
const FONT_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/LiberationSans-BoldItalic.ttf");

/// Pixels per QR module and the standard-recommended quiet-zone width (in
/// modules) around it -- without a quiet zone, some scanners fail to lock
/// onto the code at all.
const MODULE_PIXELS: u32 = 8;
const QUIET_ZONE_MODULES: u32 = 4;

pub enum Expiry {
    Never,
    At(String),
    /// The caller has no server-confirmed status for this token (e.g. the
    /// standalone `token pdf` command, which never contacts the server) --
    /// shown plainly rather than guessed at.
    Unknown,
}

impl std::fmt::Display for Expiry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expiry::Never => f.write_str("never"),
            Expiry::At(when) => f.write_str(when),
            Expiry::Unknown => f.write_str("unknown -- check `microtak-admin-cli token list`"),
        }
    }
}

pub struct HandoutInfo<'a> {
    pub token: &'a str,
    pub enrollment_url: &'a str,
    pub note: Option<&'a str>,
    pub expires_at: Expiry,
}

fn embedded_font_family() -> Result<FontFamily<FontData>> {
    Ok(FontFamily {
        regular: FontData::new(FONT_REGULAR.to_vec(), None).context("loading regular font")?,
        bold: FontData::new(FONT_BOLD.to_vec(), None).context("loading bold font")?,
        italic: FontData::new(FONT_ITALIC.to_vec(), None).context("loading italic font")?,
        bold_italic: FontData::new(FONT_BOLD_ITALIC.to_vec(), None)
            .context("loading bold italic font")?,
    })
}

/// Rasterize a QR code as a black-on-white image, by hand rather than via
/// `qrcode`'s own "image" feature -- that feature pins a newer `image`
/// major version than the one `genpdf` (and its `Image` element) accepts,
/// which would leave us with two incompatible `DynamicImage` types.
fn rasterize_qr(payload: &str) -> Result<DynamicImage> {
    let code = QrCode::new(payload.as_bytes()).context("encoding QR code")?;
    let modules = code.width() as u32;
    let colors = code.to_colors();
    let size = (modules + QUIET_ZONE_MODULES * 2) * MODULE_PIXELS;

    let mut buf = GrayImage::from_pixel(size, size, Luma([255u8]));
    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] != Color::Dark {
                continue;
            }
            let px0 = (x + QUIET_ZONE_MODULES) * MODULE_PIXELS;
            let py0 = (y + QUIET_ZONE_MODULES) * MODULE_PIXELS;
            for dy in 0..MODULE_PIXELS {
                for dx in 0..MODULE_PIXELS {
                    buf.put_pixel(px0 + dx, py0 + dy, Luma([0u8]));
                }
            }
        }
    }
    Ok(DynamicImage::ImageLuma8(buf))
}

pub fn write_enrollment_pdf(info: &HandoutInfo, out_path: &str) -> Result<()> {
    let mut doc = Document::new(embedded_font_family()?);
    doc.set_title("MicroTAK Enrollment");
    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(15);
    doc.set_page_decorator(decorator);

    doc.push(
        Paragraph::new("MicroTAK Device Enrollment").styled(Style::new().bold().with_font_size(20)),
    );
    doc.push(Break::new(1.0));

    doc.push(Paragraph::new(format!(
        "Enrollment URL: {}",
        info.enrollment_url
    )));
    doc.push(Paragraph::new(format!("Token: {}", info.token)));
    doc.push(Paragraph::new(format!("Expires: {}", info.expires_at)));
    if let Some(note) = info.note {
        doc.push(Paragraph::new(format!("Note: {note}")));
    }
    doc.push(Break::new(1.0));

    let payload = crate::qr::enrollment_payload(info.enrollment_url, info.token);
    let qr_image = rasterize_qr(&payload)?;
    doc.push(
        Image::from_dynamic_image(qr_image)
            .context("embedding QR code image")?
            .with_alignment(Alignment::Center)
            .with_scale(Scale::new(0.4, 0.4)),
    );
    doc.push(Paragraph::new("Scan with a compatible enrollment tool.").styled(Style::new().italic()));
    doc.push(Break::new(1.0));

    doc.push(
        Paragraph::new("Manual enrollment (if you can't scan the QR code)")
            .styled(Style::new().bold().with_font_size(14)),
    );
    doc.push(Break::new(0.5));
    doc.push(
        OrderedList::new()
            .element(Paragraph::new(
                "Install microtak-admin-cli on the device (or a machine that can reach it): \
                 https://github.com/microtak/microtak-admin-cli",
            ))
            .element(Paragraph::new(format!(
                "Run: microtak-admin-cli enroll --enrollment-url {} --cn <device-name> \
                 --token {} --out-cert device.pem --out-key device.key",
                info.enrollment_url, info.token,
            )))
            .element(Paragraph::new(
                "Replace <device-name> with a unique name for this device -- it becomes its \
                 identity everywhere in MicroTAK.",
            ))
            .element(Paragraph::new(
                "Configure your TAK client (ATAK/iTAK/WinTAK) with the resulting \
                 device.pem/device.key and the server's CA certificate.",
            )),
    );

    doc.render_to_file(out_path).context("writing PDF file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real QR encode/rasterize round trip: the finder-pattern corner
    /// must be dark, and the quiet-zone corner must stay white -- catches
    /// an inverted-color or off-by-one-quiet-zone bug that a "some pixels
    /// are non-white somewhere" assertion would miss.
    #[test]
    fn rasterize_qr_produces_a_real_dark_on_light_code() {
        let image = rasterize_qr("microtak-enroll:?url=http://server:8446&token=abc123").unwrap();
        let gray = image.to_luma8();

        // The finder pattern occupies the modules right after the quiet
        // zone in the top-left corner.
        let finder_x = QUIET_ZONE_MODULES * MODULE_PIXELS + MODULE_PIXELS / 2;
        let finder_y = QUIET_ZONE_MODULES * MODULE_PIXELS + MODULE_PIXELS / 2;
        assert_eq!(gray.get_pixel(finder_x, finder_y).0[0], 0, "finder pattern must be dark");

        // The extreme top-left pixel sits in the quiet zone, which must
        // stay white.
        assert_eq!(gray.get_pixel(0, 0).0[0], 255, "quiet zone must stay white");
    }

    #[test]
    fn expiry_display_covers_all_three_states() {
        assert_eq!(Expiry::Never.to_string(), "never");
        assert_eq!(Expiry::At("2026-10-01T00:00:00Z".to_string()).to_string(), "2026-10-01T00:00:00Z");
        assert!(Expiry::Unknown.to_string().contains("unknown"));
    }

    #[test]
    fn write_enrollment_pdf_produces_a_real_pdf_file() {
        let dir = std::env::temp_dir().join(format!("microtak-admin-cli-pdf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out_path = dir.join("handout.pdf");

        let info = HandoutInfo {
            token: "abc123",
            enrollment_url: "http://microtak.example.com:8446",
            note: Some("for jz_pixel"),
            expires_at: Expiry::At("2026-10-01T00:00:00Z".to_string()),
        };
        write_enrollment_pdf(&info, out_path.to_str().unwrap()).unwrap();

        let bytes = std::fs::read(&out_path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "output must be a real PDF file");
        assert!(bytes.len() > 1000, "a PDF with an embedded QR code image should not be tiny");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
