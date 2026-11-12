#!/usr/bin/env python3
"""Check example command blocks, pinned dependencies, and local Markdown links."""
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
BLOCK = re.compile(r'<!-- example-command: ([\w-]+) -->\n```sh\n(.*?)\n```\n<!-- /example-command -->', re.S)
LINK = re.compile(r'(?<!!)\[[^\]]+\]\(([^)]+)\)')


def check(root):
    examples = root / 'examples'
    manifest = json.loads((examples / 'commands.json').read_text())
    errors = []
    directories = {p.name for p in examples.iterdir() if p.is_dir() and p.name not in {'tests', '__pycache__'}}
    expected_examples = {'agents-sdk', 'docker', 'e2b', 'harness-switch', 'langgraph', 'modal', 'openshell'}
    if set(manifest) != expected_examples:
        errors.append('manifest must cover all seven examples')
    if directories != set(manifest):
        errors.append(f'example directories and manifest differ: {sorted(directories ^ set(manifest))}')
    if {name for name, entry in manifest.items() if entry['offline']} != {'harness-switch', 'langgraph', 'agents-sdk'}:
        errors.append('offline examples must be the three local SDK clients')
    requirements = {line.split('==')[0] for line in (examples / 'requirements.txt').read_text().splitlines() if '==' in line}
    for name, entry in manifest.items():
        readme = examples / name / 'README.md'
        if not readme.is_file():
            errors.append(f'missing README: {readme}')
            continue
        content = readme.read_text()
        expected = {name if len(entry['commands']) == 1 else f'{name}-{i+1}': command
                    for i, command in enumerate(entry['commands'])}
        for command in entry['commands']:
            allowed_commands = {f'python examples/{name}/main.py'}
            if name == 'openshell':
                allowed_commands.add('python examples/openshell/main.py --local')
            if command.startswith('python examples/') and (command not in allowed_commands or not (examples / name / 'main.py').is_file()):
                errors.append(f'{name}: invalid Python launch command')
        blocks = BLOCK.findall(content)
        if len(blocks) != len(expected) or dict(blocks) != expected:
            errors.append(f'{name}: command blocks differ from manifest')
        for dep in entry['dependencies']:
            if dep not in requirements or dep not in content:
                errors.append(f'{name}: dependency {dep} missing from requirements or README')
        if entry['offline'] and (len(entry['commands']) != 1 or not re.fullmatch(r'python examples/[\w-]+/main\.py', entry['commands'][0])):
            errors.append(f'{name}: offline command must be a direct Python invocation')
    known_commands = {command for entry in manifest.values() for command in entry['commands']}
    for readme in examples.rglob('README.md'):
        content = readme.read_text()
        outside_blocks = BLOCK.sub('', content)
        for dependency in requirements:
            if re.search(r'(?<![\w-])' + re.escape(dependency) + r'\s*(?:==|~=|>=|<=|>|<|!=)\s*[0-9]', content):
                errors.append(f'{readme.relative_to(root)}: dependency pin must come from examples/requirements.txt: {dependency}')
        if re.search(r'^openshell (?:profile|provider|sandbox) ', outside_blocks, re.M):
            errors.append(f'{readme.relative_to(root)}: OpenShell command outside checked block')
        for command in re.findall(r'^python examples/[\w-]+/main\.py(?:[^\n]*)$', content, re.M):
            if command not in known_commands:
                errors.append(f'{readme.relative_to(root)}: undocumented manifest command {command}')
        for link in LINK.findall(content):
            url = urlsplit(link.split()[0])
            if url.scheme or url.netloc:
                continue
            target = (readme.parent / unquote(url.path)).resolve() if url.path else readme
            if target.is_dir():
                target /= 'README.md'
            if not target.is_file():
                errors.append(f'{readme.relative_to(root)}: invalid link {link}')
                continue
            if url.fragment and target.suffix == '.md':
                headings = re.findall(r'^#{1,6}\s+(.+?)\s*#*$', target.read_text(), re.M)
                anchors = [re.sub(r'[^\w\- ]', '', h.lower()).replace(' ', '-') for h in headings]
                if unquote(url.fragment) not in anchors:
                    errors.append(f'{readme.relative_to(root)}: invalid anchor {link}')
    return errors


def main():
    errors = check(ROOT)
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print('PASS example command blocks, dependencies, and local links')
    return 0


if __name__ == '__main__':
    sys.exit(main())
