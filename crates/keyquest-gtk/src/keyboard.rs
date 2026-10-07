//! Virtual keyboard (FG-4, FG-5, FG-6): every key is tinted with its finger's
//! colour; the next key and the modifier it needs are highlighted.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::cairo::Context;
use gtk::prelude::*;
use keyquest_core::layout::{ALTGR, SHIFT_LEFT, SHIFT_RIGHT, SPACE};
use keyquest_core::{Finger, Key, KeyId, KeyStroke, Layout};

use crate::colors::{Align, Palette, Rgb, rounded_rect, text};

/// Keyboard width in key units.
const WIDTH: f64 = 15.0;
/// Number rows plus the space-bar row.
const ROWS: f64 = 5.0;

pub struct KeyboardView {
    pub area: gtk::DrawingArea,
    layout: Rc<Layout>,
    target: RefCell<Option<KeyStroke>>,
}

/// One rectangle to draw: a layout key, a modifier, or a decorative key.
struct Cap<'a> {
    x: f64,
    y: f64,
    w: f64,
    key: Option<&'a Key>,
    id: Option<KeyId>,
    label: &'static str,
}

impl KeyboardView {
    pub fn new(layout: Rc<Layout>) -> Rc<KeyboardView> {
        let area = gtk::DrawingArea::builder()
            .content_height(250)
            .hexpand(true)
            .accessible_role(gtk::AccessibleRole::Img)
            .build();
        area.update_property(&[gtk::accessible::Property::Label("Sanal klavye")]);
        let view = Rc::new(KeyboardView {
            area,
            layout,
            target: RefCell::new(None),
        });
        let weak = Rc::downgrade(&view);
        view.area.set_draw_func(move |area, cr, w, h| {
            if let Some(v) = weak.upgrade() {
                v.draw(area, cr, w as f64, h as f64);
            }
        });
        let weak = Rc::downgrade(&view);
        adw::StyleManager::default().connect_dark_notify(move |_| {
            if let Some(v) = weak.upgrade() {
                v.area.queue_draw();
            }
        });
        view
    }

    pub fn set_target(&self, stroke: Option<KeyStroke>) {
        *self.target.borrow_mut() = stroke;
        self.area.queue_draw();
    }

    fn caps(&self) -> Vec<Cap<'_>> {
        let mut caps = Vec::new();
        let rows = self.layout.rows();
        let has_iso = rows.iter().flatten().any(|k| k.id.as_str() == "KEY_102ND");
        for (r, row) in rows.iter().enumerate().take(4) {
            let y = r as f64;
            let (lead, lead_label, lead_id) = match r {
                1 => (1.5, "Tab", None),
                2 => (1.75, "Caps", None),
                3 => (
                    if has_iso { 1.25 } else { 2.25 },
                    "Shift",
                    Some(KeyId::new(SHIFT_LEFT)),
                ),
                _ => (0.0, "", None),
            };
            if lead > 0.0 {
                caps.push(Cap {
                    x: 0.0,
                    y,
                    w: lead,
                    key: None,
                    id: lead_id,
                    label: lead_label,
                });
            }
            let mut x = lead;
            for key in row {
                caps.push(Cap {
                    x,
                    y,
                    w: 1.0,
                    key: Some(key),
                    id: Some(key.id.clone()),
                    label: "",
                });
                x += 1.0;
            }
            let rest = WIDTH - x;
            if rest >= 0.75 {
                let (label, id) = match r {
                    0 => ("⌫", None),
                    3 => ("Shift", Some(KeyId::new(SHIFT_RIGHT))),
                    _ => ("↵", None),
                };
                caps.push(Cap {
                    x,
                    y,
                    w: rest,
                    key: None,
                    id,
                    label,
                });
            }
        }
        let y = 4.0;
        let bottom: [(f64, &'static str, Option<&str>); 8] = [
            (1.25, "Ctrl", None),
            (1.25, "", None),
            (1.25, "Alt", None),
            (6.25, "", Some(SPACE)),
            (1.25, "AltGr", Some(ALTGR)),
            (1.25, "", None),
            (1.25, "", None),
            (1.25, "Ctrl", None),
        ];
        let mut x = 0.0;
        for (w, label, id) in bottom {
            caps.push(Cap {
                x,
                y,
                w,
                key: None,
                id: id.map(KeyId::new),
                label,
            });
            x += w;
        }
        caps
    }

    fn draw(&self, area: &gtk::DrawingArea, cr: &Context, w: f64, h: f64) {
        let pal = Palette::current(area);
        let unit = (w / WIDTH).min(h / ROWS).floor();
        let ox = ((w - unit * WIDTH) / 2.0).floor();
        let oy = ((h - unit * ROWS) / 2.0).floor();
        let target = self.target.borrow();
        let main = target.as_ref().map(|t| (&t.key, t.finger));
        let modifier = target
            .as_ref()
            .and_then(|t| t.modifier.as_ref().map(|(k, f)| (k, *f)));
        let gap = (unit * 0.06).max(2.0);

        for cap in self.caps() {
            let x = ox + cap.x * unit + gap / 2.0;
            let y = oy + cap.y * unit + gap / 2.0;
            let (kw, kh) = (cap.w * unit - gap, unit - gap);
            let finger: Option<Finger> = cap.id.as_ref().and_then(|id| self.layout.finger_of(id));
            let active = cap.id.as_ref().and_then(|id| {
                [main, modifier]
                    .into_iter()
                    .flatten()
                    .find(|(k, _)| *k == id)
                    .map(|(_, f)| f)
            });

            let fill = match (active, finger) {
                (Some(f), _) => pal.finger(f),
                (None, Some(f)) => pal.finger_tint(f),
                (None, None) => pal.key.mix(pal.surface, 0.5),
            };
            rounded_rect(cr, x, y, kw, kh, unit * 0.12);
            fill.set(cr);
            let _ = cr.fill_preserve();
            pal.ink
                .set_alpha(cr, if active.is_some() { 0.9 } else { 0.12 });
            cr.set_line_width(if active.is_some() { 3.0 } else { 1.0 });
            let _ = cr.stroke();

            let ink = if active.is_some() {
                if pal.dark {
                    Rgb(1.0, 1.0, 1.0)
                } else {
                    Rgb(0.0, 0.0, 0.0)
                }
            } else {
                pal.ink
            };
            let px = unit * 0.36;
            if let Some(key) = cap.key {
                draw_key_labels(cr, key, x, y, kw, kh, px, ink, pal);
                if key.home {
                    ink.set_alpha(cr, 0.5);
                    cr.rectangle(
                        x + kw / 2.0 - unit * 0.12,
                        y + kh - unit * 0.14,
                        unit * 0.24,
                        2.0,
                    );
                    let _ = cr.fill();
                }
            } else if !cap.label.is_empty() {
                ink.set_alpha(cr, if cap.id.is_some() { 0.9 } else { 0.5 });
                text(
                    cr,
                    cap.label,
                    x + kw / 2.0,
                    y + kh / 2.0,
                    px * 0.7,
                    false,
                    Align::Center,
                    Align::Center,
                );
            }
            // Finger label, so colour is never the only cue.
            if let Some(f) = finger {
                ink.set_alpha(cr, if active.is_some() { 0.85 } else { 0.45 });
                text(
                    cr,
                    &f.to_string(),
                    x + kw - unit * 0.07,
                    y + unit * 0.05,
                    px * 0.5,
                    false,
                    Align::End,
                    Align::Start,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_key_labels(
    cr: &Context,
    key: &Key,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    px: f64,
    ink: Rgb,
    pal: Palette,
) {
    let pad = w * 0.12;
    let simple_case =
        matches!((key.base, key.shift), (Some(b), Some(s)) if b.is_lowercase() && s.is_uppercase());
    ink.set(cr);
    if simple_case {
        let s = key.shift.unwrap().to_string();
        text(
            cr,
            &s,
            x + w / 2.0,
            y + h / 2.0,
            px,
            true,
            Align::Center,
            Align::Center,
        );
    } else {
        if let Some(s) = key.shift {
            text(
                cr,
                &s.to_string(),
                x + pad,
                y + h * 0.08,
                px * 0.75,
                false,
                Align::Start,
                Align::Start,
            );
        }
        if let Some(b) = key.base {
            text(
                cr,
                &b.to_string(),
                x + pad,
                y + h * 0.92,
                px * 0.85,
                true,
                Align::Start,
                Align::End,
            );
        }
    }
    if let Some(a) = key.altgr {
        pal.ink.mix(pal.surface, 0.0).set_alpha(cr, 0.55);
        text(
            cr,
            &a.to_string(),
            x + w - pad,
            y + h * 0.92,
            px * 0.6,
            false,
            Align::End,
            Align::End,
        );
    }
}
