import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "factory.py"
FAKE_GH = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
p = Path(os.environ['FAKE_STATE'])
s = json.loads(p.read_text())
a = sys.argv[1:]
endpoint = a[1]
write = '--method' in a
if endpoint == 'repos/acme/repo':
    print(json.dumps({'id': 42, 'full_name': 'acme/repo', 'private': s.get('private', True), 'default_branch': 'main'}))
elif '/labels?' in endpoint:
    print(json.dumps([s['labels'][:1], s['labels'][1:]]))
elif '/issues?' in endpoint:
    if '--paginate' not in a or '--slurp' not in a:
        sys.exit(1)
    print(json.dumps([s['issues'][:1], s['issues'][1:]]))
elif endpoint.endswith('/labels') and write:
    item = json.load(sys.stdin)
    s['labels'].append(item)
    s['writes'].append('label')
    p.write_text(json.dumps(s))
    print(json.dumps(item))
elif endpoint.endswith('/issues') and write:
    if s.pop('reject_create', False):
        p.write_text(json.dumps(s))
        sys.exit(1)
    item = json.load(sys.stdin)
    number = max([i['number'] for i in s['issues']] + [0]) + 1
    item.update(number=number, state='open', html_url='https://github.com/acme/repo/issues/' + str(number))
    s['issues'].append(item)
    s['writes'].append('issue')
    lost = s.pop('lose_response', False)
    p.write_text(json.dumps(s))
    if lost:
        sys.exit(1)
    print(json.dumps(item))
else:
    sys.exit(1)
'''


class FactoryCLI(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'scripts').mkdir()
        (self.root / 'factory').mkdir()
        (self.root / 'bin').mkdir()
        shutil.copy(SCRIPT, self.root / 'scripts/factory.py')
        (self.root / 'intent.md').write_text('intent')
        self.gh = self.root / 'bin/gh'
        self.gh.write_text(FAKE_GH)
        self.gh.chmod(0o755)
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        self.state = self.root / 'state.json'
        self.write_state({'issues': [], 'labels': [], 'writes': []})
        self.manifest = {
            'version': 1,
            'labels': [{'name': 'factory:triage', 'color': '123456', 'description': 'Triage'}, {'name': 'priority:p2', 'color': 'abcdef', 'description': 'Priority'}],
            'seeds': [self.seed('first'), self.seed('second', ['first'])],
        }
        self.write_manifest()
        self.env = dict(os.environ, PATH=str(self.root / 'bin') + os.pathsep + os.environ['PATH'], FAKE_STATE=str(self.state))

    def seed(self, key, deps=None):
        return {'id': key, 'title': key, 'intent': 'Intent', 'sources': ['intent.md'], 'scope': ['future.go'], 'acceptance': ['Works'], 'verification': ['Run tests'], 'depends_on': deps or [], 'labels': ['factory:triage', 'priority:p2'], 'risk': 'low'}

    def write_state(self, state):
        self.state.write_text(json.dumps(state))

    def get_state(self):
        return json.loads(self.state.read_text())

    def write_manifest(self):
        (self.root / 'factory/seeds.json').write_text(json.dumps(self.manifest))

    def cli(self, *args, ok=True):
        result = subprocess.run([sys.executable, str(self.root / 'scripts/factory.py'), *args], cwd=self.root, env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)
        return result

    def test_dry_run_and_seed_preserve_edits_and_closed_issue(self):
        self.cli('validate')
        self.cli('plan', '--repo', 'acme/repo')
        self.cli('seed', '--repo', 'acme/repo')
        self.assertEqual(self.get_state()['writes'], [])
        self.assertFalse((self.root / '.git/h2a-factory-journal.json').exists())
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        state = self.get_state()
        self.assertEqual(state['writes'], ['label', 'label', 'issue', 'issue'])
        self.assertIn('https://github.com/acme/repo/issues/1', state['issues'][1]['body'])
        self.assertIn('https://github.com/acme/repo/blob/main/intent.md', state['issues'][0]['body'])
        state['issues'][0].update(title='Human title', body='Human body', state='closed', labels=[])
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        self.assertEqual(self.get_state(), state)

    def test_lost_response_requires_explicit_recovery(self):
        state = self.get_state()
        state['lose_response'] = True
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        state = self.get_state()
        self.assertEqual(len(state['issues']), 1)
        result = self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertIn('pending', result.stderr)
        self.assertEqual(self.get_state(), state)
        self.cli('recover', '--repo', 'acme/repo', '--id', 'first', '--confirm-absent', ok=False)
        self.cli('recover', '--repo', 'acme/repo', '--id', 'first', '--issue', '2', ok=False)
        self.cli('recover', '--repo', 'acme/repo', '--id', 'first', '--issue', '1')
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        self.assertEqual(len(self.get_state()['issues']), 2)

    def test_rejected_create_requires_explicit_absence_confirmation(self):
        state = self.get_state()
        state['reject_create'] = True
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        state = self.get_state()
        self.assertEqual(state['issues'], [])
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state(), state)
        self.cli('recover', '--repo', 'acme/repo', '--id', 'first', '--confirm-absent')
        self.assertEqual(self.get_state(), state)
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        self.assertEqual(len(self.get_state()['issues']), 2)

    def test_public_repository_and_dependency_cycle_do_not_publish(self):
        state = self.get_state()
        state['private'] = False
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state(), state)
        self.manifest['seeds'][0]['depends_on'] = ['second']
        self.write_manifest()
        self.cli('validate', ok=False)
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state(), state)

    def test_reserved_marker_in_seed_text_fails_before_publication(self):
        for field in ('intent', 'acceptance', 'verification', 'scope'):
            with self.subTest(field=field):
                self.manifest['seeds'][0] = self.seed('first')
                marker = 'Do not publish <!-- h2a-seed:another --> as task text'
                self.manifest['seeds'][0][field] = marker if field == 'intent' else [marker]
                self.write_manifest()
                self.cli('validate', ok=False)
                self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
                self.assertEqual(self.get_state()['writes'], [])

    def test_pagination_discovers_existing_markers_and_remembers_them(self):
        state = self.get_state()
        state['issues'] = [dict(number=1, body='unrelated', state='open', html_url='https://github.com/acme/repo/issues/1'), dict(number=2, body='<!-- h2a-seed:first -->', state='closed', html_url='https://github.com/acme/repo/issues/2')]
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        state = self.get_state()
        self.assertEqual(len(state['issues']), 3)
        state['issues'][1]['body'] = 'edited'
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        self.assertEqual(len(self.get_state()['issues']), 3)

    def test_invalid_inputs_and_duplicate_markers_fail_before_writes(self):
        self.manifest['seeds'][0]['sources'] = ['../escape']
        self.write_manifest()
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state()['writes'], [])
        self.manifest['seeds'][0]['sources'] = ['intent.md']
        self.write_manifest()
        state = self.get_state()
        state['issues'] = [dict(number=n, body='<!-- h2a-seed:first -->', state='open', html_url='https://github.com/acme/repo/issues/' + str(n)) for n in (1, 2)]
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state()['writes'], [])

    def test_corrupt_journal_and_missing_mapped_issue_stop_writes(self):
        self.cli('seed', '--repo', 'acme/repo', '--apply')
        state = self.get_state()
        state['issues'].pop(0)
        self.write_state(state)
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state(), state)
        (self.root / '.git/h2a-factory-journal.json').write_text('{')
        self.cli('seed', '--repo', 'acme/repo', '--apply', ok=False)
        self.assertEqual(self.get_state(), state)


if __name__ == '__main__':
    unittest.main()
