//! Finger colours and small Cairo/Pango drawing helpers.
//!
//! Fingers of the same kind share a hue on both hands (pinky, ring, middle,
//! index); thumbs are neutral. Hues come in a fixed order from a palette
//! validated for colour-vision deficiencies, and colour is never the only cue:
//! keys and fingers also carry a finger label.

use gtk::cairo::Context;
use gtk::pango;
use keyquest_core::{Digit, Finger};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub f64, pub f64, pub f64);

impl Rgb {
    const fn hex(v: u32) -> Rgb {
        Rgb(
            ((v >> 16) & 0xff) as f64 / 255.0,
            ((v >> 8) & 0xff) as f64 / 255.0,
            (v & 0xff) as f64 / 255.0,
        )
    }

    pub fn mix(self, other: Rgb, t: f64) -> Rgb {
        Rgb(
            self.0 + (other.0 - self.0) * t,
            self.1 + (other.1 - self.1) * t,
            self.2 + (other.2 - self.2) * t,
        )
    }

    pub fn set(self, cr: &Context) {
        cr.set_source_rgb(self.0, self.1, self.2);
    }

    pub fn set_alpha(self, cr: &Context, a: f64) {
        cr.set_source_rgba(self.0, self.1, self.2, a);
    }

    pub fn from_rgba(c: &gtk::gdk::RGBA) -> Rgb {
        Rgb(c.red() as f64, c.green() as f64, c.blue() as f64)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub surface: Rgb,
    pub key: Rgb,
    pub ink: Rgb,
}

impl Palette {
    pub fn current(widget: &impl gtk::prelude::WidgetExt) -> Palette {
        let dark = adw::StyleManager::default().is_dark();
        let ink = Rgb::from_rgba(&widget.color());
        if dark {
            Palette {
                dark,
                surface: Rgb::hex(0x1e1e1e),
                key: Rgb::hex(0x363636),
                ink,
            }
        } else {
            Palette {
                dark,
                surface: Rgb::hex(0xfafafa),
                key: Rgb::hex(0xffffff),
                ink,
            }
        }
    }

    /// Full-strength colour for a finger.
    pub fn finger(&self, f: Finger) -> Rgb {
        let (light, dark) = match f.digit {
            Digit::Pinky => (0x2a78d6, 0x3987e5),
            Digit::Ring => (0xeb6834, 0xd95926),
            Digit::Middle => (0x1baf7a, 0x199e70),
            Digit::Index => (0xeda100, 0xc98500),
            Digit::Thumb => (0x8a8a85, 0x8a8a85),
        };
        Rgb::hex(if self.dark { dark } else { light })
    }

    /// Faint tint of a finger's colour for key backgrounds (FG-6).
    pub fn finger_tint(&self, f: Finger) -> Rgb {
        self.key
            .mix(self.finger(f), if self.dark { 0.32 } else { 0.24 })
    }

    pub fn muted_ink(&self) -> Rgb {
        self.ink.mix(self.surface, 0.45)
    }
}

pub fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}

#[derive(Debug, Clone, Copy)]
pub enum Align {
    Start,
    Center,
    End,
}

fn font(px: f64, bold: bool) -> pango::FontDescription {
    let mut font = pango::FontDescription::from_string("Sans");
    font.set_absolute_size(px * pango::SCALE as f64);
    if bold {
        font.set_weight(pango::Weight::Bold);
    }
    font
}

/// Width of `s` in pixels at the given size.
pub fn measure(cr: &Context, s: &str, px: f64) -> f64 {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&font(px, false)));
    layout.set_text(s);
    layout.pixel_size().0 as f64
}

/// Draws `text` with its box anchored at (x, y) according to the alignments.
#[allow(clippy::too_many_arguments)]
pub fn text(cr: &Context, s: &str, x: f64, y: f64, px: f64, bold: bool, h: Align, v: Align) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&font(px, bold)));
    layout.set_text(s);
    let (w, hgt) = layout.pixel_size();
    let dx = match h {
        Align::Start => 0.0,
        Align::Center => -w as f64 / 2.0,
        Align::End => -w as f64,
    };
    let dy = match v {
        Align::Start => 0.0,
        Align::Center => -hgt as f64 / 2.0,
        Align::End => -hgt as f64,
    };
    cr.move_to(x + dx, y + dy);
    pangocairo::functions::show_layout(cr, &layout);
}
