//! Practice screen (FG-3, FG-4, FG-5). Characters come from the input method
//! (`IMMulticontext`), so dead keys and the desktop's layout are resolved
//! before they reach the session; comparison is by character (KR-5).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::{gdk, glib};
use keyquest_core::{
    CharStatus, ErrorMode, InputResult, Layout, LayoutMismatch, Session, SessionState, Summary,
};

use crate::hands::HandsView;
use crate::keyboard::KeyboardView;

/// Wrong keystrokes that must point at the same other layout before warning.
const MISMATCH_THRESHOLD: u32 = 4;

type FinishFn = Box<dyn Fn(Summary)>;

pub struct PracticeView {
    pub page: adw::NavigationPage,
    focus: gtk::Box,
    session: RefCell<Session>,
    clock: Instant,
    layout: Rc<Layout>,
    all_layouts: Rc<Vec<Layout>>,
    text: gtk::Label,
    hint: gtk::Label,
    status: gtk::Label,
    banner: adw::Banner,
    keyboard: Rc<KeyboardView>,
    hands: Rc<HandsView>,
    mismatch: RefCell<LayoutMismatch>,
    font_scale: f64,
    finished: Cell<bool>,
    on_finish: RefCell<Option<FinishFn>>,
}

impl PracticeView {
    pub fn new(
        title: &str,
        target: &str,
        mode: ErrorMode,
        layout: Rc<Layout>,
        all_layouts: Rc<Vec<Layout>>,
        font_scale: f64,
    ) -> Rc<PracticeView> {
        let text = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .css_classes(["practice-text"])
            .build();
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["card", "practice-card"])
            .build();
        card.append(&text);

        let status = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["numeric", "title-4"])
            .build();
        let help = gtk::Label::builder()
            .xalign(1.0)
            .css_classes(["dim-label", "caption"])
            .label("Esc: duraklat")
            .build();
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top.append(&status);
        top.append(&help);

        let hint = gtk::Label::builder()
            .xalign(0.5)
            .css_classes(["title-4"])
            .build();
        let keyboard = KeyboardView::new(layout.clone());
        let hands = HandsView::new();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .focusable(true)
            .build();
        content.append(&top);
        content.append(&card);
        content.append(&hint);
        content.append(&keyboard.area);
        content.append(&hands.area);

        let clamp = adw::Clamp::builder()
            .maximum_size(1100)
            .child(&content)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .vexpand(true)
            .build();
        let banner = adw::Banner::new("");
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.add_top_bar(&banner);
        toolbar.set_content(Some(&scroller));
        let page = adw::NavigationPage::builder()
            .title(title)
            .tag("practice")
            .child(&toolbar)
            .build();

        let view = Rc::new(PracticeView {
            page,
            focus: content,
            session: RefCell::new(Session::new(target, mode)),
            clock: Instant::now(),
            layout,
            all_layouts,
            text,
            hint,
            status,
            banner,
            keyboard,
            hands,
            mismatch: RefCell::new(LayoutMismatch::new(MISMATCH_THRESHOLD)),
            font_scale,
            finished: Cell::new(false),
            on_finish: RefCell::new(None),
        });
        view.connect_input();
        view.connect_lifecycle();
        view.refresh();
        view
    }

    pub fn connect_finished(&self, f: impl Fn(Summary) + 'static) {
        *self.on_finish.borrow_mut() = Some(Box::new(f));
    }

    fn now(&self) -> Duration {
        self.clock.elapsed()
    }

    fn connect_input(self: &Rc<Self>) {
        let im = gtk::IMMulticontext::new();
        im.set_client_widget(Some(&self.focus));
        let keys = gtk::EventControllerKey::new();
        keys.set_im_context(Some(&im));

        let weak = Rc::downgrade(self);
        im.connect_commit(move |_, s| {
            if let Some(v) = weak.upgrade() {
                for ch in s.chars().filter(|c| !c.is_control()) {
                    v.type_char(ch);
                }
            }
        });
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(v) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            match key {
                gdk::Key::Escape => {
                    v.toggle_pause();
                    glib::Propagation::Stop
                }
                gdk::Key::BackSpace => {
                    v.session.borrow_mut().backspace();
                    v.refresh();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.focus.add_controller(keys);

        let focus = gtk::EventControllerFocus::new();
        let im2 = im.clone();
        focus.connect_enter(move |_| im2.focus_in());
        focus.connect_leave(move |_| im.focus_out());
        self.focus.add_controller(focus);

        // Clicking anywhere gives the keyboard focus back.
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |_, _, _, _| {
            if let Some(v) = weak.upgrade() {
                v.focus.grab_focus();
            }
        });
        self.focus.add_controller(click);
    }

    fn connect_lifecycle(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.page.connect_shown(move |_| {
            if let Some(v) = weak.upgrade() {
                v.focus.grab_focus();
            }
        });
        // Leaving the page (back button) abandons the session.
        let weak = Rc::downgrade(self);
        self.page.connect_hidden(move |_| {
            if let Some(v) = weak.upgrade() {
                v.session.borrow_mut().cancel();
            }
        });
        let weak = Rc::downgrade(self);
        self.banner.connect_button_clicked(move |b| {
            b.set_revealed(false);
            if let Some(v) = weak.upgrade() {
                v.focus.grab_focus();
            }
        });
        // Live WPM while typing.
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(250), move || match weak.upgrade() {
            Some(v)
                if matches!(
                    v.session.borrow().state(),
                    SessionState::Ready | SessionState::Typing | SessionState::Paused
                ) =>
            {
                v.refresh_status();
                glib::ControlFlow::Continue
            }
            _ => glib::ControlFlow::Break,
        });
    }

    fn type_char(&self, ch: char) {
        let started = Instant::now();
        let now = self.now();
        let result = {
            let mut s = self.session.borrow_mut();
            if s.state() == SessionState::Paused {
                // The key that resumes is not part of the text.
                s.resume(now);
                InputResult::Ignored
            } else {
                s.input(ch, now)
            }
        };
        if let InputResult::Wrong { expected } = result {
            let found = self
                .mismatch
                .borrow_mut()
                .record(&self.layout, &self.all_layouts, expected, ch)
                .map(str::to_owned);
            if let Some(id) = found
                && !self.banner.is_revealed()
            {
                let name = self
                    .all_layouts
                    .iter()
                    .find(|l| l.id() == id)
                    .map(|l| l.name().to_owned())
                    .unwrap_or(id);
                self.banner.set_title(&format!(
                        "Sistem klavye düzeniniz {name} gibi görünüyor, uygulamada {} seçili. Ayarlardan düzeni değiştirebilirsiniz.",
                        self.layout.name()
                    ));
                self.banner.set_button_label(Some("Kapat"));
                self.banner.set_revealed(true);
            }
        }
        self.refresh();
        // FOG-1 budget is one frame (16 ms) from key press to redraw.
        tracing::debug!(us = started.elapsed().as_micros() as u64, "tuş işlendi");
        self.check_finished();
    }

    /// Pauses on Esc or when the window loses focus.
    pub fn pause(&self) {
        self.session.borrow_mut().pause(self.now());
        self.refresh();
    }

    fn toggle_pause(&self) {
        let paused = self.session.borrow().state() == SessionState::Paused;
        if paused {
            self.session.borrow_mut().resume(self.now());
            self.refresh();
        } else {
            self.pause();
        }
    }

    fn check_finished(&self) {
        let summary = self.session.borrow().result();
        if let Some(summary) = summary
            && !self.finished.replace(true)
            && let Some(f) = self.on_finish.borrow().as_ref()
        {
            f(summary);
        }
    }

    fn refresh(&self) {
        let s = self.session.borrow();
        self.text.set_markup(&self.markup(&s));
        let next = s.next_char();
        let stroke = next.and_then(|c| self.layout.stroke_for(c).cloned());
        self.keyboard.set_target(stroke.clone());
        self.hands.set_target(stroke.clone());
        let hint = match (next, &stroke, s.state()) {
            (_, _, SessionState::Paused) => {
                "Duraklatıldı. Devam etmek için bir tuşa basın.".to_owned()
            }
            (Some(c), Some(st), _) => {
                let shown = if c == ' ' {
                    "boşluk".to_owned()
                } else {
                    format!("“{c}”")
                };
                let mut h = format!("{shown}: {}", st.finger.label());
                if let Some((key, f)) = &st.modifier {
                    let name = if key.as_str() == keyquest_core::layout::ALTGR {
                        "AltGr"
                    } else {
                        "Shift"
                    };
                    h.push_str(&format!(" + {name}: {}", f.label()));
                }
                h
            }
            (Some(c), None, _) => format!("“{c}” bu düzende yok"),
            (None, _, _) => String::new(),
        };
        self.hint.set_label(&hint);
        drop(s);
        self.refresh_status();
    }

    fn refresh_status(&self) {
        let s = self.session.borrow();
        let live = s.summary_at(self.now());
        let label = if s.state() == SessionState::Ready {
            "Başlamak için yazmaya başlayın".to_owned()
        } else {
            format!(
                "{:.0} WPM · %{:.0} doğruluk · {:.0} sn",
                live.wpm_net,
                live.accuracy * 100.0,
                live.duration.as_secs_f64()
            )
        };
        self.status.set_label(&label);
    }

    fn markup(&self, s: &Session) -> String {
        let dark = adw::StyleManager::default().is_dark();
        let (ok, bad) = if dark {
            ("#8ff0a4", "#ff7b63")
        } else {
            ("#1a7f45", "#c01c28")
        };
        let size = (20.0 * self.font_scale * gtk::pango::SCALE as f64) as i32;
        let mut m = format!("<span font_family=\"monospace\" size=\"{size}\">");
        let cursor = (s.next_char().is_some()).then_some(s.cursor());
        for (i, (&ch, status)) in s.target().iter().zip(s.statuses()).enumerate() {
            let shown = match (ch, status) {
                (' ', CharStatus::Wrong(_)) => '·',
                _ => ch,
            };
            let esc = glib::markup_escape_text(&shown.to_string());
            let (fg, underline) = match status {
                CharStatus::Correct => (format!("foreground=\"{ok}\""), "none"),
                CharStatus::Wrong(_) => (format!("foreground=\"{bad}\""), "error"),
                CharStatus::Pending => ("fgalpha=\"60%\"".to_owned(), "none"),
            };
            let attrs = if cursor == Some(i) {
                let fg = if matches!(status, CharStatus::Pending) {
                    ""
                } else {
                    &fg
                };
                let underline = if underline == "none" {
                    "double"
                } else {
                    underline
                };
                format!("{fg} underline=\"{underline}\" background=\"#3584e4\" bgalpha=\"25%\"")
            } else {
                format!("{fg} underline=\"{underline}\"")
            };
            m.push_str(&format!("<span {attrs}>{esc}</span>"));
        }
        m.push_str("</span>");
        m
    }
}
