"""Exercise the shipped Unix installer with local, checksum-addressed archives."""
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipIf(os.name == 'nt', 'Unix installer; Windows has its own native test')
class Installer(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='h2a-installer-test-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / 'tools'
        self.bin.mkdir()
        self.dest = self.root / 'install with spaces'
        self.dest.mkdir()
        (self.dest / 'h2a').write_text('existing binary')
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        HOME=str(self.root), H2A_INSTALL_DIR=str(self.dest),
                        H2A_INSTALL_ONLY='1', FIXTURES=str(self.root))
        self.tool('uname', '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n')
        self.tool('curl', '#!' + sys.executable + '\n' + '''import os, pathlib, shutil, sys
args=sys.argv[1:]
if os.environ.get('FAIL_DOWNLOAD'): sys.exit(22)
url=next(a for a in args if a.startswith('https://'))
shutil.copyfile(pathlib.Path(os.environ['FIXTURES'])/url.rsplit('/',1)[1],args[args.index('-o')+1])
''')
        self.archive()

    def tool(self, name, text):
        p = self.bin / name
        p.write_text(text)
        p.chmod(0o755)

    def archive(self, platform='darwin_arm64', content=b'#!/bin/sh\necho installed\n', symlink=False):
        name = 'h2a_v0.1.0_' + platform + '.tar.gz'
        with tarfile.open(self.root / name, 'w:gz') as tar:
            info = tarfile.TarInfo('./h2a')
            info.mode = 0o755
            if symlink:
                info.type = tarfile.SYMTYPE
                info.linkname = '/tmp/unrelated-h2a'
                tar.addfile(info)
            else:
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
        digest = hashlib.sha256((self.root / name).read_bytes()).hexdigest()
        (self.root / ('checksums_v0.1.0_' + platform + '.txt')).write_text(digest + '  ' + name + '\n')

    def run_install(self):
        return subprocess.run(['sh', str(ROOT / 'install.sh')], env=self.env,
                              text=True, capture_output=True, timeout=20)

    def test_install_and_repeat(self):
        for _ in range(2):
            p = self.run_install()
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertEqual(subprocess.check_output([str(self.dest / 'h2a')]), b'installed\n')
            self.assertFalse(list(self.dest.glob('.h2a-install.*')))

    def test_platforms(self):
        for os_name, machine, platform in [('Darwin', 'x86_64', 'darwin_amd64'),
                                          ('Linux', 'aarch64', 'linux_arm64'),
                                          ('Linux', 'x86_64', 'linux_amd64')]:
            self.tool('uname', '#!/bin/sh\ncase "$1" in -s) echo ' + os_name + ';; -m) echo ' + machine + ';; esac\n')
            self.archive(platform)
            p = self.run_install()
            self.assertEqual(p.returncode, 0, p.stderr)

    def test_failures_preserve_binary(self):
        checksum = self.root / 'checksums_v0.1.0_darwin_arm64.txt'
        original = checksum.read_text()
        for bad in ['0' * 64 + '  h2a_v0.1.0_darwin_arm64.tar.gz\n', original + original,
                    original.replace('h2a_v0.1.0_darwin_arm64.tar.gz', '../elsewhere')]:
            checksum.write_text(bad)
            self.assertNotEqual(self.run_install().returncode, 0)
            self.assertEqual((self.dest / 'h2a').read_text(), 'existing binary')
        checksum.write_text(original)
        self.env['FAIL_DOWNLOAD'] = '1'
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertEqual((self.dest / 'h2a').read_text(), 'existing binary')

    def test_symlink_payload_rejected(self):
        self.archive(symlink=True)
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertEqual((self.dest / 'h2a').read_text(), 'existing binary')

    def test_pipe_reconnects_interactive_stdin(self):
        import pty
        self.archive(content=b'#!/bin/sh\n[ "$1" = init ] && [ -t 0 ] && echo INTERACTIVE_SETUP_STARTED\n')
        self.env.pop('H2A_INSTALL_ONLY')
        pid, master = pty.fork()
        if pid == 0:
            os.execvpe('sh', ['sh', '-c', 'cat "$1" | sh', 'installer-test', str(ROOT / 'install.sh')], self.env)
        output = b''
        try:
            while True:
                try:
                    block = os.read(master, 4096)
                except OSError:
                    break
                if not block:
                    break
                output += block
            _, status = os.waitpid(pid, 0)
            self.assertEqual(os.waitstatus_to_exitcode(status), 0, output)
            self.assertIn(b'INTERACTIVE_SETUP_STARTED', output)
        finally:
            os.close(master)


if __name__ == '__main__':
    unittest.main()
