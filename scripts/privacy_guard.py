"""Reject private/generated paths before publication; never inspect/log message content."""
import re
import subprocess

def forbidden(path):
    path=path.replace('\\','/')
    return bool(re.search(r'(^|/)(runtime|target|node_modules|__pycache__|private|profiles|prompts-private|backups|exports|technical-reports|memories|memorias|embeddings)(/|$)',path,re.I)
      or re.search(r'\.(sqlite3?|db)(-(wal|shm|journal))?$|\.(dpapi|log|gguf|pem|pfx|key)$',path,re.I)
      or re.search(r'(^|/)(technical-memory-report[^/]*\.json|AGENTS\.local\.md|identity(-profile)?\.json|personality\.private\.md|system-prompt\.private\.(txt|md)|\.env(\..*)?|api_tokens\.yml)$',path,re.I))

if __name__=='__main__':
    files=subprocess.check_output(['git','ls-files','-z']).decode().split('\0')
    rejected=[p for p in files if p and forbidden(p)]
    if rejected:
        raise SystemExit('Private/generated tracked paths rejected:\n'+'\n'.join(rejected))
    print('Privacy path guard: passed')
