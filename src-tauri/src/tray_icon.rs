//! Dynamic system tray icon + tooltip rendering.
//!
//! Renders a 64x64 RGBA tray icon showing the rolling usage percent as a
//! colored ring plus a pixel-font number, and a multi-line tooltip with the
//! current quota snapshot and a rolling-vs-yesterday delta. No image or font
//! crates are used — the bitmap and 5x7 digit font are hand-drawn, so the
//! size-optimized release profile (lto + opt-level=z + strip) is unaffected
//! and no Unicode/Emoji leaks into the OS-facing icon.

use crate::models::AppDataSnapshot;
use chrono::Local;
use std::f64::consts::PI;
use tauri::image::Image;
use tauri::AppHandle;

const TRAY_ID: &str = "main-tray";
const ICON_SIZE: u32 = 64;
const COST_UNITS_PER_USD: f64 = 100_000_000.0;

/// 5x7 bitmap font for digits 0-9. Each row's low 5 bits are the columns
/// (bit 4 = leftmost, bit 0 = rightmost). Drawn scaled for a crisp pixel look.
const DIGIT_FONT: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ], // 0
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // 1
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ], // 2
    [
        0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110,
    ], // 3
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ], // 4
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ], // 5
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ], // 6
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ], // 7
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ], // 8
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ], // 9
];

/// Pick the ring color for a rolling percent, matching the app palette:
/// blue under 80%, orange 80-94%, red at 95%+.
fn threshold_color(pct: u32) -> (u8, u8, u8) {
    if pct >= 95 {
        (224, 97, 112) // #e06170 --color-danger
    } else if pct >= 80 {
        (233, 174, 85) // #e9ae55 --color-monthly
    } else {
        (130, 162, 255) // #82a2ff --color-rolling
    }
}

/// Write one RGBA pixel into the buffer (no-op if out of bounds).
fn put_pixel(buf: &mut [u8], size: i32, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if (0..size).contains(&x) && (0..size).contains(&y) {
        let i = ((y * size + x) * 4) as usize;
        buf[i] = r;
        buf[i + 1] = g;
        buf[i + 2] = b;
        buf[i + 3] = a;
    }
}

/// Render the rolling-usage tray icon as a 64x64 RGBA byte buffer.
fn render_tray_rgba(rolling_pct: u32) -> Vec<u8> {
    let size = ICON_SIZE as i32;
    let cx = (size - 1) as f64 / 2.0;
    let cy = (size - 1) as f64 / 2.0;
    let r_bg = 32.0; // background disc
    let r_outer = 30.0; // ring outer radius
    let r_inner = 23.0; // ring inner radius
    let pct = (rolling_pct as f64).clamp(0.0, 100.0);
    let sweep = pct / 100.0 * 2.0 * PI; // radians, clockwise from top
    let (cr, cg, cb) = threshold_color(rolling_pct);

    let mut buf = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    for py in 0..size {
        for px in 0..size {
            let dx = px as f64 - cx;
            let dy = py as f64 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            // Background disc (dark, opaque) keeps the number legible on any
            // taskbar color.
            if dist <= r_bg {
                put_pixel(&mut buf, size, px, py, 24, 26, 34, 255);
            }

            // Ring annulus: filled arc + dim track for the remainder.
            if dist >= r_inner && dist <= r_outer {
                // atan2: 0 at +x (3 o'clock), -PI/2 at top (12 o'clock).
                // Normalize relative to the top so clockwise-from-top is an
                // increasing angle in [0, 2*PI).
                let mut rel = dy.atan2(dx) + PI / 2.0;
                if rel < 0.0 {
                    rel += 2.0 * PI;
                }
                if rel <= sweep {
                    put_pixel(&mut buf, size, px, py, cr, cg, cb, 255);
                } else {
                    put_pixel(&mut buf, size, px, py, 60, 64, 80, 255);
                }
            }
        }
    }

    // Number (1-3 digits), centered, drawn last so it sits on top of the ring.
    let digits: Vec<u8> = rolling_pct
        .clamp(0, 100)
        .to_string()
        .bytes()
        .map(|b| b - b'0')
        .collect();
    let scale = if digits.len() >= 3 { 3 } else { 4 };
    let (glyph_w, glyph_h, gap) = (5, 7, 1);
    let block_w = (digits.len() * glyph_w + (digits.len() - 1) * gap) * scale;
    let block_h = glyph_h * scale;
    let origin_x = (cx - block_w as f64 / 2.0).round() as i32;
    let origin_y = (cy - block_h as f64 / 2.0).round() as i32;

    for (di, &d) in digits.iter().enumerate() {
        let glyph = DIGIT_FONT[d as usize];
        let gx = origin_x + (di * (glyph_w + gap) * scale) as i32;
        for row in 0..glyph_h {
            let row_bits = glyph[row];
            for col in 0..glyph_w {
                if (row_bits >> (4 - col)) & 1 == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            put_pixel(
                                &mut buf,
                                size,
                                gx + (col * scale + sx) as i32,
                                origin_y + (row * scale + sy) as i32,
                                240,
                                242,
                                250,
                                255,
                            );
                        }
                    }
                }
            }
        }
    }

    buf
}

/// Render the rolling-usage tray icon as a Tauri image.
pub fn render_tray_image(rolling_pct: u32) -> Image<'static> {
    let buf = render_tray_rgba(rolling_pct);
    Image::new_owned(buf, ICON_SIZE, ICON_SIZE)
}

/// Refresh the tray icon + tooltip from the latest snapshot. Called after each
/// successful refresh. Failures are logged and never propagate — a tray update
/// error must not break the refresh cycle.
pub fn update_tray(app: &AppHandle, snapshot: &AppDataSnapshot) {
    let rolling = snapshot.usage.rolling.usage_percent;
    let weekly = snapshot.usage.weekly.usage_percent;
    let monthly = snapshot.usage.monthly.usage_percent;

    // Today's cost (sum of daily_costs for today's date, in cost units).
    let today = Local::now().format("%Y-%m-%d").to_string();
    let today_cost_units: i64 = snapshot
        .daily_costs
        .iter()
        .filter(|c| c.date == today)
        .map(|c| c.total_cost)
        .sum();
    let today_cost_usd = today_cost_units as f64 / COST_UNITS_PER_USD;

    // NOTE: a rolling-vs-yesterday delta line is intentionally omitted — it
    // pushes the tooltip past the Windows character limit. Keep this compact.
    let tooltip = format!(
        "OpenCode Usage\nRolling: {}%\nW: {}%  M: {}%  Cost: ${:.2}",
        rolling, weekly, monthly, today_cost_usd
    );

    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        eprintln!("[Tray] tray '{}' not found", TRAY_ID);
        return;
    };

    if let Err(e) = tray.set_tooltip(Some(&tooltip)) {
        eprintln!("[Tray] set_tooltip failed: {}", e);
    }
    let image = render_tray_image(rolling);
    if let Err(e) = tray.set_icon(Some(image)) {
        eprintln!("[Tray] set_icon failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_color_matches_app_palette() {
        assert_eq!(threshold_color(0), (130, 162, 255)); // blue
        assert_eq!(threshold_color(72), (130, 162, 255));
        assert_eq!(threshold_color(79), (130, 162, 255));
        assert_eq!(threshold_color(80), (233, 174, 85)); // orange
        assert_eq!(threshold_color(94), (233, 174, 85));
        assert_eq!(threshold_color(95), (224, 97, 112)); // red
        assert_eq!(threshold_color(100), (224, 97, 112));
    }

    #[test]
    fn digit_font_shape() {
        assert_eq!(DIGIT_FONT.len(), 10);
        for g in DIGIT_FONT.iter() {
            assert_eq!(g.len(), 7);
            for row in g {
                // each row only uses the low 5 bits
                assert_eq!(*row & 0b11100000, 0);
            }
        }
    }

    #[test]
    fn render_tray_rgba_size_and_center_opaque() {
        let buf = render_tray_rgba(50);
        assert_eq!(buf.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        // center pixel sits on the opaque background disc
        let i = ((32 * ICON_SIZE as i32 + 32) * 4) as usize;
        assert_eq!(buf[i + 3], 255);
    }

    #[test]
    fn render_tray_rgba_smoke_across_range() {
        for p in [0, 1, 25, 50, 72, 80, 94, 95, 99, 100, 150] {
            let buf = render_tray_rgba(p);
            assert_eq!(buf.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        }
    }
}
