"""Offline lifecycle checks using an actual fake OpenShell executable."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('openshell_lifecycle', ROOT / 'examples/openshell/main.py')
openshell = importlib.util.module_from_spec(spec)
spec.loader.exec_module(openshell)

FAKE = '''#!/usr/bin/env python3
import os
from pathlib import Path
import subprocess
import sys
import time
args = sys.argv[1:]
with open(os.environ['FAKE_LOG'], 'a') as log:
    log.write(' '.join(args[:2]) + "\\n")
if args[:2] == ['sandbox', 'delete'] and os.environ['FAKE_MODE'] == 'delete_timeout':
    time.sleep(30)
if args[:2] == ['sandbox', 'create']:
    mode = os.environ['FAKE_MODE']
    if mode == 'timeout':
        child = subprocess.Popen([sys.executable, '-c', "import time; from pathlib import Path; time.sleep(1.5); Path(" + repr(os.environ['FAKE_SURVIVED']) + ").write_text('alive')"])
        Path(os.environ['FAKE_PID']).write_text(str(child.pid))
        time.sleep(30)
    elif mode == 'status':
        print('sandbox created successfully')
    else:
        source = args[args.index('-c') + 1]
        exec(source.splitlines()[-1])
'''


class OpenShellLifecycle(unittest.TestCase):
    def invoke(self, mode):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            executable = temp / 'openshell'
            executable.write_text(FAKE)
            executable.chmod(0o755)
            env = dict(os.environ, PATH=str(temp) + os.pathsep + os.environ.get('PATH', ''),
                       OPENAI_BASE_URL='https://example.invalid/v1', OPENAI_API_KEY='test-key',
                       FAKE_LOG=str(temp / 'log'), FAKE_MODE=mode,
                       FAKE_SURVIVED=str(temp / 'survived'), FAKE_PID=str(temp / 'pid'))
            with patch.dict(os.environ, env, clear=True), patch.object(openshell, 'WORK_SECONDS', 0.7), \
                 patch.object(openshell, 'COMMAND_SECONDS', 0.7), \
                 patch.object(openshell, 'CLEANUP_SECONDS', 0.3), \
                 contextlib.redirect_stdout(io.StringIO()):
                if mode == 'success':
                    openshell.main()
                else:
                    with self.assertRaises((RuntimeError, subprocess.TimeoutExpired)):
                        openshell.main()
            calls = (temp / 'log').read_text().splitlines()
            self.assertEqual(calls[-3:], ['sandbox delete', 'provider delete', 'profile delete'])
            if mode == 'timeout':
                self.assertTrue((temp / 'pid').exists())
                time.sleep(1.6)
                self.assertFalse((temp / 'survived').exists(), 'descendant survived command timeout')

    @unittest.skipUnless(os.name == 'posix', 'POSIX process-group cleanup')
    def test_timeout_stops_descendant_and_cleans_resources(self):
        self.invoke('timeout')

    def test_status_only_is_not_a_completion(self):
        self.invoke('status')

    def test_cleanup_continues_after_delete_timeout(self):
        self.invoke('delete_timeout')

    def test_completion_marker_allows_success(self):
        self.invoke('success')


if __name__ == '__main__':
    unittest.main()
