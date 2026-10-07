//! Every embedded layout and lesson must load, and every lesson must be
//! typeable on its layout.

use keyquest_core::{Hand, LayoutOptions};
use keyquest_data::{LayoutRepository, LessonRepository};

#[test]
fn lessons_fit_their_layouts() {
    let layouts = LayoutRepository::embedded();
    let lessons = LessonRepository::embedded();
    assert!(lessons.warnings().is_empty(), "{:?}", lessons.warnings());
    let mut problems = Vec::new();
    for lesson in lessons.all() {
        let layout = layouts
            .layout(&lesson.layout, &LayoutOptions::default())
            .unwrap_or_else(|e| panic!("{}: {e}", lesson.id));
        for p in lesson.check_against(&layout) {
            problems.push(format!("{}: {p}", lesson.id));
        }
        if lesson.usable_words().count() < 10 {
            problems.push(format!("{}: too few words", lesson.id));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn turkish_letters_and_shift_rule() {
    let repo = LayoutRepository::embedded();
    for id in ["tr-q", "tr-f"] {
        let l = repo.layout(id, &LayoutOptions::default()).unwrap();
        for ch in "abcçdefgğhıijklmnoöprsştuüvyzABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZ.,".chars()
        {
            let s = l.stroke_for(ch).unwrap_or_else(|| panic!("{id}: {ch:?}"));
            if ch.is_uppercase() {
                let (_, shift_finger) = s.modifier.clone().expect("capital needs Shift");
                assert_ne!(shift_finger.hand, s.finger.hand, "{id}: {ch:?}");
            }
        }
    }
    let q = repo.layout("tr-q", &LayoutOptions::default()).unwrap();
    assert_eq!(q.stroke_for('İ').unwrap().key.as_str(), "KEY_APOSTROPHE");
    assert_eq!(q.stroke_for('ı').unwrap().key.as_str(), "KEY_I");
    assert_eq!(
        q.stroke_for('@')
            .unwrap()
            .modifier
            .as_ref()
            .unwrap()
            .0
            .as_str(),
        "KEY_RIGHTALT"
    );
    let f = repo
        .layout(
            "tr-f",
            &LayoutOptions {
                space_thumb: Some(Hand::Left),
            },
        )
        .unwrap();
    assert_eq!(f.stroke_for('a').unwrap().key.as_str(), "KEY_F");
    assert_eq!(f.stroke_for(' ').unwrap().finger.to_string(), "L0");
}
