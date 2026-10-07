//! Schematic drawing of both hands; the finger for the next key and the one
//! holding the modifier are highlighted (FG-4, FG-5).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::cairo::Context;
use gtk::prelude::*;
use keyquest_core::{Digit, Finger, Hand, KeyStroke};

use crate::colors::{Align, Palette, rounded_rect, text};

pub struct HandsView {
    pub area: gtk::DrawingArea,
    target: RefCell<Option<KeyStroke>>,
}

/// Relative finger lengths, pinky to thumb.
const LENGTHS: [(Digit, f64); 4] = [
    (Digit::Pinky, 0.55),
    (Digit::Ring, 0.78),
    (Digit::Middle, 0.88),
    (Digit::Index, 0.78),
];

impl HandsView {
    pub fn new() -> Rc<HandsView> {
        let area = gtk::DrawingArea::builder()
            .content_height(170)
            .hexpand(true)
            .accessible_role(gtk::AccessibleRole::Img)
            .build();
        area.update_property(&[gtk::accessible::Property::Label("Eller")]);
        let view = Rc::new(HandsView {
            area,
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

    fn draw(&self, area: &gtk::DrawingArea, cr: &Context, w: f64, h: f64) {
        let pal = Palette::current(area);
        let target = self.target.borrow();
        let main = target.as_ref().map(|t| t.finger);
        let modifier = target
            .as_ref()
            .and_then(|t| t.modifier.as_ref().map(|(_, f)| *f));

        let label_h = 22.0;
        let hand_h = (h - label_h).max(40.0);
        let hand_w = (hand_h * 1.05).min(w / 2.0 - 24.0);
        let fw = hand_w / 6.0;
        let finger_area = hand_h * 0.55;
        let palm_y = finger_area;
        let palm_h = hand_h - finger_area - 4.0;
        let gap_center = (w / 2.0 - hand_w).clamp(24.0, hand_w * 0.6);

        for hand in [Hand::Left, Hand::Right] {
            let x0 = match hand {
                Hand::Left => w / 2.0 - gap_center / 2.0 - hand_w,
                Hand::Right => w / 2.0 + gap_center / 2.0,
            };
            // Palm.
            rounded_rect(cr, x0 + fw * 0.2, palm_y, fw * 4.6, palm_h, fw * 0.8);
            pal.key.mix(pal.ink, 0.06).set(cr);
            let _ = cr.fill_preserve();
            pal.ink.set_alpha(cr, 0.15);
            cr.set_line_width(1.0);
            let _ = cr.stroke();

            // Four fingers, outer to inner for the left hand, mirrored for the right.
            for (i, (digit, len)) in LENGTHS.iter().enumerate() {
                let slot = match hand {
                    Hand::Left => i,
                    Hand::Right => 3 - i,
                } as f64;
                let fx = x0 + fw * (0.35 + slot * 1.15);
                let fh = finger_area * len + fw * 0.5;
                let fy = palm_y - fh + fw * 0.5;
                let finger = Finger::new(hand, *digit);
                draw_finger(cr, &pal, finger, fx, fy, fw, fh, main, modifier);
            }
            // Thumb, angled towards the centre, below the index finger.
            cr.save().ok();
            let (tx, angle) = match hand {
                Hand::Left => (x0 + fw * 5.0, -0.7),
                Hand::Right => (x0 + fw * 0.0, 0.7),
            };
            cr.translate(tx, palm_y + palm_h * 0.45);
            cr.rotate(angle);
            let th = finger_area * 0.55;
            draw_finger(
                cr,
                &pal,
                Finger::new(hand, Digit::Thumb),
                -fw / 2.0,
                -th / 2.0,
                fw,
                th,
                main,
                modifier,
            );
            cr.restore().ok();

            pal.muted_ink().set(cr);
            let name = match hand {
                Hand::Left => "Sol el",
                Hand::Right => "Sağ el",
            };
            text(
                cr,
                name,
                x0 + hand_w / 2.0 - fw * 0.3,
                h - 2.0,
                12.0,
                false,
                Align::Center,
                Align::End,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_finger(
    cr: &Context,
    pal: &Palette,
    finger: Finger,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    main: Option<Finger>,
    modifier: Option<Finger>,
) {
    let active = main == Some(finger) || modifier == Some(finger);
    rounded_rect(cr, x, y, w, h, w / 2.0);
    if active {
        pal.finger(finger).set(cr);
    } else {
        pal.finger_tint(finger).set(cr);
    }
    let _ = cr.fill_preserve();
    pal.ink.set_alpha(cr, if active { 0.9 } else { 0.18 });
    cr.set_line_width(if active { 3.0 } else { 1.0 });
    // A dashed outline marks the finger holding the modifier.
    if modifier == Some(finger) && main != Some(finger) {
        cr.set_dash(&[5.0, 3.0], 0.0);
    }
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
    pal.ink.set_alpha(cr, if active { 0.95 } else { 0.5 });
    text(
        cr,
        &(finger.digit as u8).to_string(),
        x + w / 2.0,
        y + w * 0.55,
        w * 0.42,
        active,
        Align::Center,
        Align::Center,
    );
}
