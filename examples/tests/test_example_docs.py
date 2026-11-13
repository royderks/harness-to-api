"""Regression tests for the executable example documentation contract."""
import importlib.util
import re
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('example_docs', ROOT / 'scripts/check-example-docs.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class ExampleDocs(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        shutil.copytree(ROOT / 'examples', self.root / 'examples')
        shutil.copytree(ROOT / 'docs', self.root / 'docs')

    def dependency_pin(self, name):
        requirements = self.root / 'examples/requirements.txt'
        matches = re.findall(rf'(?m)^{re.escape(name)}==([^\s]+)$', requirements.read_text())
        self.assertEqual(len(matches), 1, f'expected one pin for {name}')
        return f'{name}=={matches[0]}'

    def replace_pin(self, name, replacement):
        path = self.root / 'examples/requirements.txt'
        pin = self.dependency_pin(name)
        path.write_text(path.read_text().replace(pin, replacement, 1))
        return pin

    def test_current_docs_pass(self):
        self.assertEqual(checker.check(self.root), [])

    def test_command_drift_fails(self):
        path = self.root / 'examples/agents-sdk/README.md'
        path.write_text(path.read_text().replace('python examples/agents-sdk/main.py', 'python examples/agents-sdk/main.py --changed'))
        self.assertTrue(any('command' in error for error in checker.check(self.root)))

    def test_missing_example_fails(self):
        shutil.rmtree(self.root / 'examples/modal')
        self.assertTrue(any('modal' in error for error in checker.check(self.root)))

    def test_invalid_link_fails(self):
        path = self.root / 'examples/README.md'
        path.write_text(path.read_text() + '\n[bad](missing.md)\n')
        self.assertTrue(any('invalid link' in error for error in checker.check(self.root)))

    def test_version_updates_keep_documented_install_commands_valid(self):
        pin = self.dependency_pin('e2b')
        self.replace_pin('e2b', pin + '.1')
        self.assertEqual(checker.check(self.root), [])

    def test_inline_dependency_pin_fails_after_version_bump(self):
        pin = self.dependency_pin('openai')
        self.replace_pin('openai', pin + '.1')
        self.assertEqual(checker.check(self.root), [])
        path = self.root / 'examples/harness-switch/README.md'
        path.write_text(path.read_text() + f'\nInstall `{pin}` in your environment.\n')
        self.assertTrue(any('dependency pin' in error for error in checker.check(self.root)))

    def test_dependency_drift_fails(self):
        self.replace_pin('e2b', '')
        self.assertTrue(any('dependency' in error for error in checker.check(self.root)))


if __name__ == '__main__':
    unittest.main()
