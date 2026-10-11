#!/usr/bin/env python3
"""Collect license metadata and shipped license texts from the locked dependency graph.

Includes build dependencies conservatively; excludes dependencies used only by
tests. This creates notices, not a legal clearance decision.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def render(metadata):
    packages = {p['id']: p for p in metadata['packages']}
    nodes = {n['id']: n for n in metadata['resolve']['nodes']}
    pending = [p['id'] for p in packages.values() if p['name'] == 'fluxion-app']
    seen = set()
    while pending:
        package_id = pending.pop()
        if package_id in seen:
            continue
        seen.add(package_id)
        pending.extend(d['pkg'] for d in nodes[package_id]['deps']
                       if any(k['kind'] != 'dev' for k in d['dep_kinds']))
    result = ['# Fluxion third-party notices', '',
              'Locked runtime and build dependency inventory. License expressions',
              'and available upstream license/notice texts are reproduced below.',
              'Build dependencies are included conservatively.', '']
    for package in sorted((packages[i] for i in seen), key=lambda p: (p['name'], p['version'])):
        vendored = (package.get('metadata') or {}).get('fluxion-vendored')
        if not package['source'] and not vendored:
            continue
        directory = Path(package['manifest_path']).parent
        license_files = sorted({p for p in directory.glob('*') if p.is_file()
                                and p.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE'))})
        if package.get('license_file'):
            license_files = sorted(set(license_files + [directory / package['license_file']]))
        if not package.get('license') and not license_files:
            raise ValueError(f"Missing license information: {package['name']}")
        result += [f"## {package['name']} {package['version']}", '',
                   f"License: {package.get('license') or 'see license file'}", '',
                   f"Source: https://crates.io/crates/{package['name']}/{package['version']}", '']
        if vendored:
            result += [f"Upstream: {vendored['upstream']}", '', f"Fluxion modifications: {vendored['changes']}", '']
        for path in license_files:
            result += [f'### {path.name}', '', path.read_text(errors='replace').strip(), '']
    return '\n'.join(result)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
                             '--filter-platform', 'aarch64-apple-darwin'], cwd=ROOT, check=True,
                            capture_output=True, text=True)
    args.output.write_text(render(json.loads(result.stdout)))


if __name__ == '__main__':
    main()
