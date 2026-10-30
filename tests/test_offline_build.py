"""The preflight must fail before any process (including an installer) can run."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('offline_build', Path(__file__).resolve().parents[1] / 'scripts/offline_build.py')
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class OfflineBuildTests(unittest.TestCase):
    def test_missing_tool_never_launches_or_creates_cache(self):
        with tempfile.TemporaryDirectory(prefix='ordis-offline-') as tmp:
            output = Path(tmp) / 'build'
            with patch.object(build.subprocess, 'run', side_effect=AssertionError('process launched')), \
                 patch.object(build.subprocess, 'check_output', side_effect=AssertionError('process launched')):
                with self.assertRaisesRegex(ValueError, 'Missing explicit native tool'):
                    build.prepare({'cargo': str(Path(tmp) / 'absent.exe')}, output)
            self.assertFalse(output.exists())

    def test_proxy_is_rejected_without_execution(self):
        with tempfile.TemporaryDirectory(prefix='ordis-offline-') as tmp:
            root = Path(tmp)
            (root / 'cargo.exe').write_bytes(b'proxy fixture')
            (root / 'rustup.exe').write_bytes(b'proxy fixture')
            with self.assertRaisesRegex(ValueError, 'Rustup proxy forbidden'):
                build.prepare({'cargo': str(root / 'cargo.exe')}, root / 'output')


if __name__ == '__main__':
    unittest.main()
