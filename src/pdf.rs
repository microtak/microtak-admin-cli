//! Render a printable enrollment handout -- QR code plus manual
//! (type-it-by-hand) enrollment instructions -- as a single PDF file.
//!
//! Fonts are embedded at compile time (Liberation Sans, SIL Open Font
//! License -- see `assets/fonts/LICENSE-OFL.txt`) so the resulting binary
//! needs no font files on the target machine to generate a PDF.

use anyhow::{Context, Result};
use printpdf::{
    Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
    RawImage, RawImageData, RawImageFormat, TextItem, XObjectTransform,
};
use qrcode::{Color, QrCode};

const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");
const FONT_ITALIC: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Italic.ttf");

/// Pixels per QR module and the standard-recommended quiet-zone width (in
/// modules) around it -- without a quiet zone, some scanners fail to lock
/// onto the code at all.
const MODULE_PIXELS: u32 = 8;
const QUIET_ZONE_MODULES: u32 = 4;

/// A4 with 15 mm margins.
const PAGE_WIDTH: Mm = Mm(210.0);
const PAGE_HEIGHT: Mm = Mm(297.0);
const MARGIN: Mm = Mm(15.0);
const BODY_SIZE: f32 = 12.0;
/// Printed QR code width: the `tak://` link with a 256-bit token is a dense
/// code; much smaller and a phone camera can't reliably read it off paper.
const QR_WIDTH: Mm = Mm(60.0);
/// Hanging indent of numbered list items.
const LIST_INDENT: Mm = Mm(6.0);

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
    pub link: &'a crate::qr::EnrollmentLink,
    pub note: Option<&'a str>,
    pub expires_at: Expiry,
}

/// Rasterize a QR code as black-on-white 8-bit grayscale pixels
/// (row-major, `size × size`), returning `(pixels, size)`.
fn rasterize_qr(payload: &str) -> Result<(Vec<u8>, u32)> {
    let code = QrCode::new(payload.as_bytes()).context("encoding QR code")?;
    let modules = code.width() as u32;
    let colors = code.to_colors();
    let size = (modules + QUIET_ZONE_MODULES * 2) * MODULE_PIXELS;

    let mut pixels = vec![255u8; (size * size) as usize];
    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] != Color::Dark {
                continue;
            }
            let px0 = (x + QUIET_ZONE_MODULES) * MODULE_PIXELS;
            let py0 = (y + QUIET_ZONE_MODULES) * MODULE_PIXELS;
            for dy in 0..MODULE_PIXELS {
                let row = ((py0 + dy) * size + px0) as usize;
                pixels[row..row + MODULE_PIXELS as usize].fill(0);
            }
        }
    }
    Ok((pixels, size))
}

/// One embedded font: its PDF handle plus its metrics for measuring text.
struct Font {
    handle: PdfFontHandle,
    metrics: ParsedFont,
}

impl Font {
    fn load(doc: &mut PdfDocument, bytes: &'static [u8], name: &str) -> Result<Self> {
        let metrics = ParsedFont::from_bytes(bytes, 0, &mut Vec::new())
            .with_context(|| format!("loading the {name} font"))?;
        Ok(Self {
            handle: PdfFontHandle::External(doc.add_font(&metrics)),
            metrics,
        })
    }

    /// Width of `text` at `size` points.
    fn width(&self, text: &str, size: f32) -> f32 {
        let units: f32 = text
            .chars()
            .map(|c| {
                self.metrics
                    .lookup_glyph_index(c as u32)
                    .and_then(|glyph| self.metrics.get_glyph_width(glyph))
                    .unwrap_or(0) as f32
            })
            .sum();
        units * size / self.metrics.units_per_em as f32
    }

    /// Greedy word wrap of `text` into lines no wider than `max_width` pt.
    fn wrap(&self, text: &str, size: f32, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && self.width(&candidate, size) > max_width {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
}

/// Top-down page layout: a cursor from the top margin, emitting PDF ops.
struct Layout {
    ops: Vec<Op>,
    /// Distance of the next line's top from the page bottom, in pt.
    y: f32,
}

impl Layout {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            y: Pt::from(PAGE_HEIGHT).0 - Pt::from(MARGIN).0,
        }
    }

    fn left() -> f32 {
        Pt::from(MARGIN).0
    }

    fn text_width() -> f32 {
        Pt::from(PAGE_WIDTH).0 - 2.0 * Pt::from(MARGIN).0
    }

    /// One line of text at `x`, advancing the cursor by its line height.
    fn line(&mut self, font: &Font, size: f32, x: f32, text: &str) {
        let line_height = size * 1.25;
        self.y -= line_height;
        self.ops.extend([
            Op::StartTextSection,
            Op::SetFont { font: font.handle.clone(), size: Pt(size) },
            Op::SetTextCursor { pos: Point { x: Pt(x), y: Pt(self.y + (line_height - size)) } },
            Op::ShowText { items: vec![TextItem::Text(text.to_string())] },
            Op::EndTextSection,
        ]);
    }

    /// A wrapped paragraph across the full text width.
    fn paragraph(&mut self, font: &Font, size: f32, text: &str) {
        for line in font.wrap(text, size, Self::text_width()) {
            self.line(font, size, Self::left(), &line);
        }
    }

    /// A numbered list with a hanging indent.
    fn ordered_list(&mut self, font: &Font, size: f32, items: &[String]) {
        let indent = Pt::from(LIST_INDENT).0;
        for (index, item) in items.iter().enumerate() {
            let lines = font.wrap(item, size, Self::text_width() - indent);
            for (n, line) in lines.iter().enumerate() {
                if n == 0 {
                    let number = format!("{}.", index + 1);
                    let number_x = Self::left() + indent - font.width(&number, size) - 2.0;
                    // Same baseline as the item's first line.
                    let y = self.y;
                    self.line(font, size, Self::left() + indent, line);
                    let first_line_y = self.y;
                    self.y = y;
                    self.line(font, size, number_x, &number);
                    self.y = first_line_y;
                } else {
                    self.line(font, size, Self::left() + indent, line);
                }
            }
        }
    }

    fn space(&mut self, points: f32) {
        self.y -= points;
    }
}

pub fn write_enrollment_pdf(info: &HandoutInfo, out_path: &str) -> Result<()> {
    let mut doc = PdfDocument::new("MicroTAK Enrollment");
    let regular = Font::load(&mut doc, FONT_REGULAR, "regular")?;
    let bold = Font::load(&mut doc, FONT_BOLD, "bold")?;
    let italic = Font::load(&mut doc, FONT_ITALIC, "italic")?;
    let mut page = Layout::new();

    page.paragraph(&bold, 20.0, "MicroTAK Device Enrollment");
    page.space(BODY_SIZE);

    let link = info.link;
    page.paragraph(&regular, BODY_SIZE, &format!("Device name: {}", link.username));
    page.paragraph(
        &regular,
        BODY_SIZE,
        &format!(
            "Server: {} (enrollment port {}, streaming port {})",
            link.host, link.enrollment_port, link.streaming_port
        ),
    );
    page.paragraph(&regular, BODY_SIZE, &format!("Token: {}", link.token));
    page.paragraph(&regular, BODY_SIZE, &format!("Expires: {}", info.expires_at));
    if let Some(note) = info.note {
        page.paragraph(&regular, BODY_SIZE, &format!("Note: {note}"));
    }
    page.space(BODY_SIZE);

    // The QR code, centered.
    let (pixels, size) = rasterize_qr(&link.to_uri())?;
    let image = RawImage {
        pixels: RawImageData::U8(pixels),
        width: size as usize,
        height: size as usize,
        data_format: RawImageFormat::R8,
        tag: Vec::new(),
    };
    let image_id = doc.add_image(&image);
    let qr_width = Pt::from(QR_WIDTH).0;
    page.y -= qr_width;
    page.ops.push(Op::UseXobject {
        id: image_id,
        transform: XObjectTransform {
            translate_x: Some(Pt((Pt::from(PAGE_WIDTH).0 - qr_width) / 2.0)),
            translate_y: Some(Pt(page.y)),
            // Pixels per inch that make `size` pixels exactly `QR_WIDTH` wide.
            dpi: Some(size as f32 / (QR_WIDTH.0 / 25.4)),
            ..Default::default()
        },
    });
    page.space(BODY_SIZE * 0.5);

    page.paragraph(
        &italic,
        BODY_SIZE,
        "Scan with the TAK client (ATAK, OmniTAK, ...) -- it enrolls and connects on its own. \
         The token works once, for this device name only.",
    );
    page.space(BODY_SIZE);

    page.paragraph(&bold, 14.0, "Manual enrollment (if you can't scan the QR code)");
    page.space(BODY_SIZE * 0.4);
    page.ordered_list(
        &regular,
        BODY_SIZE,
        &[
            format!(
                "In the TAK client, add a server with host {} -- the bare address, without \
                 https:// or a port.",
                link.host
            ),
            format!(
                "Enrollment port {}, streaming port {} with TLS on; if the client asks \
                 whether to trust the server's certificate, accept it.",
                link.enrollment_port, link.streaming_port
            ),
            format!("Username: {} -- Password: the token above.", link.username),
        ],
    );

    let bytes = doc
        .with_pages(vec![PdfPage::new(PAGE_WIDTH, PAGE_HEIGHT, page.ops)])
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    std::fs::write(out_path, bytes).context("writing PDF file")?;
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
        let (pixels, size) =
            rasterize_qr("tak://com.atakmap.app/enroll?host=server&username=u&token=abc123").unwrap();
        let pixel = |x: u32, y: u32| pixels[(y * size + x) as usize];

        // The finder pattern occupies the modules right after the quiet
        // zone in the top-left corner.
        let finder_x = QUIET_ZONE_MODULES * MODULE_PIXELS + MODULE_PIXELS / 2;
        let finder_y = QUIET_ZONE_MODULES * MODULE_PIXELS + MODULE_PIXELS / 2;
        assert_eq!(pixel(finder_x, finder_y), 0, "finder pattern must be dark");

        // The extreme top-left pixel sits in the quiet zone, which must
        // stay white.
        assert_eq!(pixel(0, 0), 255, "quiet zone must stay white");
    }

    #[test]
    fn expiry_display_covers_all_three_states() {
        assert_eq!(Expiry::Never.to_string(), "never");
        assert_eq!(Expiry::At("2026-10-01T00:00:00Z".to_string()).to_string(), "2026-10-01T00:00:00Z");
        assert!(Expiry::Unknown.to_string().contains("unknown"));
    }

    /// Width (= height) in pixels of [`rasterize_qr`]'s output for `payload`.
    fn qr_pixel_size(payload: &str) -> u32 {
        rasterize_qr(payload).unwrap().1
    }

    fn sample_link() -> crate::qr::EnrollmentLink {
        crate::qr::EnrollmentLink::from_enrollment_url(
            "https://microtak.example.com:8446",
            "pixel",
            "abc123",
            8089,
            8443,
        )
        .unwrap()
    }

    /// The handout as a parsed PDF: one page, the QR code embedded as an
    /// image at its rasterized pixel size, and the instructions as real
    /// (extractable) text.
    #[test]
    fn handout_is_one_page_with_the_qr_image_and_the_instructions() {
        let dir = std::env::temp_dir().join(format!("microtak-admin-cli-pdf-parse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out_path = dir.join("handout.pdf");
        let link = sample_link();
        let info = HandoutInfo {
            link: &link,
            note: Some("for the pixel"),
            expires_at: Expiry::Never,
        };
        write_enrollment_pdf(&info, out_path.to_str().unwrap()).unwrap();

        let doc = lopdf::Document::load(&out_path).unwrap();
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 1);

        let qr_pixels = qr_pixel_size(&link.to_uri());
        let widths: Vec<i64> = doc
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter(|stream| {
                stream.dict.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Image".as_slice())
            })
            .filter_map(|stream| stream.dict.get(b"Width").and_then(|w| w.as_i64()).ok())
            .collect();
        assert!(widths.contains(&(qr_pixels as i64)), "QR image of {qr_pixels}px expected, images: {widths:?}");

        // Word spacing differs between PDF writers; compare words only.
        let text = doc.extract_text(&[1]).unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
        for needle in ["MicroTAK Device Enrollment", "pixel", "abc123", "microtak.example.com", "Manual enrollment"] {
            assert!(text.contains(needle), "missing {needle:?} in {text:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_enrollment_pdf_produces_a_real_pdf_file() {
        let dir = std::env::temp_dir().join(format!("microtak-admin-cli-pdf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out_path = dir.join("handout.pdf");

        let link = crate::qr::EnrollmentLink::from_enrollment_url(
            "https://microtak.example.com:8446",
            "pixel",
            "abc123",
            8089,
            8443,
        )
        .unwrap();
        let info = HandoutInfo {
            link: &link,
            note: Some("for the pixel"),
            expires_at: Expiry::At("2026-10-01T00:00:00Z".to_string()),
        };
        write_enrollment_pdf(&info, out_path.to_str().unwrap()).unwrap();

        let bytes = std::fs::read(&out_path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "output must be a real PDF file");
        assert!(bytes.len() > 1000, "a PDF with an embedded QR code image should not be tiny");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
