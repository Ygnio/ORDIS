"""Trusted gate: stdin is released only after native Job Object assignment.

The native service supplies this closed plan after approval. No model text is executed here.
Project eligibility and executable hashes are checked and held by the native parent.
"""
import json
import subprocess
import sys

request = json.loads(sys.stdin.buffer.read(262145))
command = request['command']
if (not isinstance(command, list) or not 1 <= len(command) <= 32
        or any(not isinstance(a, str) or '\0' in a or len(a) > 8192 for a in command)):
    raise ValueError('Invalid native command plan')
# Compiler descendants inherit the parent's already assigned Job Object. No shell/breakaway.
process = subprocess.Popen(command, cwd=request['cwd'], env=request['environment'],
                           stdin=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
raise SystemExit(process.wait())
