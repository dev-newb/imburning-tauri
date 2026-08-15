// Menu-bar badges — one status item per tracked provider, each drawing the
// live percentage into a 20x20 icon.
//
// The badge is a hand-rolled bitmap font rather than a text draw: the icon is
// 20px square, and at that size a real font renderer produces mush. The glyph
// tables and the pixel layout are lifted from the Electron build unchanged so
// the two menu bars are indistinguishable.
//
// One item per provider (rather than one shared icon) because macOS gives each
// status item its own slot, and the user tracks several accounts at once.

use crate::store::Store;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};

const W: usize = 20;
const H: usize = 20;
const CHAR_H: usize = 11;

/// Digit glyphs lifted verbatim from the Electron build so the badge is
/// pixel-identical; each row is a bitmask, MSB = leftmost column.
const FONT_WIDE: [[u8; 11]; 10] = [
    [0b00111100, 0b01111110, 0b11100111, 0b11000011, 0b11000011, 0b11000011, 0b11000011, 0b11000011, 0b11100111, 0b01111110, 0b00111100], // 0
    [0b00011000, 0b00111000, 0b01111000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b01111110, 0b01111110], // 1
    [0b00111100, 0b01111110, 0b11100111, 0b00000011, 0b00000110, 0b00011100, 0b00111000, 0b01110000, 0b11100000, 0b11111111, 0b11111111], // 2
    [0b00111100, 0b01111110, 0b11100111, 0b00000011, 0b00000110, 0b00111100, 0b00000110, 0b00000011, 0b11100111, 0b01111110, 0b00111100], // 3
    [0b00000110, 0b00001110, 0b00011110, 0b00110110, 0b01100110, 0b11111111, 0b11111111, 0b00000110, 0b00000110, 0b00000110, 0b00000110], // 4
    [0b11111111, 0b11111111, 0b11000000, 0b11000000, 0b11111100, 0b00000110, 0b00000011, 0b00000011, 0b11100111, 0b01111110, 0b00111100], // 5
    [0b00111100, 0b01111110, 0b11100000, 0b11000000, 0b11111100, 0b11100110, 0b11000011, 0b11000011, 0b11100111, 0b01111110, 0b00111100], // 6
    [0b11111111, 0b11111111, 0b00000011, 0b00000110, 0b00001100, 0b00011000, 0b00110000, 0b00110000, 0b01100000, 0b01100000, 0b01100000], // 7
    [0b00111100, 0b01111110, 0b11100111, 0b11000011, 0b01111110, 0b00111100, 0b01111110, 0b11000011, 0b11100111, 0b01111110, 0b00111100], // 8
    [0b00111100, 0b01111110, 0b11100111, 0b11000011, 0b11000011, 0b01111111, 0b00111111, 0b00000011, 0b00000111, 0b01111110, 0b00111100], // 9
];

/// Digit glyphs lifted verbatim from the Electron build so the badge is
/// pixel-identical; each row is a bitmask, MSB = leftmost column.
const FONT_NARROW: [[u8; 11]; 10] = [
    [0b00011110, 0b00111111, 0b00110011, 0b00110011, 0b00110011, 0b00110011, 0b00110011, 0b00110011, 0b00110011, 0b00111111, 0b00011110], // 0
    [0b00001100, 0b00011100, 0b00111100, 0b00001100, 0b00001100, 0b00001100, 0b00001100, 0b00001100, 0b00001100, 0b00111111, 0b00111111], // 1
    [0b00011110, 0b00111111, 0b00110011, 0b00000011, 0b00000110, 0b00001100, 0b00011000, 0b00110000, 0b00110000, 0b00111111, 0b00111111], // 2
    [0b00011110, 0b00111111, 0b00110011, 0b00000011, 0b00001110, 0b00001110, 0b00000011, 0b00000011, 0b00110011, 0b00111111, 0b00011110], // 3
    [0b00000110, 0b00001110, 0b00011110, 0b00110110, 0b00110110, 0b00111111, 0b00111111, 0b00000110, 0b00000110, 0b00000110, 0b00000110], // 4
    [0b00111111, 0b00111111, 0b00110000, 0b00110000, 0b00111110, 0b00111111, 0b00000011, 0b00000011, 0b00110011, 0b00111111, 0b00011110], // 5
    [0b00011110, 0b00111111, 0b00110011, 0b00110000, 0b00111110, 0b00111111, 0b00110011, 0b00110011, 0b00110011, 0b00111111, 0b00011110], // 6
    [0b00111111, 0b00111111, 0b00000011, 0b00000110, 0b00000110, 0b00001100, 0b00001100, 0b00011000, 0b00011000, 0b00011000, 0b00011000], // 7
    [0b00011110, 0b00111111, 0b00110011, 0b00110011, 0b00011110, 0b00011110, 0b00110011, 0b00110011, 0b00110011, 0b00111111, 0b00011110], // 8
    [0b00011110, 0b00111111, 0b00110011, 0b00110011, 0b00110011, 0b00111111, 0b00011111, 0b00000011, 0b00110011, 0b00111111, 0b00011110], // 9
];

#[derive(Clone, Copy)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

/// Tauri wants RGBA; the Electron original wrote BGRA because that is what
/// nativeImage.createFromBuffer expects. Same pixels, different order — a
/// straight copy of that code would render every badge with red and blue
/// swapped.
struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    fn filled(bg: Rgba) -> Self {
        let mut px = vec![0u8; W * H * 4];
        for chunk in px.chunks_exact_mut(4) {
            chunk[0] = bg.0;
            chunk[1] = bg.1;
            chunk[2] = bg.2;
            chunk[3] = 255;
        }
        Canvas { px }
    }

    fn set(&mut self, x: i64, y: i64, c: Rgba) {
        if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
            return;
        }
        let o = (y as usize * W + x as usize) * 4;
        self.px[o] = c.0;
        self.px[o + 1] = c.1;
        self.px[o + 2] = c.2;
        self.px[o + 3] = c.3;
    }

    fn draw_digit(&mut self, digit: u8, x: i64, y: i64, c: Rgba, narrow: bool) {
        let glyph = if narrow { &FONT_NARROW[digit as usize] } else { &FONT_WIDE[digit as usize] };
        let char_w = if narrow { 6 } else { 8 };
        let max_col = if narrow { 5 } else { 7 };
        for (row, bits) in glyph.iter().enumerate().take(CHAR_H) {
            for col in 0..char_w {
                if bits & (1 << (max_col - col)) != 0 {
                    self.set(x + col as i64, y + row as i64, c);
                }
            }
        }
    }

    /// A 2px border, drawn once a pool crosses the danger threshold.
    fn outline(&mut self, c: Rgba) {
        const T: usize = 2;
        for y in 0..H {
            for x in 0..W {
                if x < T || y < T || x >= W - T || y >= H - T {
                    self.set(x as i64, y as i64, Rgba(c.0, c.1, c.2, 255));
                }
            }
        }
    }

    fn into_image(self) -> Image<'static> {
        Image::new_owned(self.px, W as u32, H as u32)
    }
}

fn digits_of(percent: f64) -> Vec<u8> {
    let n = percent.round().max(0.0) as i64;
    n.to_string().bytes().map(|b| b - b'0').collect()
}

/// The ordinary badge: the number, centred.
fn percentage_icon(percent: f64, bg: Rgba, text: Rgba, outline: Option<Rgba>) -> Image<'static> {
    let mut c = Canvas::filled(bg);
    let digits = digits_of(percent);
    let narrow = digits.len() >= 3;
    let char_w: i64 = if narrow { 6 } else { 8 };
    let gap: i64 = if narrow { 0 } else { 1 };
    let total = digits.len() as i64 * char_w + (digits.len() as i64 - 1) * gap;
    let mut x = (W as i64 - total) / 2;
    let y = (H as i64 - CHAR_H as i64) / 2;
    for d in digits {
        c.draw_digit(d, x, y, text, narrow);
        x += char_w + gap;
    }
    if let Some(o) = outline {
        c.outline(o);
    }
    c.into_image()
}

/// At 100% the number stops fitting and stops mattering — show an X.
fn red_x_icon(bg: Rgba, fg: Rgba, outline: Option<Rgba>) -> Image<'static> {
    let mut c = Canvas::filled(bg);
    for i in 0..11i64 {
        for dy in 0..2i64 {
            for dx in 0..2i64 {
                c.set(5 + i + dx, 5 + i + dy, fg);
                c.set(14 - i + dx, 5 + i + dy, fg);
            }
        }
    }
    if let Some(o) = outline {
        c.outline(o);
    }
    c.into_image()
}

/// Second-account (CLI login) badge: number sits high with a fat cursor dash
/// along the bottom right, reading like "61_".
fn cli_icon(percent: f64, bg: Rgba, text: Rgba, outline: Option<Rgba>) -> Image<'static> {
    let mut c = Canvas::filled(bg);
    if percent >= 99.0 {
        for i in 0..10i64 {
            for d in 0..2i64 {
                c.set(4 + i + d, 3 + i, text);
                c.set(13 - i + d, 3 + i, text);
            }
        }
    } else {
        let digits = digits_of(percent);
        let narrow = digits.len() >= 3;
        let char_w: i64 = if narrow { 6 } else { 8 };
        let gap: i64 = if narrow { 0 } else { 1 };
        let total = digits.len() as i64 * char_w + (digits.len() as i64 - 1) * gap;
        let mut x = ((W as i64 - total) / 2).max(0);
        for d in digits {
            c.draw_digit(d, x, 2, text, narrow);
            x += char_w + gap;
        }
    }
    // the cursor
    for x in 11..18i64 {
        for y in 16..19i64 {
            c.set(x, y, text);
        }
    }
    if let Some(o) = outline {
        c.outline(o);
    }
    c.into_image()
}

// ---- tray items ------------------------------------------------------------

/// Live status items, keyed by the same provider names the Electron build uses
/// ("session", "weekly", "codex", "gemini", …) so the settings that toggle them
/// need no translation.
static TRAYS: Mutex<Option<HashMap<String, TrayIcon>>> = Mutex::new(None);

struct Badge {
    percent: f64,
    label: String,
    resets_at: Option<String>,
    forecast_at: Option<String>,
    bg: Rgba,
    text: Rgba,
    cli: bool,
}

fn color_of(settings: &Value, key: &str, fallback: (u8, u8, u8), field: &str) -> Rgba {
    let hex = settings
        .get("trayColors")
        .and_then(|c| c.get(key))
        .and_then(|c| c.get(field))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    parse_hex(hex).unwrap_or(Rgba(fallback.0, fallback.1, fallback.2, 255))
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    Some(Rgba(
        u8::from_str_radix(&h[0..2], 16).ok()?,
        u8::from_str_radix(&h[2..4], 16).ok()?,
        u8::from_str_radix(&h[4..6], 16).ok()?,
        255,
    ))
}

/// Worst pool in a provider's set — the badge shows the limit closest to
/// biting, which is the only one worth a glance at menu-bar size.
fn worst(limits: Option<&Value>) -> Option<(f64, String, Option<String>)> {
    let arr = limits?.as_array()?;
    let mut best: Option<(f64, String, Option<String>)> = None;
    for l in arr {
        let p = l.get("percent")?.as_f64()?;
        if best.as_ref().map(|b| p > b.0).unwrap_or(true) {
            best = Some((
                p,
                l.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                l.get("resetsAt").and_then(|v| v.as_str()).map(String::from),
            ));
        }
    }
    best
}

fn fmt_reset(iso: Option<&String>, time_format: &str) -> Option<String> {
    let parsed = chrono::DateTime::parse_from_rfc3339(iso?).ok()?;
    let local = parsed.with_timezone(&chrono::Local);
    Some(if time_format == "24h" {
        local.format("%a %H:%M").to_string()
    } else {
        local.format("%a %-I:%M %p").to_string()
    })
}

fn sync_one(app: &AppHandle, name: &str, enabled: bool, badge: Option<Badge>, settings: &Value) {
    let Ok(mut guard) = TRAYS.lock() else { return };
    let trays = guard.get_or_insert_with(HashMap::new);

    let Some(badge) = badge.filter(|_| enabled) else {
        // Dropping the TrayIcon removes the status item.
        trays.remove(name);
        return;
    };

    let danger = settings
        .get("dangerThreshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(90.0);
    let outline_on = settings
        .get("trayOutline")
        .and_then(|o| o.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let outline = if outline_on && badge.percent >= danger {
        settings
            .get("trayOutline")
            .and_then(|o| o.get("color"))
            .and_then(|v| v.as_str())
            .and_then(parse_hex)
    } else {
        None
    };

    let image = if badge.cli {
        cli_icon(badge.percent, badge.bg, badge.text, outline)
    } else if badge.percent >= 99.0 {
        red_x_icon(badge.bg, badge.text, outline)
    } else {
        percentage_icon(badge.percent, badge.bg, badge.text, outline)
    };

    let time_format = settings.get("timeFormat").and_then(|v| v.as_str()).unwrap_or("12h");
    let mut tooltip = format!("{}: {}%", badge.label, badge.percent.round() as i64);
    if let Some(r) = fmt_reset(badge.resets_at.as_ref(), time_format) {
        tooltip.push_str(&format!("\nResets: {}", r));
    }
    if badge.percent < 99.0 {
        if let Some(f) = fmt_reset(badge.forecast_at.as_ref(), time_format) {
            tooltip.push_str(&format!("\nAt current pace, 100% by {}", f));
        }
    }

    if let Some(tray) = trays.get(name) {
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_tooltip(Some(&tooltip));
        return;
    }
    // Clicking any badge toggles the window, same as the Electron build.
    let handle = app.clone();
    if let Ok(tray) = TrayIconBuilder::new()
        .icon(image)
        .tooltip(&tooltip)
        .on_tray_icon_event(move |_, event| {
            if let tauri::tray::TrayIconEvent::Click { .. } = event {
                if let Some(window) = handle.get_webview_window("main") {
                    let visible = window.is_visible().unwrap_or(false);
                    let _ = if visible { window.hide() } else { window.show() };
                    if !visible {
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)
    {
        trays.insert(name.to_string(), tray);
    }
}

/// Refresh every badge from a usage payload.
pub fn sync(app: &AppHandle, data: &Value, store: &Store) {
    let settings = crate::settings::with_defaults(store);
    let show_anthropic = settings.get("showTrayStats").and_then(|v| v.as_bool()).unwrap_or(false);
    let show_openai = settings.get("trayOpenai").and_then(|v| v.as_bool()).unwrap_or(false);
    let show_google = settings.get("trayGoogle").and_then(|v| v.as_bool()).unwrap_or(false);
    let forecasts = data.get("forecasts").cloned().unwrap_or(Value::Null);
    let fc = |k: &str| forecasts.get(k).and_then(|v| v.as_str()).map(String::from);

    let window = |field: &str, label: &str, key: &str| -> Option<Badge> {
        let w = data.get(field)?;
        let percent = w.get("utilization")?.as_f64()?;
        Some(Badge {
            percent,
            label: label.to_string(),
            resets_at: w.get("resets_at").and_then(|v| v.as_str()).map(String::from),
            forecast_at: fc(key),
            bg: color_of(&settings, key, (59, 130, 246), "bg"),
            text: color_of(&settings, key, (255, 255, 255), "text"),
            cli: false,
        })
    };

    sync_one(app, "session", show_anthropic, window("five_hour", "Session", "session"), &settings);
    sync_one(app, "weekly", show_anthropic, window("seven_day", "Weekly", "weekly"), &settings);

    for (name, field, prefix, enabled, fallback) in [
        ("codex", "codex", "OpenAI", show_openai, (16u8, 163u8, 127u8)),
        ("gemini", "gemini", "Google", show_google, (244, 180, 0)),
    ] {
        let badge = worst(data.get(field).and_then(|d| d.get("limits"))).map(|(p, label, resets)| Badge {
            percent: p,
            label: format!("{} — {}", prefix, label),
            resets_at: resets,
            forecast_at: fc(name),
            bg: color_of(&settings, name, fallback, "bg"),
            text: color_of(&settings, name, (255, 255, 255), "text"),
            cli: false,
        });
        sync_one(app, name, enabled, badge, &settings);

        // The CLI login is a second account and gets its own badge.
        let cli_key = format!("{}Cli", name);
        let cli_badge = worst(data.get(field).and_then(|d| d.get("cli")).and_then(|d| d.get("limits")))
            .map(|(p, label, resets)| Badge {
                percent: p,
                label: format!("{} — {} (CLI)", prefix, label),
                resets_at: resets,
                forecast_at: fc(&cli_key),
                bg: color_of(&settings, name, fallback, "bg"),
                text: color_of(&settings, name, (255, 255, 255), "text"),
                cli: true,
            });
        sync_one(app, &cli_key, enabled, cli_badge, &settings);
    }
}
