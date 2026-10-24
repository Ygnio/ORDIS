CREATE TABLE identity_profile(id INTEGER PRIMARY KEY CHECK(id=1), name TEXT NOT NULL, description TEXT NOT NULL, personality TEXT NOT NULL, instructions TEXT NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE rejected_memories(fingerprint TEXT NOT NULL, kind TEXT NOT NULL, project_id INTEGER, deleted_at INTEGER NOT NULL);
CREATE UNIQUE INDEX rejected_unique ON rejected_memories(fingerprint,kind,ifnull(project_id,0));
CREATE TABLE maintenance(id INTEGER PRIMARY KEY CHECK(id=1), state TEXT NOT NULL, processed INTEGER NOT NULL, updated_at INTEGER NOT NULL);
INSERT INTO maintenance VALUES(1,'idle',0,0);
CREATE TABLE memory_sequence(id INTEGER PRIMARY KEY CHECK(id=1), next_id INTEGER NOT NULL);
INSERT INTO memory_sequence SELECT 1,coalesce(max(mid),0)+1 FROM (SELECT id AS mid FROM memories UNION ALL SELECT memory_id AS mid FROM change_history);
