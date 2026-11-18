#!/usr/bin/env python3
"""Build the documentation site from repository Markdown; no network required."""
import argparse
import html
import posixpath
import re
import shutil
from pathlib import Path
from urllib.parse import unquote, urlsplit

from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
REPO = 'https://github.com/royderks/harness-to-api/blob/main/'
PUBLIC = 'https://royderks.github.io/harness-to-api/'
PAGES = {
    'docs/index.md': ('index.html', 'Home'),
    'docs/getting-started.md': ('getting-started/index.html', 'Getting started'),
    'docs/agent-quickstart.md': ('agent-quickstart/index.html', 'Agent quickstart'),
    'examples/README.md': ('examples/index.html', 'Examples'),
    **{f'examples/{name}/README.md': (f'examples/{name}/index.html', title) for name, title in [
        ('tool-calling', 'Client function tools'), ('harness-switch', 'Harness switching'), ('langgraph', 'LangGraph'), ('agents-sdk', 'OpenAI Agents SDK'),
        ('docker', 'Docker'), ('openshell', 'OpenShell'), ('e2b', 'E2B'), ('modal', 'Modal')]},
    **{f'docs/{name}.md': (f'{name}/index.html', title) for name, title in [
        ('api', 'API reference'), ('cli', 'CLI reference'), ('policies', 'Key policies'),
        ('remote-access', 'Remote access'), ('troubleshooting', 'Troubleshooting'),
        ('claude', 'Claude'), ('gemini', 'Gemini'), ('opencode', 'OpenCode'),
        ('ibm-bob', 'IBM Bob'), ('architecture', 'Architecture'), ('onboarding', 'Onboarding'),
        ('openshell', 'OpenShell guide'), ('openshell-live', 'OpenShell advanced'),
        ('deployment', 'Deployment'), ('docker', 'Docker guide'), ('distribution', 'Distribution'),
        ('examples-testing', 'Example verification'), ('verification', 'Verification')]},
}
NAV = [('Start', ['docs/getting-started.md', 'docs/agent-quickstart.md', 'examples/README.md']),
       ('Reference', ['docs/api.md', 'docs/cli.md', 'docs/policies.md', 'docs/remote-access.md', 'docs/troubleshooting.md']),
       ('Providers', ['docs/onboarding.md', 'docs/claude.md', 'docs/gemini.md', 'docs/opencode.md', 'docs/ibm-bob.md']),
       ('More', ['docs/architecture.md', 'docs/deployment.md', 'docs/verification.md'])]


def relative(current, target):
    return posixpath.relpath(target, posixpath.dirname(current) or '.')


def resolve_link(source, output, href):
    parts = urlsplit(href)
    if parts.scheme or parts.netloc or not parts.path:
        return href
    path = posixpath.normpath(posixpath.join(posixpath.dirname(source), unquote(parts.path)))
    if path.startswith('../') or path == '..':
        raise ValueError(f'Link escapes repository in {source}: {href}')
    if (ROOT / path).is_dir() or path.endswith('/'):
        path = posixpath.join(path, 'README.md')
    # Resolve planned pages even before their files arrive.
    if path in PAGES:
        url = relative(output, PAGES[path][0])
    else:
        if not (ROOT / path).is_file():
            raise ValueError(f'Broken link in {source}: {href}')
        url = REPO + path
    return url + (('?' + parts.query) if parts.query else '') + (('#' + parts.fragment) if parts.fragment else '')


def raw_markdown(source):
    """Keep selected Markdown links local; point other repository files to GitHub."""
    text = (ROOT / source).read_text(encoding='utf-8')
    def rewrite(match):
        href = match.group(1)
        parts = urlsplit(href)
        if parts.scheme or parts.netloc or not parts.path:
            return match.group(0)
        path = posixpath.normpath(posixpath.join(posixpath.dirname(source), unquote(parts.path)))
        if path.startswith('../') or path == '..':
            raise ValueError(f'Link escapes repository in {source}: {href}')
        if (ROOT / path).is_dir() or path.endswith('/'):
            path = posixpath.join(path, 'README.md')
        if path in PAGES:
            target = relative('raw/' + source, 'raw/' + path)
        else:
            if not (ROOT / path).is_file():
                raise ValueError(f'Broken link in {source}: {href}')
            target = 'https://raw.githubusercontent.com/royderks/harness-to-api/main/' + path
        target += (('?' + parts.query) if parts.query else '') + (('#' + parts.fragment) if parts.fragment else '')
        return '](' + target + match.group(2) + ')'
    return re.sub(r'\]\(([^)\s]+)([^)]*)\)', rewrite, text)


def slug(text):
    return re.sub(r'[^\w\- ]', '', text.lower()).replace(' ', '-')


def render(md, source, output):
    tokens = md.parse((ROOT / source).read_text(encoding='utf-8'))
    seen = {}
    for index, token in enumerate(tokens):
        if token.type == 'heading_open':
            inline = tokens[index + 1]
            plain = ''.join(child.content for child in (inline.children or []) if child.type in ('text', 'code_inline'))
            base = slug(plain)
            count = seen.get(base, 0)
            seen[base] = count + 1
            token.attrSet('id', base + (f'-{count}' if count else ''))
        if token.children:
            for child in token.children:
                if child.type in ('link_open', 'image'):
                    key = 'href' if child.type == 'link_open' else 'src'
                    child.attrSet(key, resolve_link(source, output, child.attrGet(key)))
    return md.renderer.render(tokens, md.options, {})


def build(destination, allow_missing=False):
    missing = [p for p in PAGES if not (ROOT / p).is_file()]
    if missing and not allow_missing:
        raise SystemExit('Missing required documentation: ' + ', '.join(missing))
    destination = destination.resolve()
    if destination.exists() and any(destination.iterdir()):
        if not (destination / '.h2a-docs-build').is_file():
            raise SystemExit(f'Refusing to replace unmarked output directory: {destination}')
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)
    (destination / '.h2a-docs-build').touch()
    shutil.copytree(ROOT / 'site/assets', destination / 'assets')
    md = MarkdownIt('commonmark', {'html': False}).enable('table')
    for source, (output, title) in PAGES.items():
        if source in missing:
            continue
        content = render(md, source, output)
        nav = ''.join('<div class="nav-group"><p>' + html.escape(group) + '</p>' + ''.join(
            f'<a {"aria-current=page " if page == source else ""}href="{html.escape(relative(output, PAGES[page][0]), quote=True)}">{html.escape(PAGES[page][1])}</a>'
            for page in pages if page not in missing) + '</div>' for group, pages in NAV)
        raw = relative(output, 'raw/' + source)
        canonical = PUBLIC + ('' if output == 'index.html' else output.removesuffix('index.html'))
        page = f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{html.escape(title)} · harness-to-api</title><meta name="description" content="Use installed coding harnesses through an authenticated, OpenAI-compatible text API."><link rel="canonical" href="{canonical}"><meta property="og:title" content="{html.escape(title)} · harness-to-api"><meta property="og:description" content="Your coding harness, available through an API."><meta name="twitter:card" content="summary_large_image"><meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta property="og:image" content="{PUBLIC}assets/social-card.png"><link rel="stylesheet" href="{relative(output, 'assets/site.css')}"><script src="{relative(output, 'assets/site.js')}" defer></script></head><body><a class="skip" href="#main">Skip to content</a><header><a class="brand" href="{relative(output, 'index.html')}"><span class="mark">h2a</span><span>harness-to-api</span></a><a class="github" href="https://github.com/royderks/harness-to-api">GitHub <span aria-hidden="true">↗</span></a></header><div class="layout"><aside><button type="button" class="nav-toggle" aria-expanded="false" aria-controls="docs-nav">Browse documentation</button><nav id="docs-nav" aria-label="Documentation">{nav}</nav></aside><main id="main"><div class="eyebrow">{'Documentation' if source != 'docs/index.md' else 'A local gateway for coding harnesses'}</div><article>{content}</article><div class="page-footer"><a href="{html.escape(raw, quote=True)}">View Markdown</a><button type="button" class="copy-page" data-raw="{html.escape(raw, quote=True)}">Copy Markdown</button><span class="copy-status" role="status" aria-live="polite"></span></div></main></div><footer><span>harness-to-api · open source</span><a href="{relative(output, 'llms.txt')}">For agents: llms.txt</a></footer></body></html>'''
        target = destination / output
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(page, encoding='utf-8')
        raw_target = destination / 'raw' / source
        raw_target.parent.mkdir(parents=True, exist_ok=True)
        raw_target.write_text(raw_markdown(source), encoding='utf-8')
    lines = ['# harness-to-api', '', '> An authenticated, OpenAI-compatible text API for installed coding harnesses.', '', '## Documentation', '']
    lines += [f'- [{title}]({PUBLIC}raw/{source})' for source, (_, title) in PAGES.items() if source not in missing]
    (destination / 'llms.txt').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Built {len(PAGES) - len(missing)} pages in {destination}' + (f' (missing: {", ".join(missing)})' if missing else ''))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / '_site')
    parser.add_argument('--allow-missing', action='store_true', help='development only: allow pages being authored in parallel')
    args = parser.parse_args()
    build(args.output, args.allow_missing)
