"""Synthetic migrations and index invalidation; no private data or models."""
from pathlib import Path
import sqlite3
import unittest

class SemanticSchema(unittest.TestCase):
    def setUp(self):
        self.db=sqlite3.connect(':memory:')
        self.db.execute('pragma foreign_keys=on')
        self.db.execute('pragma trusted_schema=off')
        for p in sorted((Path(__file__).resolve().parents[1]/'app/src/migrations').glob('*.sql')):
            self.db.executescript(p.read_text(encoding='utf-8'))
    def tearDown(self): self.db.close()
    def memory(self,state='active'):
        self.db.execute("insert into memories values(1,'preference','Synthetic RPG preference',null,'',?,3,'manual','synthetic',1,1)",(state,))
    def ready(self):
        revision=self.db.execute('select revision from semantic_queue where memory_id=1').fetchone()[0]
        self.db.execute("insert into semantic_vectors values(1,?,'synthetic',2,1,'[1,0]')",(revision,))
        self.db.execute("update semantic_queue set state='ready' where memory_id=1")
    def test_only_approved_indexed(self):
        self.memory('candidate')
        self.assertEqual(self.db.execute('select count(*) from semantic_queue').fetchone()[0],0)
        self.db.execute("update memories set state='active' where id=1")
        self.assertEqual(self.db.execute('select state from semantic_queue').fetchone()[0],'pending')
    def test_update_invalidates_immediately(self):
        self.memory();self.ready()
        old=self.db.execute('select revision from semantic_queue').fetchone()[0]
        self.db.execute("update memories set content='Synthetic changed' where id=1")
        self.assertEqual(self.db.execute('select count(*) from semantic_vectors').fetchone()[0],0)
        self.assertGreater(self.db.execute('select revision from semantic_queue').fetchone()[0],old)
    def test_forget_cascades_queue_and_vector(self):
        self.memory();self.ready();self.db.execute('delete from memories where id=1')
        for table in ['semantic_queue','semantic_vectors']:
            self.assertEqual(self.db.execute('select count(*) from '+table).fetchone()[0],0)
    def test_reclassify_invalidates(self):
        self.memory();self.ready()
        self.db.execute("insert into memory_labels values(1,'preferences','semantic','[]',null,'human')")
        self.assertEqual(self.db.execute('select count(*) from semantic_vectors').fetchone()[0],0)
    def test_obsolete_excluded(self):
        self.memory();self.ready();self.db.execute("update memories set state='obsolete' where id=1")
        self.db.execute("insert into memory_labels values(1,'preferences','semantic','[]',null,'human')")
        self.assertEqual(self.db.execute('select state from semantic_queue').fetchone()[0],'excluded')
        self.assertEqual(self.db.execute('select count(*) from semantic_vectors').fetchone()[0],0)
    def test_foreign_key_integrity(self):
        self.assertEqual(self.db.execute('pragma foreign_key_check').fetchall(),[])

if __name__=='__main__':unittest.main()
