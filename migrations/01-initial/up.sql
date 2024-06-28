CREATE TABLE hours(
    entry_id INTEGER PRIMARY KEY,
    date TEXT NOT NULL,
    time REAL NOT NULL,
    deleted BOOLEAN NOT NULL CHECK (deleted IN (0, 1))
);
