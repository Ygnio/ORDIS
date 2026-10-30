"""Build only with explicitly supplied, already installed native tools. Never calls rustup."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def prepare(config, root, reuse=False):
    root = Path(root).resolve()
    names = ('cargo', 'rustc', 'rustdoc', 'cc', 'ar', 'linker')
    tools = {}
    for name in names:
        path = Path(config[name])
        if not path.is_absolute() or not path.is_file():
            raise ValueError(f'Missing explicit native tool: {name}')
        # rustup proxies have the same hash as rustup.exe; reject without executing.
        proxy = path.parent / 'rustup.exe'
        if proxy.is_file() and digest(path) == digest(proxy):
            raise ValueError(f'Rustup proxy forbidden: {name}')
        tools[name] = path.resolve()
    target = config['target']
    if target not in ('x86_64-pc-windows-gnu', 'x86_64-pc-windows-msvc'):
        raise ValueError('Unsupported target')
    std = tools['rustc'].parent.parent / 'lib/rustlib' / target / 'lib'
    if not std.is_dir():
        raise ValueError('Target standard library is not installed')
    cache = Path(config['cache_source']).resolve()
    if not (cache / 'registry').is_dir():
        raise ValueError('Offline dependency cache is missing')
    evidence = {'target': target, 'tools': {k: {'path': str(v), 'sha256': digest(v)} for k, v in tools.items()}}
    if root.exists() and not reuse:
        raise ValueError('Use a new, isolated build directory')
    if reuse:
        previous = json.loads((root / 'environment.json').read_text(encoding='utf-8'))
        if previous['target'] != target or previous['tools'] != evidence['tools']:
            raise ValueError('Cannot reuse build state with a different toolchain')
    # All preflight checks precede filesystem mutation and child processes.
    root.mkdir(parents=True, exist_ok=reuse)
    home = root / 'cargo-home'
    if not reuse:
        shutil.copytree(cache / 'registry', home / 'registry')
        if (cache / 'git').exists():
            shutil.copytree(cache / 'git', home / 'git')
    env = os.environ.copy()
    for name in list(env):
        if name.startswith(('CARGO_', 'RUST', 'CC_', 'CXX_', 'AR_')) or name in ('CC', 'CXX', 'AR'):
            del env[name]
    env.update({str(k): str(v) for k, v in config.get('environment', {}).items()})
    env.update(RUSTC=str(tools['rustc']), RUSTDOC=str(tools['rustdoc']),
               RUSTUP_AUTO_INSTALL='0', RUSTUP_HOME=str(root / 'unused-rustup'),
               CARGO_HOME=str(home), CARGO_TARGET_DIR=str(root / 'target'),
               CARGO_NET_OFFLINE='true', CC=str(tools['cc']), AR=str(tools['ar']),
               CARGO_BUILD_JOBS=str(config.get('jobs', 2)))
    env['CARGO_TARGET_' + target.upper().replace('-', '_') + '_LINKER'] = str(tools['linker'])
    # Child only: never write user/system PATH. Keep OS runtime tools available.
    env['PATH'] = os.pathsep.join(dict.fromkeys(str(p.parent) for p in tools.values())) + os.pathsep + os.environ.get('PATH', '')
    (root / 'environment.json').write_text(json.dumps(evidence, indent=2), encoding='utf-8')
    return tools, env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--reuse', action='store_true', help='Reuse only this isolated cache after checking tool hashes')
    parser.add_argument('cargo_args', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    config = json.loads(Path(args.config).read_text(encoding='utf-8-sig'))
    repo = Path(__file__).resolve().parents[1]
    extra = args.cargo_args
    if extra[:1] == ['--']:
        extra = extra[1:]
    if not extra or extra[0] not in ('test', 'build', 'check'):
        raise ValueError('Only test/build/check are permitted')
    forbidden = ('--target', '--manifest-path', '--config', '--target-dir', '--lockfile-path')
    if any(a == '--' or a.startswith('+') or any(a == f or a.startswith(f + '=') for f in forbidden) for a in extra):
        raise ValueError('Do not override the isolated build contract')
    tools, env = prepare(config, args.output, args.reuse)
    command = [str(tools['cargo']), *extra, '--offline', '--locked', '--target', config['target'],
               '--manifest-path', str(repo / 'app/Cargo.toml')]
    evidence = json.loads((Path(args.output) / 'environment.json').read_text())
    evidence.update(commit=subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
                    lock_sha256=digest(repo / 'app/Cargo.lock'), arguments=extra)
    (Path(args.output) / 'environment.json').write_text(json.dumps(evidence, indent=2), encoding='utf-8')
    with (Path(args.output) / 'build.log').open('w', encoding='utf-8') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
