//! Terminal front end for the KeyQuest core. It covers phase 1 of the
//! blueprint (practice from the command line with WPM/accuracy) and doubles
//! as a tool for checking user-written layout and lesson files.

mod practice;

use std::process::ExitCode;

use keyquest_core::generator::{self, LessonGenerator};
use keyquest_core::store::{finger_stats, lesson_statuses, next_lesson, now_unix, open_lessons};
use keyquest_core::{ErrorMode, Lesson, ProgressStore, SessionResult};
use keyquest_data::{
    Config, ConfigStore, LayoutRepository, LessonRepository, Paths, SqliteProgressStore,
};

const USAGE: &str = "\
Kullanım: keyquest-cli [SEÇENEKLER] <KOMUT>

Komutlar:
  practice [DERS_ID]   Alıştırma yap (ders verilmezse sıradaki açık ders)
      --weak             Zayıf tuş alıştırması
      --text METİN       Verilen metinle alıştırma
      --length N         Üretilecek metin uzunluğu (karakter)
      --seed N           Tekrarlanabilir metin için tohum
      --continue         Hatada devam et (varsayılan: ayardaki davranış)
      --no-save          Sonucu kaydetme
  lessons              Dersleri ve durumlarını listele
  layouts              Klavye düzenlerini listele
  stroke METİN         Her karakter için tuş, parmak ve değiştiriciyi göster
  stats [--days N]     Gelişim, en zayıf tuşlar ve parmaklar
  check                Düzen ve ders dosyalarını doğrula
  reset --yes          Tüm ilerlemeyi sil

Seçenekler:
  --layout ID          Klavye düzeni (ör. tr-q, tr-f, us)
  --home DİZİN         Tüm dosyaları bu dizinde tut (ayarlar, ilerleme, kullanıcı dosyaları)
  -h, --help           Bu yardımı göster
";

struct Args {
    rest: Vec<String>,
}

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        match self.rest.iter().position(|a| a == name) {
            Some(i) => {
                self.rest.remove(i);
                true
            }
            None => false,
        }
    }

    fn value(&mut self, name: &str) -> Result<Option<String>, String> {
        match self.rest.iter().position(|a| a == name) {
            Some(i) if i + 1 < self.rest.len() => {
                self.rest.remove(i);
                Ok(Some(self.rest.remove(i)))
            }
            Some(_) => Err(format!("{name} bir değer bekliyor")),
            None => Ok(None),
        }
    }

    fn number<T: std::str::FromStr>(&mut self, name: &str) -> Result<Option<T>, String> {
        self.value(name)?
            .map(|v| {
                v.parse()
                    .map_err(|_| format!("{name}: geçersiz sayı {v:?}"))
            })
            .transpose()
    }

    fn positional(&mut self) -> Option<String> {
        let i = self.rest.iter().position(|a| !a.starts_with("--"))?;
        Some(self.rest.remove(i))
    }

    fn finish(self) -> Result<(), String> {
        match self.rest.first() {
            Some(a) => Err(format!("bilinmeyen argüman: {a}")),
            None => Ok(()),
        }
    }
}

struct App {
    paths: Paths,
    config: Config,
    layouts: LayoutRepository,
    lessons: LessonRepository,
}

impl App {
    fn load(home: Option<String>, layout: Option<String>, warn: bool) -> Result<App, String> {
        let paths = match home {
            Some(dir) => Paths::in_dir(dir.as_ref()),
            None => Paths::from_env().map_err(|e| e.to_string())?,
        };
        // Defaults → config.toml → command line.
        let mut config = ConfigStore::new(&paths.config_file)
            .load()
            .map_err(|e| e.to_string())?;
        if let Some(l) = layout {
            config.layout = l;
        }
        let layouts = LayoutRepository::load(Some(&paths.user_layouts()));
        let lessons = LessonRepository::load(Some(&paths.user_lessons()));
        let warnings = layouts.warnings().iter().chain(lessons.warnings());
        for w in warnings.filter(|_| warn) {
            eprintln!("uyarı: {}: {} (atlandı)", w.source, w.message);
        }
        Ok(App {
            paths,
            config,
            layouts,
            lessons,
        })
    }

    fn layout(&self) -> Result<keyquest_core::Layout, String> {
        self.layouts
            .layout(&self.config.layout, &self.config.layout_options())
            .map_err(|e| e.to_string())
    }

    fn store(&self) -> Result<SqliteProgressStore, String> {
        SqliteProgressStore::open(&self.paths.progress_db()).map_err(|e| e.to_string())
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hata: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = Args {
        rest: std::env::args().skip(1).collect(),
    };
    if args.rest.is_empty() || args.flag("-h") || args.flag("--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let home = args.value("--home")?;
    let layout = args.value("--layout")?;
    let command = args.positional().ok_or("komut eksik (--help)")?;
    let app = App::load(home, layout, command != "check")?;
    match command.as_str() {
        "practice" => cmd_practice(&app, args),
        "lessons" => args.finish().and_then(|_| cmd_lessons(&app)),
        "layouts" => args.finish().map(|_| cmd_layouts(&app)),
        "stroke" => {
            let text = args.positional().ok_or("stroke bir metin bekliyor")?;
            args.finish()?;
            cmd_stroke(&app, &text)
        }
        "stats" => {
            let days = args.number("--days")?.unwrap_or(30);
            args.finish()?;
            cmd_stats(&app, days)
        }
        "check" => args.finish().and_then(|_| cmd_check(&app)),
        "reset" => {
            if !args.flag("--yes") {
                return Err("ilerlemeyi silmek için --yes ekleyin".into());
            }
            args.finish()?;
            app.store()?.reset().map_err(|e| e.to_string())?;
            println!("İlerleme silindi.");
            Ok(())
        }
        other => Err(format!("bilinmeyen komut: {other} (--help)")),
    }
}

fn cmd_practice(app: &App, mut args: Args) -> Result<(), String> {
    let weak = args.flag("--weak");
    let text = args.value("--text")?;
    let length: Option<usize> = args.number("--length")?;
    let seed: Option<u64> = args.number("--seed")?;
    let mode = if args.flag("--continue") {
        ErrorMode::Continue
    } else {
        app.config.error_mode
    };
    let no_save = args.flag("--no-save");
    let lesson_id = args.positional();
    args.finish()?;

    let layout = app.layout()?;
    let mut store = app.store()?;
    let lessons = app.lessons.for_layout(layout.id());
    let statuses = lesson_statuses(&lessons, &store).map_err(|e| e.to_string())?;
    let no_lessons = || format!("{} düzeni için ders yok", layout.id());

    let weak_lesson;
    let lesson: Option<&Lesson> = if text.is_some() {
        None
    } else if weak {
        let open = open_lessons(&statuses);
        if open.is_empty() {
            return Err(no_lessons());
        }
        weak_lesson = generator::combined_lesson(layout.id(), &open);
        Some(&weak_lesson)
    } else if let Some(id) = &lesson_id {
        let status = statuses
            .iter()
            .find(|s| &s.lesson.id == id)
            .ok_or_else(|| format!("{} düzeninde {id:?} dersi yok", layout.id()))?;
        if !status.unlocked {
            eprintln!("not: bu ders henüz açılmadı; yine de alıştırma yapabilirsiniz.");
        }
        Some(status.lesson)
    } else {
        Some(next_lesson(&statuses).ok_or_else(no_lessons)?)
    };

    let (title, target) = match (&text, lesson) {
        (Some(t), _) => ("Serbest metin".to_owned(), t.trim().to_owned()),
        (None, Some(l)) => {
            let weights = if weak {
                let stats = store.key_stats(layout.id()).map_err(|e| e.to_string())?;
                Some(generator::weights_from_stats(&stats, 5))
            } else {
                None
            };
            let mut generator = match seed {
                Some(s) => LessonGenerator::new(s),
                None => LessonGenerator::from_time(),
            };
            let target =
                generator.generate(l, &layout, weights.as_ref(), length.unwrap_or(l.length()));
            (l.title.clone(), target)
        }
        (None, None) => unreachable!(),
    };
    if target.is_empty() {
        return Err("alıştırma metni boş".into());
    }
    for ch in target.chars().filter(|c| !layout.contains(*c)) {
        eprintln!(
            "uyarı: {ch:?} {} düzeninde yok; vurgu gösterilmeyecek",
            layout.id()
        );
    }

    let started_at = now_unix();
    let Some(summary) = practice::run(&title, &target, mode, &layout)? else {
        println!("Alıştırma iptal edildi.");
        return Ok(());
    };

    println!("\n{title}");
    println!(
        "  Net hız   : {:.1} WPM (brüt {:.1})",
        summary.wpm_net, summary.wpm_gross
    );
    println!("  Doğruluk  : %{:.1}", summary.accuracy * 100.0);
    println!("  Süre      : {:.1} sn", summary.duration.as_secs_f64());
    println!(
        "  Vuruş     : {} ({} hatalı, {} düzeltilmedi)",
        summary.chars_total, summary.errors, summary.uncorrected_errors
    );
    let missed = summary.missed_keys();
    if !missed.is_empty() {
        let list: Vec<String> = missed
            .iter()
            .take(8)
            .map(|k| format!("{}×{}", show(k.ch), k.misses))
            .collect();
        println!("  Hatalı tuşlar: {}", list.join("  "));
    }

    let (lesson_id, completed) = match (lesson, text.is_some(), weak) {
        (Some(l), false, false) => (l.id.clone(), l.passed(&summary)),
        (Some(l), _, true) => (l.id.clone(), false),
        _ => (format!("{}/serbest", layout.id()), false),
    };
    if let (Some(l), false, false) = (lesson, text.is_some(), weak) {
        if completed {
            println!("  Hedef aşıldı ✔");
        } else {
            println!(
                "  Hedef: {} WPM, %{:.0} doğruluk",
                l.target_wpm,
                l.target_accuracy * 100.0
            );
        }
    }
    if no_save {
        return Ok(());
    }
    let result = SessionResult {
        lesson_id,
        layout: layout.id().to_owned(),
        started_at,
        completed,
        summary,
    };
    if let Err(e) = store.save_session(&result) {
        eprintln!("uyarı: ilerleme kaydedilemedi: {e}");
    }
    Ok(())
}

fn show(ch: char) -> String {
    if ch == ' ' {
        "␣".into()
    } else {
        ch.to_string()
    }
}

fn cmd_lessons(app: &App) -> Result<(), String> {
    let store = app.store()?;
    let lessons = app.lessons.for_layout(&app.config.layout);
    if lessons.is_empty() {
        println!("{} düzeni için ders yok.", app.config.layout);
    }
    for st in lesson_statuses(&lessons, &store).map_err(|e| e.to_string())? {
        let mark = match (st.completed(), st.unlocked) {
            (true, _) => "✔",
            (_, true) => "·",
            (_, false) => "🔒",
        };
        let best = st
            .progress
            .map(|p| format!("  en iyi {:.0} WPM, %{:.0}", p.best_wpm, p.best_acc * 100.0))
            .unwrap_or_default();
        let l = st.lesson;
        println!("{mark} {:<28} {}{best}", l.id, l.title);
    }
    Ok(())
}

fn cmd_layouts(app: &App) {
    for d in app.layouts.defs() {
        let current = if d.id == app.config.layout { "*" } else { " " };
        println!("{current} {:<8} {}", d.id, d.name);
    }
}

fn cmd_stroke(app: &App, text: &str) -> Result<(), String> {
    let layout = app.layout()?;
    for ch in text.chars() {
        match layout.stroke_for(ch) {
            Some(s) => {
                let modifier = s
                    .modifier
                    .as_ref()
                    .map(|(k, f)| format!("  + {k} ({})", f.label()))
                    .unwrap_or_default();
                println!(
                    "{}  {:<16} {} ({}){modifier}",
                    show(ch),
                    s.key.as_str(),
                    s.finger,
                    s.finger.label()
                );
            }
            None => println!("{}  {} düzeninde yok", show(ch), layout.id()),
        }
    }
    Ok(())
}

fn cmd_stats(app: &App, days: u32) -> Result<(), String> {
    let layout = app.layout()?;
    let store = app.store()?;
    let e = |e: keyquest_core::StoreError| e.to_string();
    let history = store.history(layout.id(), days).map_err(e)?;
    println!(
        "Son {days} gün ({}): {} oturum",
        layout.name(),
        history.len()
    );
    for s in history.iter().rev().take(15).rev() {
        println!(
            "  #{:<4} {:<28} {:>5.1} WPM  %{:>5.1}",
            s.id,
            s.lesson_id,
            s.wpm_net,
            s.accuracy * 100.0
        );
    }
    let weak = store.weakest_keys(layout.id(), 10).map_err(e)?;
    if !weak.is_empty() {
        println!("En zayıf tuşlar:");
        for k in weak {
            let avg = k
                .avg_ms
                .map(|ms| format!(", ort. {ms} ms"))
                .unwrap_or_default();
            println!(
                "  {}  %{:>4.1} hata ({}/{}{avg})",
                show(k.ch),
                k.misses as f64 / k.attempts() as f64 * 100.0,
                k.misses,
                k.attempts()
            );
        }
    }
    let stats = store.key_stats(layout.id()).map_err(e)?;
    let fingers = finger_stats(&layout, &stats);
    if !fingers.is_empty() {
        println!("Parmaklar (en zayıftan):");
        for f in fingers {
            println!(
                "  {} {:<14} %{:>4.1} hata ({} vuruş)",
                f.finger,
                f.finger.label(),
                f.error_rate() * 100.0,
                f.hits + f.misses
            );
        }
    }
    Ok(())
}

fn cmd_check(app: &App) -> Result<(), String> {
    let mut problems = 0;
    let warnings = app.layouts.warnings().iter().chain(app.lessons.warnings());
    for w in warnings {
        println!("✗ {}: {}", w.source, w.message);
        problems += 1;
    }
    for l in app.lessons.all() {
        match app.layouts.layout(&l.layout, &Default::default()) {
            Ok(layout) => {
                for p in l.check_against(&layout) {
                    println!("✗ {}: {p}", l.id);
                    problems += 1;
                }
            }
            Err(e) => {
                println!("✗ {}: {e}", l.id);
                problems += 1;
            }
        }
    }
    let layouts = app.layouts.defs().count();
    let lessons = app.lessons.all().count();
    println!("{layouts} düzen, {lessons} ders denetlendi; {problems} sorun.");
    if problems > 0 {
        Err("doğrulama başarısız".into())
    } else {
        Ok(())
    }
}
