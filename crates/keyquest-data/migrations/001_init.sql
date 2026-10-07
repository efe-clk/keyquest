CREATE TABLE sessions (
    id          INTEGER PRIMARY KEY,
    lesson_id   TEXT    NOT NULL,
    layout      TEXT    NOT NULL,
    started_at  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    chars_total INTEGER NOT NULL,
    errors      INTEGER NOT NULL,
    wpm_net     REAL    NOT NULL,
    accuracy    REAL    NOT NULL
);
CREATE INDEX sessions_layout_started ON sessions (layout, started_at);

CREATE TABLE key_stats (
    session_id INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    ch         TEXT    NOT NULL,
    hits       INTEGER NOT NULL,
    misses     INTEGER NOT NULL,
    avg_ms     INTEGER,
    PRIMARY KEY (session_id, ch)
);
CREATE INDEX key_stats_ch ON key_stats (ch);

CREATE TABLE lesson_progress (
    lesson_id TEXT    PRIMARY KEY,
    best_wpm  REAL    NOT NULL,
    best_acc  REAL    NOT NULL,
    completed INTEGER NOT NULL DEFAULT 0
);
