//! Every symbolic icon the UI names must be bundled in data/icons/symbolic,
//! otherwise it is missing on desktops without the Adwaita icon theme.

use std::fs;
use std::path::Path;

#[test]
fn symbolic_icons_are_bundled() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bundled = crate_dir.join("../../data/icons/symbolic");
    let mut missing = Vec::new();
    for entry in fs::read_dir(crate_dir.join("src")).unwrap() {
        let source = fs::read_to_string(entry.unwrap().path()).unwrap();
        for part in source.split('"').skip(1).step_by(2) {
            let is_icon = part.ends_with("-symbolic")
                && part.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
            if is_icon && !bundled.join(format!("{part}.svg")).exists() {
                missing.push(part.to_owned());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "copy these icons to data/icons/symbolic: {missing:?}"
    );
}
