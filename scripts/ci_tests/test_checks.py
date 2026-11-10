import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


changes = load('ci_changes')
readme = load('check_readme')


class CheckSelection(unittest.TestCase):
    def test_conservative_path_selection(self):
        for paths in [['README.md'], ['README.md', 'docs/api.md']]:
            self.assertFalse(changes.needs_build(paths))
        for paths in [[], ['Cargo.lock'], ['src/main.rs'], ['README.md', 'src/main.rs'],
                      ['.github/workflows/ci.yml'], ['docs/example.py'], ['new-file'],
                      ['src/template.md'], ['README.md\nsrc/main.rs']]:
            self.assertTrue(changes.needs_build(paths), paths)

    def test_real_git_diff_and_failed_detection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()
            git('init', '-q')
            git('config', 'user.email', 'fixture@example.invalid')
            git('config', 'user.name', 'Fixture')
            (root / 'README.md').write_text('before')
            git('add', '.'); git('commit', '-qm', 'base')
            base = git('rev-parse', 'HEAD')
            (root / 'README.md').write_text('after')
            git('add', '.'); git('commit', '-qm', 'docs')
            head = git('rev-parse', 'HEAD')
            event = root / 'event.json'; output = root / 'output'
            event.write_text(json.dumps({'pull_request': {'base': {'sha': base}, 'head': {'sha': head}}}))
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_OUTPUT=str(output),
                       GITHUB_EVENT_NAME='pull_request')
            cmd = [sys.executable, str(ROOT / 'ci_changes.py')]
            good = subprocess.run(cmd, cwd=root, env=env, capture_output=True)
            self.assertEqual(good.returncode, 0)
            self.assertEqual(output.read_text(), 'build=false\n')
            # A rename from executable input to Markdown still includes the old path.
            (root / 'tool.py').write_text('print(1)')
            git('add', '.'); git('commit', '-qm', 'code')
            before = git('rev-parse', 'HEAD')
            git('mv', 'tool.py', 'notes.md'); git('commit', '-qm', 'rename')
            event.write_text(json.dumps({'before': before, 'after': git('rev-parse', 'HEAD')}))
            env['GITHUB_EVENT_NAME'] = 'push'; output.write_text('')
            self.assertEqual(subprocess.run(cmd, cwd=root, env=env, capture_output=True).returncode, 0)
            self.assertEqual(output.read_text(), 'build=true\n')
            event.write_text(json.dumps({'before': 'a' * 40, 'after': head})); output.write_text('')
            failed = subprocess.run(cmd, cwd=root, env=env, capture_output=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(output.read_text(), '')


class CommandReference(unittest.TestCase):
    HELP = '\nUsage:\n  h2a init [flags]\n  h2a init repair --discard\n  h2a keys create|list [flags]\n  h2a help [command]\n\nNext steps'
    TABLE = '<!-- commands:start -->\n' + '\n'.join(
        '| `' + command + '` | Description |' for command in
        ['h2a init', 'h2a init repair', 'h2a keys create', 'h2a keys list', 'h2a help']
    ) + '\n<!-- commands:end -->'

    def test_subcommands_and_flags(self):
        self.assertEqual(readme.check_commands(self.TABLE, self.HELP), 5)

    def test_omitted_new_obsolete_and_duplicate_commands_fail(self):
        for table, help_text in [
            (self.TABLE.replace('| `h2a init` | Description |', ''), self.HELP),
            (self.TABLE, self.HELP.replace('  h2a help', '  h2a new-command\n  h2a help')),
            (self.TABLE.replace('h2a keys list', 'h2a obsolete'), self.HELP),
            (self.TABLE.replace('<!-- commands:end -->', '| `h2a help` | duplicate |\n<!-- commands:end -->'), self.HELP),
        ]:
            with self.assertRaises(ValueError):
                readme.check_commands(table, help_text)

    def test_new_command_tokens_cannot_disappear(self):
        for command in ['repair2', 'repair_state', 'repair.v2']:
            help_text = self.HELP.replace('  h2a help', f'  h2a init {command}\n  h2a help')
            with self.assertRaises(ValueError):
                readme.check_commands(self.TABLE, help_text)
        table = self.TABLE.replace('<!-- commands:end -->', '| `h2a init repair2` | Repair |\n<!-- commands:end -->')
        help_text = self.HELP.replace('  h2a help', '  h2a init repair2\n  h2a help')
        self.assertEqual(readme.check_commands(table, help_text), 6)

    def test_broken_local_link_and_anchor_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            for markdown in ['[missing](missing.md)', '[missing](#absent)']:
                with self.assertRaises(ValueError):
                    readme.check_links(markdown, Path(directory))
            readme.check_links('## Getting started\n[go](#getting-started)', Path(directory))


if __name__ == '__main__':
    unittest.main()
