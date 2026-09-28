CREATE VIRTUAL TABLE thread_search USING fts5(
    title,
    body,
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO thread_search (rowid, title, body)
SELECT id, title, COALESCE(body, '') FROM threads;

CREATE TRIGGER threads_search_insert AFTER INSERT ON threads BEGIN
    INSERT INTO thread_search (rowid, title, body)
    VALUES (NEW.id, NEW.title, COALESCE(NEW.body, ''));
END;

CREATE TRIGGER threads_search_delete AFTER DELETE ON threads BEGIN
    DELETE FROM thread_search WHERE rowid = OLD.id;
END;

CREATE TRIGGER threads_search_update AFTER UPDATE OF title, body ON threads BEGIN
    DELETE FROM thread_search WHERE rowid = OLD.id;
    INSERT INTO thread_search (rowid, title, body)
    VALUES (NEW.id, NEW.title, COALESCE(NEW.body, ''));
END;
