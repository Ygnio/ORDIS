-- Additive classification: keep the original kind CHECK and FTS5 storage.
CREATE TABLE learning_policy(id INTEGER PRIMARY KEY CHECK(id=1), mode TEXT NOT NULL, categories TEXT NOT NULL);
INSERT INTO learning_policy VALUES(1,'manual','[]');
CREATE TABLE memory_labels(memory_id INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE, category TEXT NOT NULL, form TEXT NOT NULL, tags TEXT NOT NULL, expires_at INTEGER, policy TEXT NOT NULL DEFAULT 'human');
CREATE TABLE memory_versions(id INTEGER PRIMARY KEY, memory_id INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE, content TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, created_at INTEGER NOT NULL);
INSERT INTO memory_versions(memory_id,content,kind,state,created_at) SELECT id,content,kind,state,updated_at FROM memories;
CREATE TRIGGER memory_version_insert AFTER INSERT ON memories BEGIN INSERT INTO memory_versions(memory_id,content,kind,state,created_at) VALUES(new.id,new.content,new.kind,new.state,new.updated_at); END;
CREATE TRIGGER memory_version_update AFTER UPDATE OF content,kind,state ON memories WHEN old.content<>new.content OR old.kind<>new.kind OR old.state<>new.state BEGIN INSERT INTO memory_versions(memory_id,content,kind,state,created_at) VALUES(new.id,new.content,new.kind,new.state,new.updated_at); END;
CREATE TABLE memory_requests(id INTEGER PRIMARY KEY, operation TEXT NOT NULL, target INTEGER REFERENCES memories(id) ON DELETE CASCADE, candidate INTEGER REFERENCES memories(id) ON DELETE CASCADE, expected TEXT NOT NULL, reason TEXT NOT NULL, conversation TEXT NOT NULL, state TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE memory_origin_blocks(conversation TEXT NOT NULL, ordinal INTEGER NOT NULL, project_id INTEGER, created_at INTEGER NOT NULL, PRIMARY KEY(conversation,ordinal));
CREATE TABLE memory_key_blocks(fact_key TEXT NOT NULL, project_id INTEGER, created_at INTEGER NOT NULL);
CREATE TABLE memory_evidence(id INTEGER PRIMARY KEY, conversation TEXT NOT NULL, ordinal INTEGER NOT NULL, tool TEXT NOT NULL, outcome TEXT NOT NULL, content TEXT NOT NULL, created_at INTEGER NOT NULL, UNIQUE(conversation,ordinal));
CREATE TABLE learning_audit(id INTEGER PRIMARY KEY, memory_id INTEGER, action TEXT NOT NULL, policy TEXT NOT NULL, created_at INTEGER NOT NULL);
