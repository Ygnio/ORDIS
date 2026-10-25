CREATE TABLE semantic_settings(id INTEGER PRIMARY KEY CHECK(id=1),config TEXT NOT NULL);
CREATE TABLE visual_sources(conversation TEXT NOT NULL,ordinal INTEGER NOT NULL,PRIMARY KEY(conversation,ordinal),FOREIGN KEY(conversation,ordinal) REFERENCES messages(conversation_id,ordinal) ON DELETE CASCADE);
CREATE TABLE semantic_vectors(
 memory_id INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL, model_id TEXT NOT NULL, dimension INTEGER NOT NULL,
 generated_at INTEGER NOT NULL, vector TEXT NOT NULL
);
CREATE TABLE semantic_queue(
 memory_id INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL DEFAULT 1, state TEXT NOT NULL DEFAULT 'pending',
 attempts INTEGER NOT NULL DEFAULT 0, error TEXT, retry_at INTEGER NOT NULL DEFAULT 0
);
INSERT INTO semantic_queue(memory_id) SELECT id FROM memories WHERE state='active';
CREATE TRIGGER semantic_insert AFTER INSERT ON memories WHEN NEW.state='active' BEGIN
 INSERT INTO semantic_queue(memory_id) VALUES(NEW.id);
END;
CREATE TRIGGER semantic_update AFTER UPDATE OF content,kind,state,project_id,importance ON memories BEGIN
 DELETE FROM semantic_vectors WHERE memory_id=NEW.id;
 INSERT INTO semantic_queue(memory_id,state) VALUES(NEW.id,CASE WHEN NEW.state='active' THEN 'pending' ELSE 'excluded' END)
 ON CONFLICT(memory_id) DO UPDATE SET revision=revision+1,state=excluded.state,attempts=0,error=NULL,retry_at=0;
END;
CREATE TRIGGER semantic_labels_insert AFTER INSERT ON memory_labels BEGIN
 DELETE FROM semantic_vectors WHERE memory_id=NEW.memory_id;
 UPDATE semantic_queue SET revision=revision+1,state='pending',attempts=0,error=NULL,retry_at=0 WHERE memory_id=NEW.memory_id;
END;
CREATE TRIGGER semantic_labels_update AFTER UPDATE ON memory_labels BEGIN
 DELETE FROM semantic_vectors WHERE memory_id=NEW.memory_id;
 UPDATE semantic_queue SET revision=revision+1,state='pending',attempts=0,error=NULL,retry_at=0 WHERE memory_id=NEW.memory_id;
END;
