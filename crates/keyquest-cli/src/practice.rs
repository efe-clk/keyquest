//! Interactive practice screen in the terminal (raw mode).

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{cursor, execute, queue};
use keyquest_core::{CharStatus, ErrorMode, Layout, Session, SessionState, Summary};

/// Leaves raw mode even if drawing fails or the program panics.
struct RawMode;

impl RawMode {
    fn enter() -> io::Result<RawMode> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), cursor::Hide)?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

/// Runs a session; `None` if the user cancelled it.
pub fn run(
    title: &str,
    target: &str,
    mode: ErrorMode,
    layout: &Layout,
) -> Result<Option<Summary>, String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("alıştırma için etkileşimli bir terminal gerekli".into());
    }
    let mut session = Session::new(target, mode);
    let clock = Instant::now();
    let _raw = RawMode::enter().map_err(|e| e.to_string())?;
    let io_err = |e: io::Error| e.to_string();

    loop {
        let now = clock.elapsed();
        draw(title, &session, layout, now).map_err(io_err)?;
        if session.is_finished() || session.state() == SessionState::Cancelled {
            break;
        }
        // Wake up regularly so the live WPM keeps updating.
        if !event::poll(Duration::from_millis(250)).map_err(io_err)? {
            continue;
        }
        let Event::Key(key) = event::read().map_err(io_err)? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        let now = clock.elapsed();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        match (session.state(), key.code) {
            (_, KeyCode::Char('c')) if ctrl => session.cancel(),
            (SessionState::Paused, KeyCode::Char('q')) => session.cancel(),
            (SessionState::Paused, _) => session.resume(now),
            (SessionState::Ready, KeyCode::Esc) => session.cancel(),
            (_, KeyCode::Esc) => session.pause(now),
            (_, KeyCode::Backspace) => {
                session.backspace();
            }
            // Ctrl shortcuts are not text; AltGr arrives as Ctrl+Alt and is.
            (_, KeyCode::Char(ch)) if !ctrl => {
                session.input(ch, now);
            }
            // Arrows, function keys, Enter...: not part of the text.
            _ => {}
        }
    }
    let result = session.result();
    drop(_raw);
    println!();
    Ok(result)
}

fn draw(title: &str, s: &Session, layout: &Layout, now: Duration) -> io::Result<()> {
    let mut out = io::stdout();
    queue!(
        out,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All),
        SetAttribute(Attribute::Bold),
        Print(title),
        SetAttribute(Attribute::Reset),
        Print("\r\n\r\n"),
    )?;

    for (i, (&ch, status)) in s.target().iter().zip(s.statuses()).enumerate() {
        let shown = match (ch, status) {
            (_, CharStatus::Wrong(' ')) | (' ', CharStatus::Wrong(_)) => '·',
            _ => ch,
        };
        match status {
            CharStatus::Correct => queue!(out, SetForegroundColor(Color::Green))?,
            CharStatus::Wrong(_) => queue!(out, SetForegroundColor(Color::Red))?,
            CharStatus::Pending => queue!(out, SetForegroundColor(Color::DarkGrey))?,
        }
        if i == s.cursor() && s.next_char().is_some() {
            queue!(out, SetAttribute(Attribute::Reverse))?;
        }
        queue!(
            out,
            Print(shown),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
    }
    queue!(out, Print("\r\n\r\n"))?;

    if let Some(next) = s.next_char() {
        let shown = if next == ' ' {
            "boşluk".to_owned()
        } else {
            format!("'{next}'")
        };
        queue!(out, Print(format!("Sıradaki: {shown}  ")))?;
        if let Some(stroke) = layout.stroke_for(next) {
            queue!(
                out,
                SetForegroundColor(Color::Cyan),
                Print(format!("{} → {}", stroke.key, stroke.finger.label())),
                ResetColor,
            )?;
            if let Some((key, finger)) = &stroke.modifier {
                queue!(
                    out,
                    SetForegroundColor(Color::Yellow),
                    Print(format!("  + {key} → {}", finger.label())),
                    ResetColor,
                )?;
            }
        }
        queue!(out, Print("\r\n"))?;
    }

    let live = s.summary_at(now);
    queue!(
        out,
        Print(format!(
            "{:.0} WPM · %{:.0} doğruluk · {:.0} sn\r\n\r\n",
            live.wpm_net,
            live.accuracy * 100.0,
            live.duration.as_secs_f64()
        )),
        SetForegroundColor(Color::DarkGrey),
    )?;
    let help = match s.state() {
        SessionState::Paused => "Duraklatıldı — devam etmek için bir tuşa, çıkmak için q'ya basın",
        SessionState::Ready => "Başlamak için yazmaya başlayın · Esc: çık",
        _ => "Esc: duraklat · Ctrl+C: iptal",
    };
    queue!(out, Print(help), ResetColor)?;
    out.flush()
}
