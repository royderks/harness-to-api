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
for name, pattern in (
    ('install.sh', rb"(?m)^(    version=)v[0-9]+\.[0-9]+\.[0-9]+(\r?)$"),
    ('install.ps1', rb"(?m)^(    \$version = ')v[0-9]+\.[0-9]+\.[0-9]+('\r?)$"),
):
    source = (args.source / name).read_bytes()
    staged, count = re.subn(pattern, lambda match: match[1] + args.version.encode() + match[2], source)
    if count != 1:
        raise ValueError(name + ': expected exactly one public version assignment')
    (args.output / name).write_bytes(staged)
