//! Default layouts and lessons compiled into the binary.

pub const LAYOUTS: &[(&str, &str)] = &[
    (
        "data/layouts/tr-f.toml",
        include_str!("../data/layouts/tr-f.toml"),
    ),
    (
        "data/layouts/tr-q.toml",
        include_str!("../data/layouts/tr-q.toml"),
    ),
    (
        "data/layouts/us.toml",
        include_str!("../data/layouts/us.toml"),
    ),
];

pub const LESSONS: &[(&str, &str)] = &[
    (
        "data/lessons/tr-q/01-ana-sira.toml",
        include_str!("../data/lessons/tr-q/01-ana-sira.toml"),
    ),
    (
        "data/lessons/tr-q/02-e-i.toml",
        include_str!("../data/lessons/tr-q/02-e-i.toml"),
    ),
    (
        "data/lessons/tr-q/03-r-u.toml",
        include_str!("../data/lessons/tr-q/03-r-u.toml"),
    ),
    (
        "data/lessons/tr-q/04-g-h.toml",
        include_str!("../data/lessons/tr-q/04-g-h.toml"),
    ),
    (
        "data/lessons/tr-q/05-i-o-n-t-y.toml",
        include_str!("../data/lessons/tr-q/05-i-o-n-t-y.toml"),
    ),
    (
        "data/lessons/tr-q/06-tum-harfler.toml",
        include_str!("../data/lessons/tr-q/06-tum-harfler.toml"),
    ),
    (
        "data/lessons/tr-q/07-buyuk-harfler.toml",
        include_str!("../data/lessons/tr-q/07-buyuk-harfler.toml"),
    ),
];
