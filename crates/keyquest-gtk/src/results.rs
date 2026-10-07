//! Session results (FG-7).

use adw::prelude::*;
use keyquest_core::{Lesson, Summary};

pub struct Actions {
    pub retry: Box<dyn Fn()>,
    pub next: Option<Box<dyn Fn()>>,
    pub lessons: Box<dyn Fn()>,
}

pub fn page(
    title: &str,
    summary: &Summary,
    lesson: Option<&Lesson>,
    passed: bool,
    actions: Actions,
) -> adw::NavigationPage {
    let (icon, heading) = if passed {
        ("emblem-ok-symbolic", "Hedef aşıldı!")
    } else {
        ("view-refresh-symbolic", "Tamamlandı")
    };
    let description = match lesson {
        Some(l) if !passed && l.target_wpm > 0.0 => format!(
            "Sonraki dersi açmak için hedef: {:.0} WPM ve %{:.0} doğruluk",
            l.target_wpm,
            l.target_accuracy * 100.0
        ),
        Some(_) if passed => "Sonraki ders açıldı.".to_owned(),
        _ => title.to_owned(),
    };

    let group = adw::PreferencesGroup::new();
    let row = |title: &str, value: String| {
        let r = adw::ActionRow::builder().title(title).build();
        r.add_suffix(
            &gtk::Label::builder()
                .label(value)
                .css_classes(["numeric"])
                .build(),
        );
        group.add(&r);
    };
    row("Net hız", format!("{:.1} WPM", summary.wpm_net));
    row("Brüt hız", format!("{:.1} WPM", summary.wpm_gross));
    row("Doğruluk", format!("%{:.1}", summary.accuracy * 100.0));
    row("Süre", format!("{:.1} sn", summary.duration.as_secs_f64()));
    row(
        "Vuruş",
        format!(
            "{} ({} hatalı, {} düzeltilmedi)",
            summary.chars_total, summary.errors, summary.uncorrected_errors
        ),
    );
    let missed: Vec<String> = summary
        .missed_keys()
        .iter()
        .take(10)
        .map(|k| format!("{}×{}", if k.ch == ' ' { '␣' } else { k.ch }, k.misses))
        .collect();
    if !missed.is_empty() {
        row("Hatalı tuşlar", missed.join("  "));
    }

    let buttons = gtk::Box::builder()
        .spacing(12)
        .halign(gtk::Align::Center)
        .margin_top(12)
        .build();
    let button = |label: &str, suggested: bool, f: Box<dyn Fn()>| {
        let b = gtk::Button::builder()
            .label(label)
            .css_classes(if suggested {
                vec!["pill", "suggested-action"]
            } else {
                vec!["pill"]
            })
            .build();
        b.connect_clicked(move |_| f());
        buttons.append(&b);
        b
    };
    let has_next = actions.next.is_some();
    let retry = button("Tekrar dene", !(passed && has_next), actions.retry);
    let next = actions.next.map(|f| button("Sonraki ders", passed, f));
    button("Ders listesi", false, actions.lessons);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&group);
    content.append(&buttons);
    let status = adw::StatusPage::builder()
        .icon_name(icon)
        .title(heading)
        .description(glib_escape(&description))
        .child(
            &adw::Clamp::builder()
                .maximum_size(520)
                .child(&content)
                .build(),
        )
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&status));
    let page = adw::NavigationPage::builder()
        .title("Sonuç")
        .tag("results")
        .child(&toolbar)
        .build();
    let default = next.filter(|_| passed).unwrap_or(retry);
    page.connect_shown(move |_| {
        default.grab_focus();
    });
    page
}

fn glib_escape(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}
