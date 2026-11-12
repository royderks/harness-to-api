import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    path = ROOT / 'scripts' / (name + '.py')
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


live = load('check-live-example')


class LiveChecks(unittest.TestCase):
    def invoke(self, env, consent=True):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, env, clear=True), contextlib.redirect_stdout(io.StringIO()) as out:
            path = Path(temp) / 'result.json'
            args = ['--example', 'e2b', '--report', str(path)]
            if consent:
                args += ['--allow-live']
            code = live.main(args)
            return code, json.loads(path.read_text()), out.getvalue()

    def test_missing_accounts_do_not_execute_or_claim_success(self):
        with patch.object(live, 'run_check') as run:
            code, report, _ = self.invoke({})
        self.assertEqual(code, 2)
        self.assertEqual(report['status'], 'not_run')
        self.assertIsNone(report['last_success_at'])
        run.assert_not_called()

    def test_consent_required_even_with_credentials(self):
        with patch.object(live, 'run_check') as run:
            code, report, _ = self.invoke(self.env(), consent=False)
        self.assertEqual(code, 2)
        run.assert_not_called()
        self.assertEqual(report['status'], 'not_run')

    def env(self):
        return {'OPENAI_BASE_URL': 'https://example.invalid/v1', 'OPENAI_API_KEY': 'private-key',
                'H2A_MODEL': 'codex/default', 'E2B_API_KEY': 'private-cloud-key'}

    def test_failure_output_and_evidence_are_sanitized(self):
        with patch.object(live, 'run_check', side_effect=RuntimeError('private-key private-cloud-key private model output')):
            code, report, output = self.invoke(self.env())
        self.assertEqual(code, 1)
        self.assertEqual(report['status'], 'failed')
        self.assertIsNone(report['last_success_at'])
        self.assertNotIn('private-key', output)
        self.assertNotIn('private-cloud-key', output)
        self.assertNotIn('private model output', output)

    def test_success_has_timestamp_model_and_source_identity(self):
        with patch.object(live, 'run_check'):
            code, report, _ = self.invoke(self.env())
        self.assertEqual(code, 0)
        self.assertEqual(report['status'], 'passed')
        self.assertIsNotNone(report['last_success_at'])
        self.assertEqual(report['model'], 'codex/default')
        self.assertRegex(report['source_commit'], r'^[a-f0-9]{40}$')

    def test_invalid_url_or_model_prevents_child(self):
        for change in ({'OPENAI_BASE_URL': 'http://public.example/v1'}, {'H2A_MODEL': 'invalid model'}):
            env = dict(self.env(), **change)
            with patch.dict(os.environ, env, clear=True), patch.object(live.subprocess, 'run') as run:
                with self.assertRaises(ValueError):
                    live.run_check('e2b', env)
                run.assert_not_called()

    def test_interrupt_retains_incomplete_evidence(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, self.env(), clear=True):
            path = Path(temp) / 'result.json'
            with patch.object(live, 'run_check', side_effect=KeyboardInterrupt):
                with self.assertRaises(KeyboardInterrupt):
                    live.main(['--example', 'e2b', '--allow-live', '--report', str(path)])
            report = json.loads(path.read_text())
            self.assertEqual(report['status'], 'incomplete')
            self.assertIsNone(report['last_success_at'])

    def test_workflow_records_not_run_before_checkout(self):
        workflow = (ROOT / '.github/workflows/examples-live.yml').read_text()
        start = workflow.index('      - name: Record attempt before setup')
        end = workflow.index('      - uses: actions/checkout', start)
        block = workflow[start:end].split('        run: |\n', 1)[1]
        with tempfile.TemporaryDirectory() as temp:
            env = dict(os.environ, RUNNER_TEMP=temp, LIVE_EXAMPLE='e2b', SOURCE_COMMIT='a' * 40,
                       OPENAI_API_KEY='must-not-appear')
            subprocess.run(['bash', '-eu', '-c', textwrap.dedent(block)], env=env, check=True)
            text = (Path(temp) / 'h2a-live-example.json').read_text()
            report = json.loads(text)
            self.assertEqual(report['status'], 'not_run')
            self.assertEqual(report['source_commit'], 'a' * 40)
            self.assertIsNone(report['last_success_at'])
            self.assertNotIn('must-not-appear', text)


if __name__ == '__main__':
    unittest.main()
