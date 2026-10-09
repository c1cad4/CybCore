#!/usr/bin/env python3
"""Clone pinned ecosystem siblings; never reset an existing working directory."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile


def validate_pins(rows):
    """Validate the entire lock before starting any filesystem or Git work."""
    names = set()
    for row in rows:
        name, commit = row.get('name'), row.get('commit')
        if (not isinstance(name, str) or
                not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', name) or
                not isinstance(commit, str) or
                not re.fullmatch(r'[0-9a-f]{40}', commit)):
            raise ValueError('invalid pinned repository')
        if name.casefold() in names:
            raise ValueError(f'{name}: duplicate pinned repository')
        names.add(name.casefold())


def bootstrap(root, rows, verify=False):
    rows = list(rows)
    validate_pins(rows)
    root = Path(root).resolve()
    if not verify:
        root.mkdir(parents=True, exist_ok=True)
    for row in rows:
        name, commit = row['name'], row['commit']
        target = root / name
        if target.is_symlink():
            raise ValueError(f'{name}: checkout path is a symbolic link')
        if target.exists():
            if not (target / '.git').exists():
                raise ValueError(f'{name}: existing directory is not a Git checkout')
            if verify:
                actual = subprocess.check_output(['git', '-C', str(target), 'rev-parse', 'HEAD'], text=True).strip()
                if actual != commit:
                    raise ValueError(f'{name}: checkout differs from lock; local changes preserved')
            print(f'EXISTING {name}', flush=True)
            continue
        if verify:
            raise ValueError(f'{name}: checkout is missing')
        # Publish the checkout only after all Git steps succeed. A failed fetch
        # must not leave an incomplete directory that a later run will skip.
        with tempfile.TemporaryDirectory(prefix=f'.{name}-', dir=root) as staging:
            checkout = Path(staging) / name
            subprocess.run(['git', 'init', str(checkout)], check=True)
            subprocess.run(['git', '-C', str(checkout), 'remote', 'add', 'origin',
                            f'https://github.com/c1cad4/{name}.git'], check=True)
            subprocess.run(['git', '-C', str(checkout), 'fetch', '--depth=1',
                            'origin', commit], check=True)
            subprocess.run(['git', '-C', str(checkout), 'checkout', '--detach', commit], check=True)
            if target.exists() or target.is_symlink():
                raise ValueError(f'{name}: checkout appeared during bootstrap')
            checkout.rename(target)
        print(f'CLONED {name} {commit}', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    rows = json.loads((Path(__file__).resolve().parents[1] / 'components.lock.json').read_text())['repositories']
    bootstrap(args.root.resolve(), rows, args.verify)

if __name__ == '__main__':
    main()
