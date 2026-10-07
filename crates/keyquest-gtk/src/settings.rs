//! Settings dialog (FG-10).

use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
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

/// What the "İlerleme" group does; the window owns the progress store.
pub struct ProgressActions {
    /// Runs after the user confirms deleting their progress.
    pub reset: Box<dyn Fn()>,
    /// Receives the file chosen for the export.
    pub export: Box<dyn Fn(PathBuf)>,
    /// Receives the chosen file after the user confirms replacing progress.
    pub import: Box<dyn Fn(PathBuf)>,
}

fn json_dialog(title: &str) -> gtk::FileDialog {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JSON"));
    filter.add_pattern("*.json");
    filter.add_mime_type("application/json");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    gtk::FileDialog::builder()
        .title(title)
        .modal(true)
        .filters(&filters)
        .default_filter(&filter)
        .build()
}

fn action_row(
    title: &str,
    subtitle: &str,
    button: &str,
    css: &str,
) -> (adw::ActionRow, gtk::Button) {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let b = gtk::Button::builder()
        .label(button)
        .valign(gtk::Align::Center)
        .build();
    if !css.is_empty() {
        b.add_css_class(css);
    }
    row.add_suffix(&b);
    (row, b)
}

/// `layouts` are (id, name) pairs. `on_change` receives every edited config.
pub fn dialog(
    config: &Config,
    layouts: &[(String, String)],
    on_change: impl Fn(Config) + 'static,
    progress: ProgressActions,
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
    let (export_row, export) = action_row(
        "Dışa aktar",
        "Tüm oturumları ve istatistikleri bir JSON dosyasına kaydeder",
        "Dışa aktar",
        "",
    );
    let (import_row, import) = action_row(
        "İçe aktar",
        "Dışa aktarılmış bir dosyadan geri yükler; mevcut ilerlemenin yerine geçer",
        "İçe aktar",
        "",
    );
    let (reset_row, reset) = action_row(
        "İlerlemeyi sıfırla",
        "Tüm oturumlar, istatistikler ve açılan dersler silinir",
        "Sıfırla",
        "destructive-action",
    );
    data.add(&export_row);
    data.add(&import_row);
    data.add(&reset_row);

    let page = adw::PreferencesPage::new();
    page.add(&keyboard);
    page.add(&look);
    page.add(&data);
    let dialog = adw::PreferencesDialog::builder().title("Ayarlar").build();
    dialog.add(&page);

    let ProgressActions {
        reset: on_reset,
        export: on_export,
        import: on_import,
    } = progress;
    let parent = |d: &adw::PreferencesDialog| d.root().and_downcast::<gtk::Window>();

    let on_reset: Rc<dyn Fn()> = Rc::from(on_reset);
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

    let on_export: Rc<dyn Fn(PathBuf)> = Rc::from(on_export);
    let weak = dialog.downgrade();
    export.connect_clicked(move |_| {
        let Some(d) = weak.upgrade() else { return };
        let files = json_dialog("İlerlemeyi dışa aktar");
        files.set_initial_name(Some("keyquest-ilerleme.json"));
        let on_export = on_export.clone();
        files.save(parent(&d).as_ref(), gio::Cancellable::NONE, move |res| {
            if let Some(path) = res.ok().and_then(|f| f.path()) {
                on_export(path);
            }
        });
    });

    let on_import: Rc<dyn Fn(PathBuf)> = Rc::from(on_import);
    let weak = dialog.downgrade();
    import.connect_clicked(move |_| {
        let Some(d) = weak.upgrade() else { return };
        let on_import = on_import.clone();
        let weak = d.downgrade();
        json_dialog("İlerlemeyi içe aktar").open(
            parent(&d).as_ref(),
            gio::Cancellable::NONE,
            move |res| {
                let (Some(path), Some(d)) = (res.ok().and_then(|f| f.path()), weak.upgrade())
                else {
                    return;
                };
                let alert = adw::AlertDialog::new(
                    Some("İlerleme değiştirilsin mi?"),
                    Some("Mevcut tüm ilerleme, seçilen dosyadaki ilerlemeyle değiştirilecek."),
                );
                alert.add_responses(&[("cancel", "Vazgeç"), ("import", "İçe aktar")]);
                alert.set_response_appearance("import", adw::ResponseAppearance::Destructive);
                alert.set_default_response(Some("cancel"));
                let on_import = on_import.clone();
                alert.connect_response(Some("import"), move |_, _| on_import(path.clone()));
                alert.present(Some(&d));
            },
        );
    });
    dialog
}
