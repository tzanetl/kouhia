CREATE TABLE undolog(
    row_id INTEGER PRIMARY KEY,
    entry_id INTEGER NOT NULL REFERENCES hours (entry_id),
    deleted_old BOOLEAN NOT NULL CHECK (deleted_old IN (0, 1)),
    processed BOOLEAN NOT NULL CHECK (processed IN (0, 1))
);

CREATE TRIGGER hours_insert AFTER INSERT ON hours WHEN not_undo()
BEGIN
    INSERT INTO undolog (entry_id, deleted_old, processed) VALUES (new.entry_id, 1, 0);
END;

CREATE TRIGGER hours_delete AFTER UPDATE ON hours WHEN not_undo()
BEGIN
    INSERT INTO undolog (entry_id, deleted_old, processed) VALUES (old.entry_id, new.deleted, 0);
END;
