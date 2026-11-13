#!/usr/bin/env python3
"""Stage version-switched installer copies without changing public defaults."""
import argparse
from pathlib import Path
import re

parser = argparse.ArgumentParser()
parser.add_argument('version')
parser.add_argument('--source', type=Path, default=Path('.'))
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+', args.version):
    parser.error('version must be a stable vMAJOR.MINOR.PATCH')
args.output.mkdir(parents=True, exist_ok=True)
for name, old, new in (
    ('install.sh', '    version=v0.1.0', '    version=' + args.version),
    ('install.ps1', "    $version = 'v0.1.0'", "    $version = '" + args.version + "'"),
):
    source = (args.source / name).read_bytes()
    old, new = old.encode(), new.encode()
    if source.count(old) != 1:
        raise ValueError(name + ': expected exactly one public version assignment')
    (args.output / name).write_bytes(source.replace(old, new, 1))
