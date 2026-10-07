//! Statistics screen (FG-8): net WPM over time, weakest keys and fingers.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::cairo::Context;
use keyquest_core::{FingerStat, KeyStat, SessionSummary};

use crate::colors::{Align, Palette, Rgb, measure, rounded_rect, text};

pub const DAYS: u32 = 30;

pub fn page(
    history: Vec<SessionSummary>,
    weak: Vec<KeyStat>,
    fingers: Vec<FingerStat>,
) -> adw::NavigationPage {
    let prefs = adw::PreferencesPage::new();

    let mut description = format!("Son {DAYS} gün, {} oturum", history.len());
    if !history.is_empty() {
        let avg_acc = history.iter().map(|s| s.accuracy).sum::<f64>() / history.len() as f64;
        let best = history.iter().map(|s| s.wpm_net).fold(0.0, f64::max);
        description.push_str(&format!(
            " · en iyi {best:.1} WPM · ortalama doğruluk %{:.1}",
            avg_acc * 100.0
        ));
    }
    let trend = adw::PreferencesGroup::builder()
        .title("Net hız (WPM)")
        .description(description)
        .build();
    if history.is_empty() {
        trend.add(&adw::ActionRow::builder().title("Henüz oturum yok").build());
    } else {
        trend.add(&chart(history));
    }
    prefs.add(&trend);

    let keys = adw::PreferencesGroup::builder()
        .title("En zayıf tuşlar")
        .description("En az 5 kez yazılmış tuşlar, hata oranına göre")
        .build();
    if weak.is_empty() {
        keys.add(
            &adw::ActionRow::builder()
                .title("Henüz yeterli veri yok")
                .build(),
        );
    }
    for k in &weak {
        let name = if k.ch == ' ' {
            "boşluk".to_owned()
        } else {
            k.ch.to_string()
        };
        let mut sub = format!("{} hata / {} vuruş", k.misses, k.attempts());
        if let Some(ms) = k.avg_ms {
            sub.push_str(&format!(" · ortalama {ms} ms"));
        }
        keys.add(&rate_row(
            &name,
            &sub,
            k.misses as f64 / k.attempts() as f64,
        ));
    }
    prefs.add(&keys);

    let fingers_group = adw::PreferencesGroup::builder().title("Parmaklar").build();
    if fingers.is_empty() {
        fingers_group.add(&adw::ActionRow::builder().title("Henüz veri yok").build());
    }
    for f in &fingers {
        let title = format!("{} ({})", capitalize(&f.finger.label()), f.finger);
        let sub = format!("{} vuruş", f.hits + f.misses);
        fingers_group.add(&rate_row(&title, &sub, f.error_rate()));
    }
    prefs.add(&fingers_group);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&prefs));
    adw::NavigationPage::builder()
        .title("İstatistikler")
        .tag("stats")
        .child(&toolbar)
        .build()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn rate_row(title: &str, subtitle: &str, rate: f64) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let bar = gtk::LevelBar::builder()
        .min_value(0.0)
        .max_value(1.0)
        .value(rate.clamp(0.0, 1.0))
        .width_request(120)
        .valign(gtk::Align::Center)
        .build();
    // Error rate: a fuller bar is worse, so no "good/bad" offsets colouring.
    bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_LOW));
    bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_HIGH));
    bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_FULL));
    row.add_suffix(&bar);
    row.add_suffix(
        &gtk::Label::builder()
            .label(format!("%{:.1}", rate * 100.0))
            .width_chars(6)
            .xalign(1.0)
            .css_classes(["numeric"])
            .build(),
    );
    row
}

/// Line chart of net WPM per session, with a hover marker and value.
fn chart(history: Vec<SessionSummary>) -> gtk::Widget {
    let area = gtk::DrawingArea::builder()
        .content_height(200)
        .hexpand(true)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    area.update_property(&[gtk::accessible::Property::Label(
        "Oturumlara göre net hız grafiği",
    )]);
    let hover: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let points: Rc<Vec<SessionSummary>> = Rc::new(history);

    let motion = gtk::EventControllerMotion::new();
    let (h2, p2, a2) = (hover.clone(), points.clone(), area.downgrade());
    motion.connect_motion(move |_, x, _| {
        if let Some(a) = a2.upgrade() {
            let (left, right) = plot_x(a.width() as f64);
            let n = p2.len();
            let i = if n <= 1 {
                0
            } else {
                (((x - left) / (right - left)) * (n - 1) as f64)
                    .round()
                    .clamp(0.0, (n - 1) as f64) as usize
            };
            if h2.replace(Some(i)) != Some(i) {
                a.queue_draw();
            }
        }
    });
    let (h3, a3) = (hover.clone(), area.downgrade());
    motion.connect_leave(move |_| {
        h3.set(None);
        if let Some(a) = a3.upgrade() {
            a.queue_draw();
        }
    });
    area.add_controller(motion);

    area.set_draw_func(move |area, cr, w, h| {
        draw_chart(area, cr, w as f64, h as f64, &points, hover.get())
    });
    let frame = gtk::Box::builder().css_classes(["card"]).build();
    frame.append(&area);
    frame.upcast()
}

fn plot_x(w: f64) -> (f64, f64) {
    (44.0, w - 16.0)
}

fn draw_chart(
    area: &gtk::DrawingArea,
    cr: &Context,
    w: f64,
    h: f64,
    pts: &[SessionSummary],
    hover: Option<usize>,
) {
    let pal = Palette::current(area);
    let series = if pal.dark {
        Rgb(
            0x39 as f64 / 255.0,
            0x87 as f64 / 255.0,
            0xe5 as f64 / 255.0,
        )
    } else {
        Rgb(
            0x2a as f64 / 255.0,
            0x78 as f64 / 255.0,
            0xd6 as f64 / 255.0,
        )
    };
    let (left, right) = plot_x(w);
    let (top, bottom) = (16.0, h - 16.0);
    let max = pts.iter().map(|p| p.wpm_net).fold(10.0, f64::max);
    let step = nice_step(max / 4.0);
    let y_max = (max / step).ceil() * step;
    let y = |v: f64| bottom - (v / y_max) * (bottom - top);
    let x = |i: usize| {
        if pts.len() <= 1 {
            (left + right) / 2.0
        } else {
            left + (right - left) * i as f64 / (pts.len() - 1) as f64
        }
    };

    // Recessive grid and y labels.
    cr.set_line_width(1.0);
    let mut v = 0.0;
    while v <= y_max + 1e-9 {
        pal.ink.set_alpha(cr, 0.08);
        cr.move_to(left, y(v).round() + 0.5);
        cr.line_to(right, y(v).round() + 0.5);
        let _ = cr.stroke();
        pal.muted_ink().set(cr);
        text(
            cr,
            &format!("{v:.0}"),
            left - 8.0,
            y(v),
            11.0,
            false,
            Align::End,
            Align::Center,
        );
        v += step;
    }

    series.set(cr);
    cr.set_line_width(2.0);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    for (i, p) in pts.iter().enumerate() {
        if i == 0 {
            cr.move_to(x(i), y(p.wpm_net));
        } else {
            cr.line_to(x(i), y(p.wpm_net));
        }
    }
    let _ = cr.stroke();
    if pts.len() == 1 {
        cr.arc(x(0), y(pts[0].wpm_net), 4.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }

    if let Some(i) = hover.filter(|i| *i < pts.len()) {
        let (px, py) = (x(i), y(pts[i].wpm_net));
        pal.ink.set_alpha(cr, 0.25);
        cr.move_to(px.round() + 0.5, top);
        cr.line_to(px.round() + 0.5, bottom);
        let _ = cr.stroke();
        pal.surface.set(cr);
        cr.arc(px, py, 6.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        series.set(cr);
        cr.arc(px, py, 4.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();

        let p = &pts[i];
        let label = format!(
            "{:.1} WPM · %{:.0} · {}",
            p.wpm_net,
            p.accuracy * 100.0,
            p.lesson_id
        );
        let tw = measure(cr, &label, 12.0) + 16.0;
        let bx = (px + 10.0).min(w - tw - 4.0).max(4.0);
        let by = (py - 34.0).max(2.0);
        rounded_rect(cr, bx, by, tw, 24.0, 6.0);
        pal.key.set(cr);
        let _ = cr.fill_preserve();
        pal.ink.set_alpha(cr, 0.2);
        let _ = cr.stroke();
        pal.ink.set(cr);
        text(
            cr,
            &label,
            bx + 8.0,
            by + 12.0,
            12.0,
            false,
            Align::Start,
            Align::Center,
        );
    }
}

fn nice_step(raw: f64) -> f64 {
    let mag = 10f64.powf(raw.max(1e-9).log10().floor());
    let n = raw / mag;
    let s = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    s * mag
}
