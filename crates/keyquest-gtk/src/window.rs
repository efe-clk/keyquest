//! Main window: lesson list and navigation between the screens.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::{gio, glib};
use keyquest_core::generator::{self, LessonGenerator};
use keyquest_core::store::{finger_stats, lesson_statuses, next_lesson, now_unix, open_lessons};
use keyquest_core::{Lesson, SessionResult, Summary};

use crate::practice::PracticeView;
use crate::state::State;
use crate::{results, settings, stats};

#[derive(Clone)]
enum Kind {
    Lesson(String),
    Weak,
}

pub struct Window {
    pub window: adw::ApplicationWindow,
    nav: adw::NavigationView,
    toasts: adw::ToastOverlay,
    lessons_view: adw::ToolbarView,
    lessons_page: adw::NavigationPage,
    state: RefCell<State>,
    practice: RefCell<Weak<PracticeView>>,
}

impl Window {
    pub fn new(app: &adw::Application) -> Rc<Window> {
        let state = State::load();
        settings::apply_theme(state.config.theme);

        let menu = gio::Menu::new();
        menu.append(Some("İstatistikler"), Some("win.stats"));
        menu.append(Some("Ayarlar"), Some("win.settings"));
        menu.append(Some("Hakkında"), Some("win.about"));
        let header = adw::HeaderBar::new();
        header.pack_end(
            &gtk::MenuButton::builder()
                .icon_name("open-menu-symbolic")
                .menu_model(&menu)
                .primary(true)
                .tooltip_text("Ana menü")
                .build(),
        );
        let lessons_view = adw::ToolbarView::new();
        lessons_view.add_top_bar(&header);
        let lessons_page = adw::NavigationPage::builder()
            .title("KeyQuest")
            .tag("lessons")
            .child(&lessons_view)
            .build();
        let nav = adw::NavigationView::new();
        nav.add(&lessons_page);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&nav));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("KeyQuest")
            .default_width(1100)
            .default_height(820)
            .width_request(360)
            .height_request(400)
            .content(&toasts)
            .build();

        let win = Rc::new(Window {
            window,
            nav,
            toasts,
            lessons_view,
            lessons_page,
            state: RefCell::new(state),
            practice: RefCell::new(Weak::new()),
        });
        win.add_actions();
        win.refresh_lessons();

        // Losing focus pauses the running session (blueprint 9.2).
        let weak = Rc::downgrade(&win);
        win.window.connect_is_active_notify(move |w| {
            if !w.is_active()
                && let Some(p) = weak
                    .upgrade()
                    .and_then(|win| win.practice.borrow().upgrade())
            {
                p.pause();
            }
        });
        let warnings = std::mem::take(&mut win.state.borrow_mut().warnings);
        for w in warnings {
            win.toast(&w);
        }
        win
    }

    fn toast(&self, msg: &str) {
        let t = adw::Toast::new(&glib::markup_escape_text(msg));
        t.set_timeout(6);
        self.toasts.add_toast(t);
    }

    fn add_actions(self: &Rc<Self>) {
        let add = |name: &str, f: fn(&Rc<Window>)| {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(w) = weak.upgrade() {
                    f(&w);
                }
            });
            self.window.add_action(&action);
        };
        add("stats", Window::show_stats);
        add("settings", Window::show_settings);
        add("about", Window::show_about);
    }

    fn refresh_lessons(self: &Rc<Self>) {
        let state = self.state.borrow();
        let layout = state.layout.clone();
        let lessons = state.lessons.for_layout(layout.id());
        let statuses = match lesson_statuses(&lessons, state.store.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                drop(state);
                self.toast(&format!("İlerleme okunamadı: {e}"));
                return;
            }
        };
        self.lessons_page
            .set_title(&format!("KeyQuest · {}", layout.name()));

        if lessons.is_empty() {
            let status = adw::StatusPage::builder()
                .icon_name("input-keyboard-symbolic")
                .title("Bu düzen için ders yok")
                .description(
                    "Ayarlardan başka bir düzen seçin veya kullanıcı ders dizinine ders ekleyin.",
                )
                .build();
            self.lessons_view.set_content(Some(&status));
            return;
        }

        let page = adw::PreferencesPage::new();
        let quick = adw::PreferencesGroup::new();
        if let Some(next) = next_lesson(&statuses) {
            let row = nav_row(
                &format!("Devam et: {}", next.title),
                "Sıradaki açık ders",
                "media-playback-start-symbolic",
            );
            let (weak, id) = (Rc::downgrade(self), next.id.clone());
            row.connect_activated(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.start(Kind::Lesson(id.clone()));
                }
            });
            quick.add(&row);
        }
        let weakest = state.store.weakest_keys(layout.id(), 6).unwrap_or_default();
        let weak_sub = if weakest.is_empty() {
            "Birkaç oturumdan sonra en çok hata yapılan tuşlara odaklanır".to_owned()
        } else {
            let keys: Vec<String> = weakest
                .iter()
                .map(|k| {
                    if k.ch == ' ' {
                        "boşluk".into()
                    } else {
                        k.ch.to_string()
                    }
                })
                .collect();
            format!("Odak: {}", keys.join(" "))
        };
        let row = nav_row("Zayıf tuş alıştırması", &weak_sub, "view-refresh-symbolic");
        let weak = Rc::downgrade(self);
        row.connect_activated(move |_| {
            if let Some(w) = weak.upgrade() {
                w.start(Kind::Weak);
            }
        });
        quick.add(&row);
        page.add(&quick);

        let group = adw::PreferencesGroup::builder().title("Dersler").build();
        for st in &statuses {
            let l = st.lesson;
            let mut sub = format!(
                "Hedef {:.0} WPM · %{:.0}",
                l.target_wpm,
                l.target_accuracy * 100.0
            );
            if let Some(p) = &st.progress {
                sub.push_str(&format!(
                    " — en iyi {:.0} WPM · %{:.0}",
                    p.best_wpm,
                    p.best_acc * 100.0
                ));
            }
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&l.title))
                .subtitle(sub)
                .activatable(st.unlocked)
                .sensitive(st.unlocked)
                .build();
            let (icon, tip) = match (st.completed(), st.unlocked) {
                (true, _) => ("emblem-ok-symbolic", "Tamamlandı"),
                (_, true) => ("media-playback-start-symbolic", "Açık"),
                _ => (
                    "system-lock-screen-symbolic",
                    "Kilitli: önceki dersi tamamlayın",
                ),
            };
            let img = gtk::Image::from_icon_name(icon);
            img.set_tooltip_text(Some(tip));
            img.update_property(&[gtk::accessible::Property::Label(tip)]);
            row.add_prefix(&img);
            if st.unlocked {
                row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            }
            let (weak, id) = (Rc::downgrade(self), l.id.clone());
            row.connect_activated(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.start(Kind::Lesson(id.clone()));
                }
            });
            group.add(&row);
        }
        page.add(&group);
        self.lessons_view.set_content(Some(&page));
    }

    fn start(self: &Rc<Self>, kind: Kind) {
        let state = self.state.borrow();
        let layout = state.layout.clone();
        let lessons = state.lessons.for_layout(layout.id());
        let lesson: Lesson = match &kind {
            Kind::Lesson(id) => match state.lessons.get(id) {
                Some(l) => l.clone(),
                None => return,
            },
            Kind::Weak => {
                let statuses = lesson_statuses(&lessons, state.store.as_ref()).unwrap_or_default();
                let open = open_lessons(&statuses);
                if open.is_empty() {
                    return;
                }
                generator::combined_lesson(layout.id(), &open)
            }
        };
        let weights = match kind {
            Kind::Weak => state
                .store
                .key_stats(layout.id())
                .ok()
                .map(|s| generator::weights_from_stats(&s, 5)),
            Kind::Lesson(_) => None,
        };
        let target = LessonGenerator::from_time().generate(
            &lesson,
            &layout,
            weights.as_ref(),
            lesson.length(),
        );
        if target.is_empty() {
            drop(state);
            self.toast("Bu ders için alıştırma metni üretilemedi");
            return;
        }
        let view = PracticeView::new(
            &lesson.title,
            &target,
            state.config.error_mode,
            layout,
            state.all_layouts.clone(),
            state.config.font_scale,
        );
        drop(state);

        let started_at = now_unix();
        let weak = Rc::downgrade(self);
        let (k, l) = (kind.clone(), lesson.clone());
        view.connect_finished(move |summary| {
            if let Some(w) = weak.upgrade() {
                // Defer: we are inside the practice page's key handler.
                let (k, l) = (k.clone(), l.clone());
                glib::idle_add_local_once(move || w.finish(k, l, started_at, summary));
            }
        });
        *self.practice.borrow_mut() = Rc::downgrade(&view);
        self.nav.push(&view.page);
        // The page keeps the view alive through its signal handlers' weak refs
        // only, so tie the view's lifetime to the page.
        let keep = RefCell::new(Some(view.clone()));
        view.page.connect_destroy(move |_| {
            keep.borrow_mut().take();
        });
    }

    fn finish(self: &Rc<Self>, kind: Kind, lesson: Lesson, started_at: i64, summary: Summary) {
        let is_lesson = matches!(kind, Kind::Lesson(_));
        let passed = is_lesson && lesson.passed(&summary);
        let layout_id = self.state.borrow().layout.id().to_owned();
        let result = SessionResult {
            lesson_id: lesson.id.clone(),
            layout: layout_id.clone(),
            started_at,
            completed: passed,
            summary: summary.clone(),
        };
        let saved = self.state.borrow_mut().store.save_session(&result);
        if let Err(e) = saved {
            // FOG-3: the result is still shown; the transaction left old data intact.
            self.toast(&format!("İlerleme kaydedilemedi: {e}"));
        }
        self.refresh_lessons();

        let next_id = if is_lesson {
            let state = self.state.borrow();
            let lessons = state.lessons.for_layout(&layout_id);
            let pos = lessons.iter().position(|l| l.id == lesson.id);
            pos.and_then(|i| lessons.get(i + 1)).map(|l| l.id.clone())
        } else {
            None
        };
        let unlocked_next = next_id.filter(|id| {
            let state = self.state.borrow();
            matches!(state.store.lesson_progress(&lesson.id), Ok(Some(p)) if p.completed)
                && state.lessons.get(id).is_some()
        });

        let weak = Rc::downgrade(self);
        let retry_kind = kind.clone();
        let actions = results::Actions {
            retry: Box::new({
                let weak = weak.clone();
                move || {
                    if let Some(w) = weak.upgrade() {
                        w.nav.pop_to_tag("lessons");
                        w.start(retry_kind.clone());
                    }
                }
            }),
            next: unlocked_next.map(|id| -> Box<dyn Fn()> {
                let weak = weak.clone();
                Box::new(move || {
                    if let Some(w) = weak.upgrade() {
                        w.nav.pop_to_tag("lessons");
                        w.start(Kind::Lesson(id.clone()));
                    }
                })
            }),
            lessons: Box::new(move || {
                if let Some(w) = weak.upgrade() {
                    w.nav.pop_to_tag("lessons");
                }
            }),
        };
        let page = results::page(
            &lesson.title,
            &summary,
            is_lesson.then_some(&lesson),
            passed,
            actions,
        );
        self.nav.replace(&[self.lessons_page.clone(), page]);
    }

    fn show_stats(self: &Rc<Self>) {
        let state = self.state.borrow();
        let id = state.layout.id().to_owned();
        let data = (|| {
            let history = state.store.history(&id, stats::DAYS)?;
            let weak = state.store.weakest_keys(&id, 10)?;
            let keys = state.store.key_stats(&id)?;
            Ok::<_, keyquest_core::StoreError>((history, weak, finger_stats(&state.layout, &keys)))
        })();
        drop(state);
        match data {
            Ok((h, w, f)) => self.nav.push(&stats::page(h, w, f)),
            Err(e) => self.toast(&format!("İstatistikler okunamadı: {e}")),
        }
    }

    fn show_settings(self: &Rc<Self>) {
        let state = self.state.borrow();
        let layouts: Vec<(String, String)> = state
            .layouts
            .defs()
            .map(|d| (d.id.clone(), d.name.clone()))
            .collect();
        let config = state.config.clone();
        drop(state);
        let weak = Rc::downgrade(self);
        let weak2 = weak.clone();
        let dialog = settings::dialog(
            &config,
            &layouts,
            move |c| {
                if let Some(w) = weak.upgrade() {
                    let res = w.state.borrow_mut().set_config(c);
                    if let Err(e) = res {
                        w.toast(&e);
                    }
                    w.refresh_lessons();
                }
            },
            move || {
                if let Some(w) = weak2.upgrade() {
                    let res = w.state.borrow_mut().store.reset();
                    match res {
                        Ok(()) => w.toast("İlerleme silindi"),
                        Err(e) => w.toast(&format!("İlerleme silinemedi: {e}")),
                    }
                    w.refresh_lessons();
                }
            },
        );
        dialog.present(Some(&self.window));
    }

    fn show_about(self: &Rc<Self>) {
        let about = adw::AboutDialog::builder()
            .application_name("KeyQuest")
            .application_icon(crate::APP_ID)
            .version(env!("CARGO_PKG_VERSION"))
            .comments("10 parmak klavye alıştırması")
            .license_type(gtk::License::Gpl30)
            .website(env!("CARGO_PKG_REPOSITORY"))
            .build();
        about.present(Some(&self.window));
    }
}

fn nav_row(title: &str, subtitle: &str, icon: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(title))
        .subtitle(glib::markup_escape_text(subtitle))
        .activatable(true)
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(icon));
    row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    row
}
