"""Synthetic hostile and valid scripts; no project/user data."""
import json
from pathlib import Path
import subprocess
import sys
import unittest
ROOT = Path(__file__).resolve().parents[1]

class RestrictedPython(unittest.TestCase):
    def run_source(self, source, extra=None):
        sources = {'test_calc.py': source, **(extra or {})}
        return subprocess.run([sys.executable, '-I', '-S', '-B', str(ROOT/'app/src/developer_python.py')], input=json.dumps({'entry':'test_calc.py','sources':sources}), text=True, capture_output=True, timeout=10)

    def test_real_unittest_workflow(self):
        source='import unittest\nfrom calc import add\nclass Calc(unittest.TestCase):\n def test_add(self):\n  self.assertEqual(add(2,3),5)\nunittest.main()'
        for body, expected in [('def add(a,b): return a+b',0),('def add(a,b): return a-b',1)]:
            with self.subTest(expected=expected):
                result=self.run_source(source,{'calc.py':body})
                self.assertEqual(result.returncode,expected,result.stderr)
                self.assertIn('Ran 1 test',result.stderr)

    def test_no_os_disk_network_reflection_or_dynamic_test_import(self):
        for source in ['import os','open("outside","w")','import subprocess','import socket','import math\nmath.__dict__','print.__class__','import unittest\nunittest.main(module="antigravity")', 'import unittest\nmain=unittest.main\nmain(module="antigravity")','getattr(str,"format")','eval("1")','exec("print(1)")','import sys','from math import sqrt','@print\ndef x(): pass']:
            with self.subTest(source=source): self.assertNotEqual(self.run_source(source).returncode,0)

    def test_pure_code_unicode_and_malformed(self):
        result=self.run_source('import math\nprint("ação",math.sqrt(9))')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertNotEqual(self.run_source('def broken(').returncode,0)

@unittest.skipUnless(sys.platform=='win32','Windows AST runner')
class RestrictedPowerShell(unittest.TestCase):
    def test_literal_and_escape_attempts(self):
        for source, expected in [("Write-Output 'synthetic'",0),('exit 7',7),('Get-Content C:/Windows/win.ini',2),('Write-Output $(Get-Process)',2),('[IO.File]::ReadAllText("x")',2),('cmd.exe /c echo x',2),('$x="Get-Process"; & $x',2),('Write-Output "$(Get-Process)"',2),('Set-Item HKCU:/Software/x y',2)]:
            with self.subTest(source=source):
                r=subprocess.run(['powershell.exe','-NoLogo','-NoProfile','-NonInteractive','-File',str(ROOT/'app/src/developer_powershell.ps1')],input=json.dumps({'source':source}),capture_output=True,text=True,timeout=15)
                self.assertEqual(r.returncode,expected,r.stderr)
