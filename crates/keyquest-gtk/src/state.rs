//! Application state shared by the screens: settings, data repositories, the
//! active layout and the progress store.

use std::rc::Rc;

use keyquest_core::{Layout, MemoryProgressStore, ProgressStore};
use keyquest_data::{
    Config, ConfigStore, LayoutRepository, LessonRepository, Paths, SqliteProgressStore,
};

pub struct State {
    pub config_store: ConfigStore,
    pub config: Config,
    pub layouts: LayoutRepository,
    pub lessons: LessonRepository,
    pub layout: Rc<Layout>,
    /// Every known layout, for spotting a different system layout (R-1).
    pub all_layouts: Rc<Vec<Layout>>,
    pub store: Box<dyn ProgressStore>,
    /// Problems found while starting up, shown once as toasts.
    pub warnings: Vec<String>,
}

impl State {
    /// Never fails: anything that cannot be loaded falls back to a default
    /// and is reported in `warnings`, so the app always starts.
    pub fn load() -> State {
        let mut warnings = Vec::new();
        let paths = Paths::from_env().unwrap_or_else(|e| {
            warnings.push(format!("Veri dizini bulunamadı: {e}"));
            Paths::in_dir(&std::env::temp_dir().join("keyquest"))
        });
        let config_store = ConfigStore::new(&paths.config_file);
        let config = config_store.load().unwrap_or_else(|e| {
            warnings.push(format!(
                "Ayarlar okunamadı, varsayılanlar kullanılıyor: {e}"
            ));
            Config::default()
        });
        let layouts = LayoutRepository::load(Some(&paths.user_layouts()));
        let lessons = LessonRepository::load(Some(&paths.user_lessons()));
        for w in layouts.warnings().iter().chain(lessons.warnings()) {
            warnings.push(format!("{} atlandı: {}", w.source, w.message));
        }
        let store: Box<dyn ProgressStore> = match SqliteProgressStore::open(&paths.progress_db()) {
            Ok(s) => Box::new(s),
            Err(e) => {
                warnings.push(format!(
                    "İlerleme kaydı açılamadı, bu oturum kaydedilmeyecek: {e}"
                ));
                Box::new(MemoryProgressStore::new())
            }
        };
        let mut state = State {
            config_store,
            config,
            layouts,
            lessons,
            layout: Rc::new(placeholder_layout()),
            all_layouts: Rc::new(Vec::new()),
            store,
            warnings,
        };
        state.rebuild_layouts();
        state
    }

    /// Rebuilds the layouts after the layout or space-thumb setting changed.
    fn rebuild_layouts(&mut self) {
        let opts = self.config.layout_options();
        let layout = match self.layouts.layout(&self.config.layout, &opts) {
            Ok(l) => l,
            Err(e) => {
                let fallback = Config::default().layout;
                self.warnings
                    .push(format!("{e}; {fallback} düzeni kullanılıyor"));
                self.config.layout = fallback;
                self.layouts
                    .layout(&self.config.layout, &opts)
                    .expect("embedded default layout is valid")
            }
        };
        self.layout = Rc::new(layout);
        self.all_layouts = Rc::new(
            self.layouts
                .defs()
                .filter_map(|d| self.layouts.layout(&d.id, &opts).ok())
                .collect(),
        );
    }

    /// Applies and saves new settings.
    pub fn set_config(&mut self, config: Config) -> Result<(), String> {
        let rebuild =
            config.layout != self.config.layout || config.space_thumb != self.config.space_thumb;
        self.config = config;
        if rebuild {
            self.rebuild_layouts();
        }
        self.config_store
            .save(&self.config)
            .map_err(|e| format!("Ayarlar kaydedilemedi: {e}"))
    }
}

fn placeholder_layout() -> Layout {
    LayoutRepository::embedded()
        .layout(&Config::default().layout, &Default::default())
        .expect("embedded default layout is valid")
}
