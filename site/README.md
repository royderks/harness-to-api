# Documentation site

The site is generated from the Markdown files listed in `scripts/build-docs.py`.
The default build requires every listed page, including the getting started,
agent quickstart, and troubleshooting guides. It does not fetch content or run
commands from Markdown. Raw HTML is escaped.

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-docs.txt
python scripts/build-docs.py
python -m unittest discover -s scripts/docs_tests -v
python -m http.server --directory _site 8000
```

Open `http://localhost:8000/`. For a project-subpath check, build to a directory
named `harness-to-api` inside a temporary parent and serve that parent:

```sh
mkdir -p /tmp/h2a-docs-preview
python scripts/build-docs.py --output /tmp/h2a-docs-preview/harness-to-api
python -m http.server --directory /tmp/h2a-docs-preview 8000
```

Open `http://localhost:8000/harness-to-api/`. `--allow-missing` is only for
local development while required guides are being authored; omit it for a
publishable build. The builder only replaces a nonempty output directory when
it contains its own `.h2a-docs-build` marker.

`assets/social-card.svg` is a 1200 × 630 vector card used in page metadata. To
set a repository social preview, export it to a 1200 × 630 PNG and upload it
through the repository's GitHub settings. This is a separate manual action.
