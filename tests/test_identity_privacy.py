import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location('privacy_guard',Path(__file__).resolve().parents[1]/'scripts/privacy_guard.py')
guard=importlib.util.module_from_spec(spec);spec.loader.exec_module(guard)
class PrivacyTests(unittest.TestCase):
    def test_private_paths(self):
        for path in ['runtime/a.json','data/lucille.sqlite3-wal','lucille.sqlite3-shm','identity.json','identity-profile.json','profiles/person.json','AGENTS.local.md','personality.private.md','system-prompt.private.txt','backups/snapshot.zip','technical-reports/report.json','technical-memory-report-123.json']:
            self.assertTrue(guard.forbidden(path),path)
    def test_source_and_synthetic_tests_allowed(self):
        for path in ['app/src/identity.rs','app/src/migrations/003_identity.sql','tests/test_identity_privacy.py','docs/FASE-3-1-IDENTIDADE.md']:
            self.assertFalse(guard.forbidden(path),path)
