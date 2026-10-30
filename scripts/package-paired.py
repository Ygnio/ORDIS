"""Package a clean, paired build; no discovery or copying of user profiles."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def package(binary, loader, output, official=False):
    repo = Path(__file__).resolve().parents[1]
    commit = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
    dirty = subprocess.check_output(['git', '-C', str(repo), 'status', '--porcelain'], text=True).strip()
    if dirty:
        raise ValueError('Commit the reviewed source before packaging')
    if output.exists():
        raise ValueError('Package destination already exists; use a new version directory')
    for path in [binary / 'ordis.exe', binary / 'ordis-cli.exe', loader]:
        if not path.is_file():
            raise ValueError('Paired binaries and loader are required')
    # Running --version is read-only and does not discover profiles or start services.
    metadata = json.loads(subprocess.check_output([str(binary / 'ordis-cli.exe'), '--version'], text=True, encoding='utf-8'))
    if metadata['commit'] != commit or metadata['dirty'] or metadata['distribution'] != ('lucille' if official else 'ordis'):
        raise ValueError('Binary does not match this clean source commit/distribution')
    if official and not metadata['staging']:
        raise ValueError('This task only packages the guarded Lucille staging distribution')
    gui_metadata = json.loads(subprocess.check_output([str(binary / 'ordis.exe'), '--version'], text=True, encoding='utf-8'))
    if gui_metadata != metadata:
        raise ValueError('GUI and CLI provenance differ; rebuild both from the same source/configuration')
    output.mkdir(parents=True)
    app = output / 'app'
    app.mkdir()
    shutil.copy2(binary / 'ordis.exe', app / ('Lucille.exe' if official else 'ORDIS.exe'))
    shutil.copy2(binary / 'ordis-cli.exe', app / ('lucille-cli.exe' if official else 'ordis-cli.exe'))
    shutil.copy2(loader, app / 'WebView2Loader.dll')
    # Explicit public support allowlist. Never glob a data/profile directory.
    support = ['agente/workspace/AGENTS.md', 'app/ui/ordis.png',
               'scripts/serve_inference.py', 'scripts/service_shutdown.py', 'scripts/agent_web_search.py',
               'scripts/ordis_token_monitor.py', 'scripts/token_counter_paths.py', 'docs/CLI-GUI-CONTRACT.md']
    if not official:
        support += ['config/paths.json', 'config/tabbyapi-inicial.yml', 'config/ordis_qwen_thinking.yml',
                    'config/runtime-packages.txt', 'config/images-requirements.lock.txt']
    for name in support:
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(repo / name, target)
    if official:
        shutil.copy2(repo / 'scripts/Start-Lucille-Staging.ps1', output / 'Start-Lucille-Staging.ps1')
    manifest = dict(metadata, source_commit=commit, source_lock_sha256=sha(repo / 'app/Cargo.lock'),
                    binaries_same_build_configuration=True, operational_installation_updated=False,
                    files=[{'path': p.relative_to(output).as_posix(), 'bytes': p.stat().st_size, 'sha256': sha(p)}
                           for p in sorted(output.rglob('*')) if p.is_file()])
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    (output / 'SHA256SUMS').write_text(''.join(f'{sha(p)}  {p.relative_to(output).as_posix()}\n' for p in sorted(output.rglob('*')) if p.is_file()), encoding='utf-8')
    print(json.dumps({'package': str(output), 'commit': commit, 'manifest_sha256': sha(output / 'manifest.json')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', required=True, type=Path)
    parser.add_argument('--loader', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--official-staging', action='store_true')
    args = parser.parse_args()
    package(args.bin_dir.resolve(), args.loader.resolve(), args.output.resolve(), args.official_staging)
