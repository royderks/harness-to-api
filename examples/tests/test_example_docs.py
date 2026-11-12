"""Regression tests for the executable example documentation contract."""
import importlib.util
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
        path = self.root / 'examples/requirements.txt'
        path.write_text(path.read_text().replace('e2b==2.10.2', 'e2b==2.10.3'))
        self.assertEqual(checker.check(self.root), [])

    def test_dependency_drift_fails(self):
        path = self.root / 'examples/requirements.txt'
        path.write_text(path.read_text().replace('e2b==2.10.2', ''))
        self.assertTrue(any('dependency' in error for error in checker.check(self.root)))


if __name__ == '__main__':
    unittest.main()
