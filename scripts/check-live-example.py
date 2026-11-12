#!/usr/bin/env python3
"""Opt-in live example checks. Reports contain metadata, never model output or secrets."""
import argparse
from datetime import datetime, timezone
import importlib.metadata
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ('harness-switch', 'langgraph', 'agents-sdk', 'docker', 'e2b', 'modal', 'openshell')


def prerequisites(example, env):
    missing = [name for name in ('OPENAI_BASE_URL', 'OPENAI_API_KEY', 'H2A_MODEL') if not env.get(name)]
    if example == 'e2b' and not env.get('E2B_API_KEY'):
        missing.append('E2B_API_KEY')
    if example == 'modal':
        missing += [name for name in ('MODAL_TOKEN_ID', 'MODAL_TOKEN_SECRET') if not env.get(name)]
    if example in ('docker', 'openshell') and not shutil.which(example):
        missing.append(example + ' executable')
    if example == 'openshell' and not env.get('OPENSHELL_GATEWAY_ENDPOINT'):
        missing.append('OPENSHELL_GATEWAY_ENDPOINT')
    return missing


def metadata(example):
    versions = {}
    for package in ('openai', 'openai-agents', 'langgraph', 'e2b', 'modal'):
        try:
            versions[package] = importlib.metadata.version(package)
        except importlib.metadata.PackageNotFoundError:
            versions[package] = None
    commit = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
    return {'schema_version': 1, 'example': example, 'checked_at': datetime.now(timezone.utc).isoformat(),
            'source_commit': commit, 'sdk_versions': versions, 'status': 'not_run', 'last_success_at': None}


def run_check(example, env):
    model = env['H2A_MODEL']
    if not re.fullmatch(r'(codex|antigravity|ibm-bob|gemini|claude|opencode)/[A-Za-z0-9_./:-]+', model):
        raise ValueError('Invalid model')
    sys.path.insert(0, str(ROOT / 'examples'))
    from _client import config
    config(remote=True)
    env = dict(env, H2A_MODELS=model, H2A_REVIEW_MODEL=model, LANGSMITH_TRACING='false',
               LANGCHAIN_TRACING_V2='false', OPENAI_AGENTS_DISABLE_TRACING='1')
    command = [sys.executable, str(ROOT / 'examples' / example / 'main.py')]
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, timeout=420)
    if result.returncode != 0 or not result.stdout.strip():
        raise RuntimeError('Example failed')


def save_report(path, report):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--example', choices=EXAMPLES, required=True)
    parser.add_argument('--allow-live', action='store_true', help='Permit provider requests and selected sandbox resource creation')
    parser.add_argument('--report', type=Path, default=Path('artifacts/live-example.json'))
    args = parser.parse_args(argv)
    report = metadata(args.example)
    code = 2
    if not args.allow_live:
        report['reason'] = 'Explicit --allow-live is required'
    else:
        missing = prerequisites(args.example, os.environ)
        if missing:
            report['reason'] = 'Missing prerequisites: ' + ', '.join(missing)
        else:
            report['status'] = 'incomplete'
            report['reason'] = 'Execution started but no final result was recorded'
            save_report(args.report, report)
            try:
                run_check(args.example, os.environ)
                report.pop('reason', None)
                report['model'] = os.environ['H2A_MODEL']
                report['status'] = 'passed'
                report['last_success_at'] = datetime.now(timezone.utc).isoformat()
                code = 0
            except Exception:
                report['status'] = 'failed'
                report['reason'] = 'Live check failed; inspect configuration and quota privately'
                code = 1
    save_report(args.report, report)
    print(json.dumps(report))
    return code


if __name__ == '__main__':
    sys.exit(main())
