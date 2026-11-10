#!/usr/bin/env python3
"""Select native checks from the complete Git diff, not GitHub's path filter."""
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess


def needs_build(paths):
    # Only documentation locations are exempt; unfamiliar inputs run all checks.
    return not paths or any(
        not (PurePosixPath(path).suffix == '.md' and
             (len(PurePosixPath(path).parts) == 1 or path.startswith('docs/')))
        for path in paths
    )


def changed_paths(event, name):
    if name == 'pull_request':
        base, head = event['pull_request']['base']['sha'], event['pull_request']['head']['sha']
        separator = '...'
    elif name == 'push':
        base, head = event['before'], event['after']
        separator = '..'
        if base == '0' * 40:
            return []  # New branch: run everything.
    else:
        raise ValueError('Unsupported CI event')
    if not all(re.fullmatch(r'[0-9a-f]{40}', value) for value in (base, head)):
        raise ValueError('Invalid Git revision')
    result = subprocess.run(
        ['git', 'diff', '--no-renames', '--name-only', '-z', base + separator + head, '--'],
        check=True, capture_output=True,
    )
    return [os.fsdecode(p) for p in result.stdout.split(b'\0') if p]


def main():
    event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
    paths = changed_paths(event, os.environ['GITHUB_EVENT_NAME'])
    build = str(needs_build(paths)).lower()
    with open(os.environ['GITHUB_OUTPUT'], 'a') as out:
        out.write('build=' + build + '\n')
    print('Native build required: ' + build + f' ({len(paths)} changed paths)')


if __name__ == '__main__':
    main()
