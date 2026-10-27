//! The line of text about the keys of a run, at the bottom of the picture.

use std::collections::HashMap;

use freeway_core::DisplayLanguage;
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory, IDWriteTextLayout,
};
use windows::core::{PCWSTR, Result, w};
use windows_numerics::Vector2;

use super::dashboard::Devices;
use crate::speedometer;

/// What is drawn: the hint, and over it the line that tells that time stands
/// still, if it does.
pub struct Overlay<'a> {
    pub text: &'a str,
    pub paused: Option<&'a str>,
    /// From 0 (gone) to 1.
    pub opacity: f32,
    pub language: DisplayLanguage,
}

/// Size of the text at a scale of 1 (see `speedometer::scale`).
const TEXT_SIZE: f32 = 15.0;
/// Distance from the bottom edge and the padding of the panel, at a scale of 1.
const BOTTOM_MARGIN: f32 = 24.0;
const PADDING: (f32, f32) = (14.0, 8.0);
const LINE_GAP: f32 = 4.0;
/// The text may take this part of the width of the window; it is made
/// smaller where it would not fit.
const WIDTH_SHARE: f32 = 0.92;
/// The panel behind the text, and the text, as the dashboard does: dimmed
/// like the lit parts of the speedometer.
const NIGHT_LEVEL: f32 = 0.6;
const PANEL_ALPHA: f32 = 0.5;
const TEXT_ALPHA: f32 = 0.85 * NIGHT_LEVEL;
const TEXT_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.88,
    g: 0.9,
    b: 0.95,
    a: 1.0,
};
const PAUSED_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 1.0,
    g: 0.75,
    b: 0.25,
    a: 1.0,
};
const BLACK: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
/// The cache holds the lines of a few sizes and languages.
const MAX_CACHED: usize = 32;

#[derive(Clone)]
struct Line {
    layout: IDWriteTextLayout,
    width: f32,
    height: f32,
}

/// The laid out lines, by text, size and language.
#[derive(Default)]
pub struct HintPainter {
    lines: HashMap<(String, u32, bool), Line>,
}

impl HintPainter {
    /// Draws the overlay at the bottom center of a window with the given
    /// pixel size. Does nothing where it is gone or does not fit.
    pub fn draw(
        &mut self,
        devices: &Devices,
        brush: &ID2D1SolidColorBrush,
        overlay: &Overlay,
        viewport: (f32, f32, f32),
    ) -> Result<()> {
        let (width, height, pixel_scale) = viewport;
        let scale = speedometer::scale(height, pixel_scale);
        if !(overlay.opacity > 0.0 && scale > 0.0) {
            return Ok(());
        }
        let mut size = TEXT_SIZE * scale;
        let mut hint = self.line(devices.dwrite, overlay.text, size, overlay.language)?;
        let room = width * WIDTH_SHARE - 2.0 * PADDING.0 * scale;
        if hint.width > room && hint.width > 0.0 {
            size *= room / hint.width;
            hint = self.line(devices.dwrite, overlay.text, size, overlay.language)?;
        }
        if !speedometer::text_visible(size, 1.0) {
            return Ok(());
        }
        let paused = match overlay.paused {
            Some(text) => Some(self.line(devices.dwrite, text, size, overlay.language)?),
            None => None,
        };
        let gap = LINE_GAP * scale;
        let content_width = paused
            .as_ref()
            .map_or(hint.width, |p| p.width.max(hint.width));
        let content_height = hint.height + paused.as_ref().map_or(0.0, |p| p.height + gap);
        let (pad_x, pad_y) = (PADDING.0 * scale, PADDING.1 * scale);
        let panel = D2D_RECT_F {
            left: (width - content_width) / 2.0 - pad_x,
            top: height - BOTTOM_MARGIN * scale - content_height - pad_y,
            right: (width + content_width) / 2.0 + pad_x,
            bottom: height - BOTTOM_MARGIN * scale + pad_y,
        };
        let radius = pad_y * 1.5;
        unsafe {
            brush.SetColor(&BLACK);
            brush.SetOpacity(PANEL_ALPHA * overlay.opacity);
            devices.context.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: panel,
                    radiusX: radius,
                    radiusY: radius,
                },
                brush,
            );
        }
        let mut top = panel.top + pad_y;
        if let Some(paused) = &paused {
            draw_line(
                devices,
                brush,
                paused,
                width,
                top,
                PAUSED_COLOR,
                overlay.opacity,
            );
            top += paused.height + gap;
        }
        draw_line(
            devices,
            brush,
            &hint,
            width,
            top,
            TEXT_COLOR,
            overlay.opacity,
        );
        Ok(())
    }

    fn line(
        &mut self,
        factory: &IDWriteFactory,
        text: &str,
        size: f32,
        language: DisplayLanguage,
    ) -> Result<Line> {
        let key = (
            text.to_owned(),
            size.to_bits(),
            language == DisplayLanguage::Japanese,
        );
        if !self.lines.contains_key(&key) {
            // Why: the sizes change only with the window, so the cache stays
            // small; it is emptied if a window is resized through many sizes.
            if self.lines.len() >= MAX_CACHED {
                self.lines.clear();
            }
            self.lines
                .insert(key.clone(), lay_out(factory, text, size, language)?);
        }
        Ok(self.lines[&key].clone())
    }
}

fn draw_line(
    devices: &Devices,
    brush: &ID2D1SolidColorBrush,
    line: &Line,
    width: f32,
    top: f32,
    color: D2D1_COLOR_F,
    opacity: f32,
) {
    unsafe {
        brush.SetColor(&color);
        brush.SetOpacity(TEXT_ALPHA * opacity);
        devices.context.DrawTextLayout(
            Vector2 {
                X: (width - line.width) / 2.0,
                Y: top,
            },
            &line.layout,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
    }
}

fn lay_out(
    factory: &IDWriteFactory,
    text: &str,
    size: f32,
    language: DisplayLanguage,
) -> Result<Line> {
    // Why: the face is one that has the arrows and, for Japanese, the
    // characters; DirectWrite substitutes another face for a missing glyph.
    let (font, locale): (PCWSTR, PCWSTR) = match language {
        DisplayLanguage::English => (w!("Segoe UI"), w!("en-us")),
        DisplayLanguage::Japanese => (w!("Yu Gothic UI"), w!("ja-jp")),
    };
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let format = factory.CreateTextFormat(
            font,
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            locale,
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        // The box is wide and tall enough that nothing is cut off.
        let layout = factory.CreateTextLayout(&wide, &format, size * 200.0, size * 2.0)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics)?;
        Ok(Line {
            layout,
            width: metrics.width,
            height: metrics.height,
        })
    }
}
