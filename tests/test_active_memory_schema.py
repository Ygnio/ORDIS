"""Synthetic additive migration checks. No user profiles or databases are read."""
from pathlib import Path
import sqlite3
import unittest

ROOT = Path(__file__).resolve().parents[1]

class ActiveSchemaTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(':memory:')
        self.db.execute('pragma foreign_keys=on')
        self.db.execute('pragma trusted_schema=off')
        for p in sorted((ROOT/'app/src/migrations').glob('*.sql')):
            self.db.executescript(p.read_text(encoding='utf-8'))
    def tearDown(self):
        self.db.close()
    def test_original_kinds_and_manual_default(self):
        self.assertEqual(self.db.execute('select mode,categories from learning_policy').fetchone(),('manual','[]'))
        self.assertIn("'hardware'",self.db.execute("select sql from sqlite_schema where name='memories'").fetchone()[0])
    def test_versions_cascade_without_leaving_deleted_text(self):
        self.db.execute("insert into memories values(1,'fact','Synthetic fact',null,'','candidate',3,'manual','synthetic',1,1)")
        self.db.execute("update memories set content='Synthetic updated',state='active' where id=1")
        self.assertEqual(self.db.execute('select count(*) from memory_versions').fetchone()[0],2)
        self.db.execute('delete from memories where id=1')
        self.assertEqual(self.db.execute('select count(*) from memory_versions').fetchone()[0],0)
    def test_migration_keeps_existing_rows(self):
        self.assertEqual(self.db.execute('pragma integrity_check').fetchone()[0],'ok')
        self.assertEqual(self.db.execute('pragma foreign_key_check').fetchall(),[])
