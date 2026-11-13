#!/usr/bin/env python3
"""Copy license/notice texts from resolved Cargo packages into a release archive.

This inventory supports human review; it does not interpret SPDX alternatives.
"""
import json
import pathlib
import shutil
import sys

metadata_path, destination = map(pathlib.Path, sys.argv[1:])
metadata = json.loads(metadata_path.read_text())
destination.mkdir(parents=True, exist_ok=True)
missing = []
index = []
for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
    label = f"{package['name']}-{package['version']}"
    source = pathlib.Path(package['manifest_path']).parent
    files = []
    for entry in source.iterdir():
        if entry.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright')):
            files.append(entry)
    if package.get('license_file'):
        files.append(source / package['license_file'])
    # Git workspaces often keep the license at their root instead of each member.
    if not files and package.get('source', '').startswith('git+'):
        for parent in source.parents:
            candidates = [p for p in parent.glob('*') if p.is_file() and p.name.lower().startswith(('license', 'copying', 'notice'))]
            if candidates:
                files = candidates
                break
            if (parent / '.git').exists():
                break
    if not files:
        family = 'accesskit' if package['name'].startswith('accesskit') else package['name']
        fallback = pathlib.Path(__file__).resolve().parent.parent / 'third_party' / family
        if fallback.is_dir():
            files = [p for p in fallback.iterdir() if p.name != 'SOURCE.txt']
    output = destination / label
    output.mkdir(exist_ok=True)
    for path in dict.fromkeys(files):
        if path.is_dir():
            shutil.copytree(path, output / path.name, dirs_exist_ok=True)
        elif path.is_file():
            shutil.copy2(path, output / path.name)
    if not list(output.iterdir()):
        missing.append(label)
    index.append({'package': label, 'license': package.get('license'), 'source': package.get('source'), 'license_texts_found': bool(list(output.iterdir()))})
(destination / 'index.json').write_text(json.dumps(index, indent=2) + '\n')
(destination / 'REVIEW.txt').write_text('Review the SPDX expressions and copied notices before publication.\n'
    + 'Packages with no standalone license text:\n' + '\n'.join(missing) + '\n')
print(f'Collected notices for {len(index)} packages; {len(missing)} need license-text review.')

# Ship only the dependency inventory, without build-machine paths or Cargo target internals.
metadata_path.write_text(json.dumps({'packages': [
    {key: package.get(key) for key in ('name', 'version', 'license', 'repository', 'source')}
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version']))
]}, indent=2) + '\n')
