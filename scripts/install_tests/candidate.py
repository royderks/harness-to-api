#!/usr/bin/env python3
"""Install the actual native archive through the staged Unix installer offline."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tarfile

parser = argparse.ArgumentParser()
parser.add_argument('--version', required=True)
parser.add_argument('--platform', required=True)
parser.add_argument('--fixture', type=Path, required=True)
args = parser.parse_args()
dist = Path('dist').resolve()
archive = dist / f'h2a_{args.version}_{args.platform}.tar.gz'
checksum = dist / f'checksums_{args.version}_{args.platform}.txt'
installer = dist / 'candidate-installers/install.sh'
with tempfile.TemporaryDirectory(prefix='h2a-candidate-install-') as tmp:
    root = Path(tmp)
    tools = root / 'tools'
    tools.mkdir()
    curl = tools / 'curl'
    curl.write_text('''#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in https://*) url=$1;; -o) shift; output=$1;; esac
  shift
done
case "$url" in */download/''' + args.version + '''/*) ;; *) exit 22;; esac
cp "$CANDIDATE_DIST/${url##*/}" "$output"
''')
    curl.chmod(0o755)
    dest = root / 'install with spaces'
    env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'],
               HOME=str(root), H2A_INSTALL_DIR=str(dest), H2A_INSTALL_ONLY='1', CANDIDATE_DIST=str(dist))
    def install():
        return subprocess.run(['sh', str(installer)], env=env, capture_output=True, text=True, timeout=60)
    for _ in range(2):
        result = install()
        if result.returncode:
            raise RuntimeError('Candidate install failed: ' + result.stderr)
    binary = dest / 'h2a'
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    with tarfile.open(archive) as package:
        if hashlib.sha256(package.extractfile('./h2a').read()).hexdigest() != digest:
            raise RuntimeError('Installed bytes differ from candidate archive')
    if subprocess.check_output([str(binary), 'version'], text=True).strip() != args.version:
        raise RuntimeError('Installed version mismatch')
    # Supply a corrupted checksum at the mocked download boundary, preserving dist.
    corrupt = root / 'corrupt'
    corrupt.mkdir()
    shutil.copy2(archive, corrupt / archive.name)
    (corrupt / checksum.name).write_text('0' * 64 + '  ' + archive.name + '\n')
    env['CANDIDATE_DIST'] = str(corrupt)
    if install().returncode == 0 or hashlib.sha256(binary.read_bytes()).hexdigest() != digest:
        raise RuntimeError('Bad checksum did not preserve installed binary')
    env['CANDIDATE_DIST'] = str(root / 'missing')
    if install().returncode == 0 or hashlib.sha256(binary.read_bytes()).hexdigest() != digest:
        raise RuntimeError('Failed download did not preserve installed binary')
    env.update(H2A_BIN=str(binary), H2A_FIXTURE=str(args.fixture.resolve()))
    subprocess.run(['python3', 'scripts/check-init.py'], env=env, check=True, timeout=180)
    print('PASS: candidate installer, repeat, checksum/download refusal and installed binary init')
