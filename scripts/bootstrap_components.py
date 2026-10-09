#!/usr/bin/env python3
"""Clone pinned ecosystem siblings; never reset an existing working directory."""
import argparse
import json
from pathlib import Path
import re
import subprocess


def bootstrap(root, rows, verify=False):
    for row in rows:
        name, commit = row['name'], row['commit']
        if not re.fullmatch(r'[A-Za-z0-9_.-]+', name) or name in ('.', '..') or not re.fullmatch(r'[0-9a-f]{40}', commit):
            raise ValueError('invalid pinned repository')
        target = root / name
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
        subprocess.run(['git', 'clone', '--no-checkout', f'https://github.com/c1cad4/{name}.git', str(target)], check=True)
        subprocess.run(['git', '-C', str(target), 'fetch', 'origin', commit], check=True)
        subprocess.run(['git', '-C', str(target), 'checkout', '--detach', commit], check=True)
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
