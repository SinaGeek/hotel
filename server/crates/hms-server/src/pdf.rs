//! F5: server-side PDF generation (todo F5 "PDF export server side") —
//! reservation vouchers and revenue reports, replacing the browser-print
//! placeholder. Uses printpdf's low-level `Op` API with one bundled font:
//! DejaVu Sans (`assets/fonts/DejaVuSans.ttf`, free license beside it) so
//! Turkish guest/hotel names render correctly — base-14 fonts (Helvetica)
//! have no ğ/ş/ı glyphs, which would corrupt exactly the names this file
//! prints. Automatic font subsetting keeps the output small.
//!
//! Coordinates are millimetres from the BOTTOM-LEFT of an A4 page
//! (210 × 297 mm). Money is printed as `1,234.56 CUR`; padding to fixed
//! character widths makes numeric columns line up visually without shaping.

use axum::response::IntoResponse;
use printpdf::{
    FontId, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Pt, TextItem,
};

const A4_W: f32 = 210.0;
const A4_H: f32 = 297.0;
const MARGIN_L: f32 = 20.0;
const MARGIN_R: f32 = 190.0;
const TOP_Y: f32 = 277.0; // first text baseline
const ROW_PITCH_MM: f32 = 5.4;
const ROWS_PER_PAGE: usize = 40;

/// Parses the bundled TTF once; every document re-registers it cheaply.
fn font() -> &'static ParsedFont {
    static FONT: std::sync::OnceLock<ParsedFont> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = include_bytes!("../assets/fonts/DejaVuSans.ttf");
        let mut warnings = Vec::new();
        ParsedFont::from_bytes(bytes, 0, &mut warnings)
            .expect("bundled crates/hms-server/assets/fonts/DejaVuSans.ttf is a valid TTF")
    })
}

/// `123456` cents → "1,234.56" (sign preserved for negatives).
pub(crate) fn fmt_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let whole = cents.unsigned_abs() / 100;
    let frac = cents.unsigned_abs() % 100;
    let digits = whole.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{sign}{grouped}.{frac:02}")
}

/// Word wrap; hard-splits single tokens wider than `max`.
pub(crate) fn wrap(s: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.chars().count() + word.chars().count() + usize::from(!cur.is_empty()) > max {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if word.chars().count() > max {
                let mut rem = word;
                while rem.chars().count() > max {
                    let cut_at = rem
                        .char_indices()
                        .nth(max)
                        .map(|(i, _)| i)
                        .unwrap_or(rem.len());
                    out.push(rem[..cut_at].to_string());
                    rem = &rem[cut_at..];
                }
                cur = rem.to_string();
            } else {
                cur = word.to_string();
            }
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Emits `lines` starting at `y_mm` (baseline), descending by `pitch` per
/// line; returns the y for the element below the block.
fn text_lines(
    font_id: &FontId,
    y0: f32,
    size_pt: f32,
    pitch_mm: f32,
    lines: &[String],
    cursor_x: f32,
) -> (Vec<Op>, f32) {
    let mut ops = Vec::new();
    if lines.is_empty() {
        return (ops, y0);
    }
    ops.push(Op::StartTextSection);
    ops.push(Op::SetTextCursor {
        pos: Point::new(Mm(cursor_x), Mm(y0)),
    });
    ops.push(Op::SetFont {
        font: PdfFontHandle::External(font_id.clone()),
        size: Pt(size_pt),
    });
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            ops.push(Op::AddLineBreak);
        }
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(line.clone())],
        });
    }
    ops.push(Op::EndTextSection);
    (ops, y0 - pitch_mm * lines.len() as f32)
}

fn divider(y_mm: f32) -> Vec<Op> {
    vec![
        Op::SetOutlineThickness { pt: Pt(0.4) },
        Op::DrawLine {
            line: Line {
                points: vec![
                    LinePoint {
                        p: Point::new(Mm(MARGIN_L), Mm(y_mm)),
                        bezier: false,
                    },
                    LinePoint {
                        p: Point::new(Mm(MARGIN_R), Mm(y_mm)),
                        bezier: false,
                    },
                ],
                is_closed: false,
            },
        },
    ]
}

/// One report/voucher page: title block, optional column header band, a list
/// of pre-formatted body lines, then a footer band.
fn build_page(
    font_id: &FontId,
    title: &str,
    subtitle: &str,
    header_line: Option<&str>,
    body_lines: &[String],
    footer: &str,
) -> PdfPage {
    let mut ops = Vec::new();
    let mut y = TOP_Y;

    let (title_ops, y1) = text_lines(font_id, y, 15.0, 8.0, &[title.to_string()], MARGIN_L);
    ops.extend(title_ops);
    y = y1;
    let (sub_ops, y2) = text_lines(font_id, y, 10.0, 6.0, &[subtitle.to_string()], MARGIN_L);
    ops.extend(sub_ops);
    y = y2;
    ops.extend(divider(y));
    y -= 7.0;

    if let Some(headers) = header_line {
        let (h_ops, y3) = text_lines(
            font_id,
            y,
            9.0,
            ROW_PITCH_MM,
            &[headers.to_string()],
            MARGIN_L,
        );
        ops.extend(h_ops);
        y = y3;
        ops.extend(divider(y + 1.0));
        y -= 0.5;
    }

    let (body_ops, y4) = text_lines(font_id, y, 9.0, ROW_PITCH_MM, body_lines, MARGIN_L);
    ops.extend(body_ops);
    y = y4;

    ops.extend(divider(y - 2.0));
    let (foot_ops, _) = text_lines(font_id, y - 7.0, 8.0, 5.0, &[footer.to_string()], MARGIN_L);
    ops.extend(foot_ops);

    PdfPage::new(Mm(A4_W), Mm(A4_H), ops)
}

fn save(mut doc: PdfDocument, pages: Vec<PdfPage>) -> Vec<u8> {
    let mut warnings = Vec::new();
    doc.with_pages(pages).save(
        &PdfSaveOptions {
            subset_fonts: true,
            ..Default::default()
        },
        &mut warnings,
    )
}

/// Attach-able PDF response (axum): `application/pdf` + ASCII filename.
/// The reservation id/voucher number is a UUID, so no header lies.
pub(crate) fn pdf_response(bytes: Vec<u8>, filename: &str) -> axum::response::Response {
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/pdf"),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                axum::http::HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                    .unwrap_or(axum::http::HeaderValue::from_static("attachment")),
            ),
        ],
        bytes,
    )
        .into_response()
}

/// By-day revenue report: one A4 page per chunk of day rows; page 1 carries
/// the totals in its footer.
pub(crate) fn by_day_report_pdf(
    hotel_name: &str,
    currency: &str,
    from: &str,
    to: &str,
    rows: &[[String; 5]], // date, room, extra, collected, occupied-nights
    totals: [i64; 4],     // room, extra, collected, occupied-nights
    generated_at: &str,
) -> Vec<u8> {
    let title_str = format!("HMS daily revenue — {hotel_name}");
    let mut doc = PdfDocument::new(&title_str);
    let font_id = doc.add_font(font());

    let title = "HMS — Daily Revenue Report";
    let subtitle = format!("{hotel_name}   {from} → {to}  ({currency})");

    let header_line = Some(format!(
        "{:<11}{:>14}{:>14}{:>14}{:>8}",
        "Date", "Room", "Extra", "Collected", "Nights"
    ));

    let mut pages: Vec<PdfPage> = Vec::new();
    for chunk in rows.chunks(ROWS_PER_PAGE) {
        let is_first = pages.is_empty();
        let footer: String = if is_first {
            format!(
                "Totals: room {}   extra {}   collected {}   nights {}   —   HMS, generated UTC {generated_at}",
                fmt_cents(totals[0]),
                fmt_cents(totals[1]),
                fmt_cents(totals[2]),
                totals[3],
            )
        } else {
            format!("HMS — generated UTC {generated_at}")
        };
        let lines: Vec<String> = chunk
            .iter()
            .map(|r| {
                format!(
                    "{:<11}{:>14}{:>14}{:>14}{:>8}",
                    r[0], r[1], r[2], r[3], r[4]
                )
            })
            .collect();
        pages.push(build_page(
            &font_id,
            title,
            &subtitle,
            header_line.as_deref(),
            &lines,
            &footer,
        ));
    }
    save(doc, pages)
}

/// Reservation voucher: a single A4 field sheet; `fields` are label/value
/// pairs where values are word-wrapped at 70 chars.
pub(crate) fn voucher_pdf(hotel_name: &str, fields: &[(&str, String)], footer: &str) -> Vec<u8> {
    let mut doc = PdfDocument::new("HMS reservation voucher");
    let font_id = doc.add_font(font());

    let mut body: Vec<String> = Vec::new();
    for (label, value) in fields {
        let wrapped = wrap(value, 70);
        body.push(format!(
            "{:<11} {}",
            label,
            wrapped.first().cloned().unwrap_or_default()
        ));
        for cont in wrapped.iter().skip(1) {
            body.push(format!("{:<11} {}", "", cont));
        }
    }

    save(
        doc,
        vec![build_page(
            &font_id,
            "RESERVATION VOUCHER",
            hotel_name,
            None,
            &body,
            footer,
        )],
    )
}

#[cfg(test)]
mod pdf_tests {
    use super::*;

    #[test]
    fn cents_formatting_groups_thousands() {
        assert_eq!(fmt_cents(0), "0.00");
        assert_eq!(fmt_cents(7), "0.07");
        assert_eq!(fmt_cents(975), "9.75");
        assert_eq!(fmt_cents(123456), "1,234.56");
        assert_eq!(fmt_cents(1000000), "10,000.00");
        assert_eq!(fmt_cents(-98765), "-987.65");
    }

    #[test]
    fn wrap_respects_char_counts_and_hard_splits() {
        assert_eq!(wrap("a b c", 80), vec!["a b c"]);
        let long = "a".repeat(30);
        assert_eq!(
            wrap(&long, 10),
            vec!["a".repeat(10), "a".repeat(10), "a".repeat(10)]
        );
        assert_eq!(wrap("one two", 6), vec!["one", "two"]);
        // greedy: "Göksu Çağlar" is exactly 12 chars and still fits
        assert_eq!(
            wrap("Göksu Çağlar pours tea", 12),
            vec!["Göksu Çağlar", "pours tea"],
            "greedy packing takes the longest line that fits"
        );
    }

    /// f5_truth: a 95-row report is a parseable PDF with the embedded
    /// DejaVuSans font (subsetted) and a proper %%EOF trailer.
    #[test]
    fn by_day_report_pdf_is_structurally_sound() {
        let rows: Vec<[String; 5]> = (0..95)
            .map(|i| {
                [
                    format!("2026-01-{:02}", (i % 28) + 1),
                    "1,234.56".into(),
                    "56.78".into(),
                    "900.00".into(),
                    "42".into(),
                ]
            })
            .collect();
        let bytes = by_day_report_pdf(
            "Scale Hotel 12",
            "TRY",
            "2026-01-01",
            "2026-04-05",
            &rows,
            [1_234_567_989, 12_345, 900_000, 3_990],
            "2026-11-05T20:00:00Z",
        );
        assert!(bytes.starts_with(b"%PDF"), "must be a PDF");
        assert!(
            bytes.windows(5).any(|w| w == b"%%EOF"),
            "must end with %%EOF marker family"
        );
        // persist the artifact so it can be opened in a viewer while iterating
        let sample = std::env::temp_dir().join("hms-f5-by-day.pdf");
        std::fs::write(&sample, &bytes).expect("write sample to temp dir");
        assert!(
            bytes.windows(10).any(|w| w == b"/FontFile2")
                && bytes.windows(10).any(|w| w == b"Identity-H"),
            "an embedded Unicode font program must be present (FontFile2, Identity-H)"
        );
        assert!(
            bytes.len() > 2_000,
            "font subset + layout must be non-trivial"
        );
        // 95 rows at 40 rows/page → exactly 3 leaf pages
        let leaves = bytes
            .windows(11)
            .filter(|w| &w[..10] == b"/Type/Page" && w[10] != b's')
            .count();
        assert_eq!(leaves, 3, "95 rows must paginate into 3 pages");
    }

    /// f5_truth: a voucher is a valid single-page PDF (each page has its own
    /// MediaBox; `/Type /Pages` is the tree node, `/Type /Page` the leaves).
    #[test]
    fn voucher_pdf_is_structurally_sound() {
        let fields: Vec<(&str, String)> = vec![
            ("Voucher No", "V-2026-0007".into()),
            ("Guest", "Abdurrahman Şerafettin Öğüç-dağ".into()),
            ("Room", "105 (Standard Double)".into()),
            ("Channel", "Booking.com".into()),
            ("Stay", "2026-11-05 → 2026-11-08 (3 nights)".into()),
            ("Total", "9,845.40 TRY".into()),
        ];
        let bytes = voucher_pdf(
            "Otel İstanbul Şah —nağ",
            &fields,
            "HMS — generated UTC 2026-11-05T20:00:00Z",
        );
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
        let sample = std::env::temp_dir().join("hms-f5-voucher.pdf");
        std::fs::write(&sample, &bytes).expect("write sample to temp dir");
        assert!(
            bytes.windows(10).any(|w| w == b"/FontFile2")
                && bytes.windows(10).any(|w| w == b"Identity-H"),
            "an embedded Unicode font program must be present (FontFile2, Identity-H)"
        );
        let leaves = bytes
            .windows(11)
            .filter(|w| &w[..10] == b"/Type/Page" && w[10] != b's')
            .count();
        assert_eq!(leaves, 1, "a voucher is exactly one page (got {leaves})");
    }
}
