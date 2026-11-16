import importlib.util
from html.parser import HTMLParser
from pathlib import Path
import shutil
import tempfile
import unittest
from urllib.parse import unquote, urlsplit

SCRIPT = Path(__file__).resolve().parents[1] / 'build-docs.py'
spec = importlib.util.spec_from_file_location('build_docs', SCRIPT)
site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(site)


class Page(HTMLParser):
    def __init__(self, content):
        super().__init__()
        self.links = []
        self.ids = []
        self.feed(content)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs:
            self.ids.append(attrs['id'])
        if tag in ('a', 'link', 'script', 'img'):
            target = attrs.get('href') or attrs.get('src')
            if target:
                self.links.append(target)


class SiteTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'repo'
        self.root.mkdir()
        for directory in ('docs', 'examples', 'site'):
            shutil.copytree(site.ROOT / directory, self.root / directory)
        for file in site.ROOT.iterdir():
            if file.is_file():
                shutil.copyfile(file, self.root / file.name)
        for path in site.PAGES:
            target = self.root / path
            if not target.exists():
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text('# Pending guide\n', encoding='utf-8')
        self.original_root = site.ROOT
        site.ROOT = self.root
        self.addCleanup(setattr, site, 'ROOT', self.original_root)
        self.output = Path(self.tmp.name) / 'host' / 'harness-to-api'

    def test_build_has_valid_relative_links_fragments_and_raw_sources(self):
        site.build(self.output)
        pages = {}
        for file in self.output.rglob('*.html'):
            page = Page(file.read_text(encoding='utf-8'))
            self.assertEqual(len(page.ids), len(set(page.ids)), file)
            pages[file] = page
        for file, page in pages.items():
            for link in page.links:
                parts = urlsplit(link)
                if parts.scheme or parts.netloc:
                    continue
                self.assertFalse(parts.path.startswith('/'), (file, link))
                target = (file.parent / unquote(parts.path)).resolve() if parts.path else file
                self.assertTrue(target.is_file(), (file, link, target))
                if parts.fragment and target in pages:
                    self.assertIn(unquote(parts.fragment), pages[target].ids, (file, link))
        for source in site.PAGES:
            self.assertEqual((self.output / 'raw' / source).read_bytes(), (self.root / source).read_bytes())
            self.assertIn(site.PUBLIC + 'raw/' + source, (self.output / 'llms.txt').read_text())

    def test_missing_pages_fail_and_existing_directory_is_protected(self):
        (self.root / 'docs/getting-started.md').unlink()
        with self.assertRaisesRegex(SystemExit, 'getting-started.md'):
            site.build(self.output)
        self.output.mkdir(parents=True)
        (self.output / 'keep').write_text('safe')
        with self.assertRaisesRegex(SystemExit, 'unmarked'):
            site.build(self.output, allow_missing=True)
        self.assertEqual((self.output / 'keep').read_text(), 'safe')

    def test_raw_html_is_escaped(self):
        (self.root / 'docs/index.md').write_text('# Home\n\n<script>alert(1)</script>\n')
        site.build(self.output)
        rendered = (self.output / 'index.html').read_text()
        self.assertIn('&lt;script&gt;alert(1)&lt;/script&gt;', rendered)
        self.assertNotIn('<script>alert(1)</script>', rendered)


if __name__ == '__main__':
    unittest.main()
