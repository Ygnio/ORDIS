import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('paired_package', Path(__file__).resolve().parents[1] / 'scripts/package-paired.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackageTests(unittest.TestCase):
    def test_mismatched_gui_cli_are_rejected_before_destination_creation(self):
        with tempfile.TemporaryDirectory(prefix='ordis-pair-synthetic-') as tmp:
            root = Path(tmp)
            for name in ('ordis.exe', 'ordis-cli.exe', 'WebView2Loader.dll'):
                (root / name).write_bytes(b'synthetic fixture')
            cli = {'commit': 'a' * 40, 'dirty': False, 'distribution': 'ordis', 'version': 'synthetic'}
            gui = dict(cli, commit='b' * 40)
            with patch.object(package.subprocess, 'check_output', side_effect=['a' * 40, '', json.dumps(cli), json.dumps(gui)]):
                with self.assertRaisesRegex(ValueError, 'GUI and CLI provenance differ'):
                    package.package(root, root / 'WebView2Loader.dll', root / 'output')
            self.assertFalse((root / 'output').exists())

    def test_dirty_checkout_is_not_packaged(self):
        with tempfile.TemporaryDirectory(prefix='ordis-pair-synthetic-') as tmp:
            root = Path(tmp)
            with patch.object(package.subprocess, 'check_output', side_effect=['a' * 40, ' M synthetic.rs']):
                with self.assertRaisesRegex(ValueError, 'Commit the reviewed source'):
                    package.package(root, root / 'loader.dll', root / 'output')
            self.assertFalse((root / 'output').exists())
