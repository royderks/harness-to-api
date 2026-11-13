"""Staging follows the actual published version without changing its source."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class StageInstallers(unittest.TestCase):
    def stage(self, root):
        return subprocess.run([sys.executable, str(ROOT / 'scripts/stage-installers.py'),
                               'v0.1.2', '--source', str(root), '--output', str(root / 'out')],
                              capture_output=True, timeout=10)

    def test_future_versions_comments_and_line_endings(self):
        for newline in (b'\n', b'\r\n'):
            with self.subTest(newline=newline), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for name, assignment in [('install.sh', b'    version=v0.1.1'),
                                         ('install.ps1', b"    $version = 'v0.1.1'")]:
                    (root / name).write_bytes(b'# Previous default v0.1.0' + newline + assignment + newline)
                originals = {name: (root / name).read_bytes() for name in ('install.sh', 'install.ps1')}
                result = self.stage(root)
                self.assertEqual(result.returncode, 0, result.stderr)
                for name, original in originals.items():
                    self.assertEqual((root / name).read_bytes(), original)
                    self.assertEqual((root / 'out' / name).read_bytes(), original.replace(b'v0.1.1', b'v0.1.2'))

    def test_missing_or_duplicate_assignments_fail(self):
        for name in ('install.sh', 'install.ps1'):
            for count in (0, 2):
                with self.subTest(name=name, count=count), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    for filename, assignment in [('install.sh', b'    version=v0.1.1\n'),
                                                 ('install.ps1', b"    $version = 'v0.1.1'\n")]:
                        (root / filename).write_bytes(assignment * (count if filename == name else 1))
                    self.assertNotEqual(self.stage(root).returncode, 0)


if __name__ == '__main__':
    unittest.main()
