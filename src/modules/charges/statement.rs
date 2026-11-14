//! The monthly statement as a PDF (step 33). Pure: `parse_month`, `ascii` and `build_statement` need no
//! database. The PDF uses the built-in Helvetica font, which has no Romanian letters, so text is written in
//! ASCII (`ăâîșț` become `aaist`).

use chrono::{Datelike, NaiveDate};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

use crate::{error::AppError, modules::extras::trips::models::RecentTrip};

/// A4, in points.
const PAGE_WIDTH: f32 = 595.0;
const PAGE_HEIGHT: f32 = 842.0;
/// Trips on one page.
const ROWS_PER_PAGE: usize = 42;
/// The longest stop name written in a column (characters); longer ones end with `...`.
const MAX_STOP_CHARS: usize = 28;

/// The first day of the month and the first day of the next one, from `YYYY-MM` (four digits, a dash, two
/// digits, a month from 01 to 12). Anything else is a 400. Pure.
pub fn parse_month(text: &str) -> Result<(NaiveDate, NaiveDate), AppError> {
    let bad = || AppError::Validation("The month must look like 2026-10.".to_string());
    let bytes = text.as_bytes();
    let shape_ok = bytes.len() == 7
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_digit());
    if !shape_ok {
        return Err(bad());
    }
    let year: i32 = text[..4].parse().map_err(|_| bad())?;
    let month: u32 = text[5..].parse().map_err(|_| bad())?;
    let first = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(bad)?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(bad)?;
    Ok((first, next))
}

/// Text that the PDF font can write: Romanian letters lose their marks, any other character outside ASCII
/// becomes `?`, control characters become a space. Pure.
pub fn ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'ă' | 'â' => 'a',
            'Ă' | 'Â' => 'A',
            'î' => 'i',
            'Î' => 'I',
            'ș' | 'ş' => 's',
            'Ș' | 'Ş' => 'S',
            'ț' | 'ţ' => 't',
            'Ț' | 'Ţ' => 'T',
            c if c.is_ascii_control() => ' ',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

/// A stop name cut to `MAX_STOP_CHARS` characters (with `...` when it was longer), as ASCII. Pure.
fn stop_text(name: &str) -> String {
    let name = ascii(name);
    if name.chars().count() <= MAX_STOP_CHARS {
        return name;
    }
    let cut: String = name.chars().take(MAX_STOP_CHARS - 3).collect();
    format!("{cut}...")
}

/// An amount in bani as lei with two decimals, `5.00`. Pure.
pub fn lei(bani: i64) -> String {
    format!("{}.{:02}", bani / 100, bani % 100)
}

/// The sum of the fares of the trips, in bani. Pure.
pub fn total_bani(trips: &[RecentTrip]) -> i64 {
    trips.iter().map(|t| i64::from(t.fare_bani)).sum()
}

/// One line of text at a place on the page.
fn text(content: &mut Content, font_size: f32, x: f32, y: f32, line: &str) {
    content.begin_text();
    content.set_font(Name(b"F1"), font_size);
    content.next_line(x, y);
    content.show(Str(ascii(line).as_bytes()));
    content.end_text();
}

/// The PDF of the statement of `month` (`2026-10`): the trips (date, time, line, from, to, km, minutes, fare)
/// and the total. More than `ROWS_PER_PAGE` trips continue on more pages; the total is on the last one.
/// The trips must not be empty (the caller answers 404 for a month without trips). Pure.
pub fn build_statement(month: &str, trips: &[RecentTrip]) -> Vec<u8> {
    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let font_id = Ref::new(3);

    let pages: Vec<&[RecentTrip]> = if trips.is_empty() {
        vec![&[]]
    } else {
        trips.chunks(ROWS_PER_PAGE).collect()
    };
    // page n has the ids 4 + 2n (page) and 5 + 2n (its content)
    let page_id = |n: usize| Ref::new(4 + 2 * n as i32);
    let content_id = |n: usize| Ref::new(5 + 2 * n as i32);

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids((0..pages.len()).map(page_id))
        .count(pages.len() as i32);
    pdf.type1_font(font_id)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    for (n, rows) in pages.iter().enumerate() {
        let mut page = pdf.page(page_id(n));
        page.media_box(Rect::new(0.0, 0.0, PAGE_WIDTH, PAGE_HEIGHT))
            .parent(page_tree_id)
            .contents(content_id(n));
        page.resources().fonts().pair(Name(b"F1"), font_id);
        page.finish();

        let mut content = Content::new();
        let mut y = 790.0;
        text(
            &mut content,
            16.0,
            40.0,
            y,
            &format!("EchoPass - statement {month}"),
        );
        y -= 16.0;
        text(
            &mut content,
            9.0,
            40.0,
            y,
            &format!("Page {} of {}", n + 1, pages.len()),
        );
        y -= 28.0;
        for (x, title) in [
            (40.0, "Date"),
            (95.0, "Time"),
            (130.0, "Line"),
            (170.0, "From"),
            (310.0, "To"),
            (440.0, "km"),
            (480.0, "min"),
            (520.0, "Fare"),
        ] {
            text(&mut content, 9.0, x, y, title);
        }
        y -= 6.0;
        content.move_to(40.0, y).line_to(555.0, y).stroke();
        y -= 14.0;
        for trip in rows.iter() {
            let cells = [
                (40.0, trip.date.format("%Y-%m-%d").to_string()),
                (95.0, trip.time.format("%H:%M").to_string()),
                (130.0, trip.line.clone()),
                (170.0, stop_text(&trip.from_stop)),
                (310.0, stop_text(&trip.to_stop)),
                (440.0, format!("{:.1}", trip.distance_km)),
                (480.0, trip.duration_min.to_string()),
                (520.0, lei(i64::from(trip.fare_bani))),
            ];
            for (x, cell) in cells {
                text(&mut content, 8.0, x, y, &cell);
            }
            y -= 14.0;
        }
        if n + 1 == pages.len() {
            y -= 6.0;
            content.move_to(40.0, y).line_to(555.0, y).stroke();
            y -= 18.0;
            text(
                &mut content,
                11.0,
                40.0,
                y,
                &format!("Trips: {}", trips.len()),
            );
            text(
                &mut content,
                11.0,
                440.0,
                y,
                &format!("Total: {} RON", lei(total_bani(trips))),
            );
        }
        pdf.stream(content_id(n), &content.finish());
    }
    pdf.finish()
}

/// The month of a date as `YYYY-MM`. Pure.
pub fn month_label(first_day: NaiveDate) -> String {
    format!("{:04}-{:02}", first_day.year(), first_day.month())
}
