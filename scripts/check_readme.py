#!/usr/bin/env python3
"""Check README links and its command reference against source or compiled help."""
import argparse
import itertools
import json
from pathlib import Path
import re
import subprocess

START = '<!-- commands:start -->'
END = '<!-- commands:end -->'


def help_commands(help_text):
    usage = help_text.split('\nUsage:\n', 1)[1].split('\n\n', 1)[0]
    commands = set()
    for line in usage.splitlines():
        parts = line.split()
        if not parts or parts[0] != 'h2a':
            raise ValueError('Unrecognized CLI usage line')
        words = []
        for part in parts[1:]:
            if part.startswith(('--', '[', '<')):
                break
            if not re.fullmatch(r'[a-z][a-z0-9_-]*(?:\|[a-z][a-z0-9_-]*)*', part):
                raise ValueError('Unrecognized command token in CLI help: ' + part)
            words.append(part.split('|'))
        if not words:
            raise ValueError('Missing command in CLI usage')
        commands.update('h2a ' + ' '.join(choice) for choice in itertools.product(*words))
    if not commands:
        raise ValueError('CLI help contains no commands')
    return commands


def check_commands(readme, help_text):
    if readme.count(START) != 1 or readme.count(END) != 1:
        raise ValueError('README needs exactly one marked Commands table')
    section = readme.split(START)[1].split(END)[0]
    rows = re.findall(r'^\| `(h2a [^`]+)` \|', section, re.MULTILINE)
    if len(rows) != len(set(rows)):
        raise ValueError('Duplicate README command')
    # Repository-maintenance commands are documented in the factory guide.
    expected = {command for command in help_commands(help_text)
                if command.split()[1] != 'factory'}
    missing, extra = expected - set(rows), set(rows) - expected
    if missing or extra:
        raise ValueError(f'Command reference mismatch: missing={sorted(missing)}, extra={sorted(extra)}')
    return len(expected)


def check_links(readme, root):
    for target in re.findall(r'\]\(([^)]+)\)', readme):
        if '://' in target or target.startswith('#'):
            continue
        if not (root / target.split('#')[0]).exists():
            raise ValueError('Missing README link target: ' + target)
    headings = {re.sub(r'[^\w -]', '', line[2:].lower()).replace(' ', '-')
                for line in readme.splitlines() if line.startswith('# ')}
    headings.update(re.sub(r'[^\w -]', '', line[3:].lower()).replace(' ', '-')
                    for line in readme.splitlines() if line.startswith('## '))
    for target in re.findall(r'\]\(#([^)]+)\)', readme):
        if target not in headings:
            raise ValueError('Missing README anchor: ' + target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    readme = (root / 'README.md').read_text()
    check_links(readme, root)
    if args.binary:
        help_text = subprocess.run([str(args.binary.resolve()), 'help'], check=True,
                                   capture_output=True, text=True, timeout=10).stdout
    else:
        source = (root / 'src/cli/mod.rs').read_text()
        match = re.search(r'const HELP: &str = ("(?:\\.|[^"\\])*");', source)
        if not match:
            raise ValueError('Cannot read CLI help literal; update the documentation checker')
        help_text = json.loads(match[1])
    count = check_commands(readme, help_text)
    print(f'PASS README links and {count} CLI commands')


if __name__ == '__main__':
    main()
