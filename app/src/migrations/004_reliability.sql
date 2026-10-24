ALTER TABLE settings ADD COLUMN auto_consolidation INTEGER NOT NULL DEFAULT 0 CHECK(auto_consolidation IN (0,1));
CREATE TABLE processing(conversation_id TEXT NOT NULL, ordinal INTEGER NOT NULL, fingerprint TEXT NOT NULL, state TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY(conversation_id,ordinal), FOREIGN KEY(conversation_id,ordinal) REFERENCES messages(conversation_id,ordinal) ON DELETE CASCADE);
CREATE TABLE diagnostics(id INTEGER PRIMARY KEY, code TEXT NOT NULL, amount INTEGER NOT NULL, elapsed_ms REAL NOT NULL, created_at INTEGER NOT NULL);
