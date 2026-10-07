//! Settings dialog (FG-10).

use std::rc::Rc;

use adw::prelude::*;
use keyquest_core::{ErrorMode, Hand};
use keyquest_data::{Config, Theme};

pub fn apply_theme(theme: Theme) {
    adw::StyleManager::default().set_color_scheme(match theme {
        Theme::System => adw::ColorScheme::Default,
        Theme::Light => adw::ColorScheme::ForceLight,
        Theme::Dark => adw::ColorScheme::ForceDark,
    });
}

fn combo(title: &str, subtitle: Option<&str>, items: &[&str], selected: usize) -> adw::ComboRow {
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(items))
        .selected(selected as u32)
        .build();
    if let Some(s) = subtitle {
        row.set_subtitle(s);
    }
    row
}

/// `layouts` are (id, name) pairs. `on_change` receives every edited config;
/// `on_reset` runs after the user confirms deleting their progress.
pub fn dialog(
    config: &Config,
    layouts: &[(String, String)],
    on_change: impl Fn(Config) + 'static,
    on_reset: impl Fn() + 'static,
) -> adw::PreferencesDialog {
    let current = Rc::new(std::cell::RefCell::new(config.clone()));
    let on_change = Rc::new(on_change);
    let update = {
        let current = current.clone();
        let on_change = on_change.clone();
        move |f: &dyn Fn(&mut Config)| {
            f(&mut current.borrow_mut());
            on_change(current.borrow().clone());
        }
    };
    let update = Rc::new(update);

    let keyboard = adw::PreferencesGroup::builder().title("Klavye").build();
    let names: Vec<&str> = layouts.iter().map(|(_, n)| n.as_str()).collect();
    let layout_row = combo(
        "Klavye düzeni",
        Some("Sistemdeki düzenle aynı olmalı"),
        &names,
        layouts
            .iter()
            .position(|(id, _)| *id == config.layout)
            .unwrap_or(0),
    );
    let ids: Vec<String> = layouts.iter().map(|(id, _)| id.clone()).collect();
    let u = update.clone();
    layout_row.connect_selected_notify(move |r| {
        if let Some(id) = ids.get(r.selected() as usize).cloned() {
            u(&move |c: &mut Config| c.layout = id.clone());
        }
    });
    keyboard.add(&layout_row);

    let space_row = combo(
        "Boşluk tuşu",
        None,
        &["Sağ başparmak", "Sol başparmak"],
        (config.space_thumb == Hand::Left) as usize,
    );
    let u = update.clone();
    space_row.connect_selected_notify(move |r| {
        let hand = if r.selected() == 1 {
            Hand::Left
        } else {
            Hand::Right
        };
        u(&move |c: &mut Config| c.space_thumb = hand);
    });
    keyboard.add(&space_row);

    let error_row = combo(
        "Hata olduğunda",
        Some("“Hatada dur”: doğru tuşa basana kadar imleç ilerlemez"),
        &["Hatada dur", "Devam et"],
        (config.error_mode == ErrorMode::Continue) as usize,
    );
    let u = update.clone();
    error_row.connect_selected_notify(move |r| {
        let mode = if r.selected() == 1 {
            ErrorMode::Continue
        } else {
            ErrorMode::StopOnError
        };
        u(&move |c: &mut Config| c.error_mode = mode);
    });
    keyboard.add(&error_row);

    let look = adw::PreferencesGroup::builder().title("Görünüm").build();
    let themes = [Theme::System, Theme::Light, Theme::Dark];
    let theme_row = combo(
        "Tema",
        None,
        &["Sistem", "Açık", "Koyu"],
        themes.iter().position(|t| *t == config.theme).unwrap_or(0),
    );
    let u = update.clone();
    theme_row.connect_selected_notify(move |r| {
        let theme = themes[(r.selected() as usize).min(2)];
        apply_theme(theme);
        u(&move |c: &mut Config| c.theme = theme);
    });
    look.add(&theme_row);
    let font_row = adw::SpinRow::with_range(0.75, 2.0, 0.05);
    font_row.set_title("Metin boyutu");
    font_row.set_digits(2);
    font_row.set_value(config.font_scale);
    let u = update.clone();
    font_row.connect_value_notify(move |r| {
        let v = r.value();
        u(&move |c: &mut Config| c.font_scale = v);
    });
    look.add(&font_row);

    let data = adw::PreferencesGroup::builder().title("İlerleme").build();
    let reset_row = adw::ActionRow::builder()
        .title("İlerlemeyi sıfırla")
        .subtitle("Tüm oturumlar, istatistikler ve açılan dersler silinir")
        .build();
    let reset = gtk::Button::builder()
        .label("Sıfırla")
        .valign(gtk::Align::Center)
        .css_classes(["destructive-action"])
        .build();
    reset_row.add_suffix(&reset);
    data.add(&reset_row);

    let page = adw::PreferencesPage::new();
    page.add(&keyboard);
    page.add(&look);
    page.add(&data);
    let dialog = adw::PreferencesDialog::builder().title("Ayarlar").build();
    dialog.add(&page);

    let on_reset = Rc::new(on_reset);
    let weak = dialog.downgrade();
    reset.connect_clicked(move |_| {
        let alert = adw::AlertDialog::new(
            Some("İlerleme silinsin mi?"),
            Some("Bu işlem geri alınamaz."),
        );
        alert.add_responses(&[("cancel", "Vazgeç"), ("reset", "Sıfırla")]);
        alert.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
        alert.set_default_response(Some("cancel"));
        let on_reset = on_reset.clone();
        alert.connect_response(Some("reset"), move |_, _| on_reset());
        alert.present(weak.upgrade().as_ref());
    });
    dialog
}
