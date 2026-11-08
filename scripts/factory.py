#!/usr/bin/env python3
"""Validate and import the factory seed snapshot using the authenticated gh CLI."""
import argparse
import contextlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "factory" / "seeds.json"
STATUSES = {"factory:" + x for x in ("triage", "ready", "running", "review", "blocked")}
PRIORITIES = {"priority:p1", "priority:p2", "priority:p3"}
TYPES = {"type:maintenance", "type:feature", "type:decision"}
MARKER = re.compile(r"<!-- h2a-seed:([a-z0-9]+(?:-[a-z0-9]+)*) -->")
ID = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")


class FactoryError(Exception):
    pass


def fail(message):
    raise FactoryError(message)


def load_manifest():
    try:
        data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        fail(f"cannot read manifest: {e}")
    if not isinstance(data, dict) or set(data) != {"version", "labels", "seeds"} or type(data["version"]) is not int or data["version"] != 1:
        fail("manifest must have version 1, labels, and seeds")
    if not isinstance(data["labels"], list) or not isinstance(data["seeds"], list):
        fail("labels and seeds must be arrays")
    names = set()
    for label in data["labels"]:
        if not isinstance(label, dict) or set(label) != {"name", "color", "description"} or any(not isinstance(label[k], str) or not label[k].strip() for k in label):
            fail("invalid label")
        name = label["name"]
        if name not in STATUSES | PRIORITIES | TYPES or name in names or not re.fullmatch(r"[0-9a-fA-F]{6}", label["color"]):
            fail(f"invalid or duplicate label: {name}")
        names.add(name)
    seeds = {}
    fields = {"id", "title", "intent", "sources", "scope", "acceptance", "verification", "depends_on", "labels", "risk"}
    for seed in data["seeds"]:
        if not isinstance(seed, dict) or set(seed) != fields:
            fail("invalid seed fields")
        if any(not isinstance(seed[k], str) or not seed[k].strip() for k in ("id", "title", "intent", "risk")) or not ID.fullmatch(seed["id"]) or seed["id"] in seeds:
            fail("invalid or duplicate seed ID or text")
        for key in ("sources", "scope", "acceptance", "verification", "depends_on", "labels"):
            if not isinstance(seed[key], list) or any(not isinstance(v, str) or not v.strip() for v in seed[key]) or len(set(seed[key])) != len(seed[key]):
                fail(f"invalid {key} in {seed['id']}")
        text_fields = [text for value in seed.values() for text in (value if isinstance(value, list) else [value])]
        if any("<!-- h2a-seed:" in text for text in text_fields):
            fail(f"reserved publication marker in seed text: {seed['id']}")
        if any(not seed[k] for k in ("sources", "scope", "acceptance", "verification")):
            fail(f"empty required list in {seed['id']}")
        if seed["risk"] not in ("low", "medium", "high") or not set(seed["labels"]) <= names or len(set(seed["labels"]) & STATUSES) != 1 or len(set(seed["labels"]) & PRIORITIES) != 1:
            fail(f"invalid risk or labels in {seed['id']}")
        for source in seed["sources"]:
            if source.startswith("https://"):
                continue
            path = source.split("#", 1)[0]
            resolved = (ROOT / path).resolve()
            if not path or not resolved.is_relative_to(ROOT.resolve()) or not resolved.is_file():
                fail(f"invalid source: {source}")
        seeds[seed["id"]] = seed
    ordered = []
    visiting, done = set(), set()
    def visit(key):
        if key in visiting:
            fail("dependency cycle")
        if key in done:
            return
        if key not in seeds:
            fail(f"unknown dependency: {key}")
        visiting.add(key)
        for dep in seeds[key]["depends_on"]:
            visit(dep)
        visiting.remove(key)
        done.add(key)
        ordered.append(seeds[key])
    for key in seeds:
        visit(key)
    return data, ordered


def gh(args, payload=None, paginated=False):
    command = ["gh", "api"] + args
    if paginated:
        command += ["--paginate", "--slurp"]
    if payload is not None:
        command += ["--input", "-"]
    try:
        result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None, text=True, capture_output=True, check=False, timeout=60)
    except subprocess.TimeoutExpired:
        fail("gh API request timed out; inspect pending publications before retrying")
    except OSError as e:
        fail(f"cannot run gh: {e}")
    if result.returncode:
        fail("gh API request failed; inspect gh authentication and repository access")
    try:
        value = json.loads(result.stdout)
        if paginated:
            if (not isinstance(value, list) or not value or any(not isinstance(page, list) or len(page) > 100 for page in value)
                    or any(len(page) != 100 for page in value[:-1])):
                raise ValueError("invalid pages")
            return [item for page in value for item in page]
        return value
    except ValueError:
        fail("gh returned incomplete or invalid JSON")


def common_dir():
    try:
        result = subprocess.run(["git", "rev-parse", "--git-common-dir"], cwd=ROOT, text=True, capture_output=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        fail("cannot locate Git common directory")
    path = Path(result.stdout.strip())
    return (ROOT / path).resolve()


@contextlib.contextmanager
def lock(path):
    with open(path / "h2a-factory.lock", "a+b") as f:
        if os.name == "nt":
            import msvcrt
            f.seek(0)
            if not f.read(1):
                f.write(b"0")
                f.flush()
            f.seek(0)
            try:
                msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError:
                fail("another factory publisher holds the lock")
        else:
            import fcntl
            try:
                fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError:
                fail("another factory publisher holds the lock")
        try:
            yield
        finally:
            if os.name == "nt":
                f.seek(0)
                msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(f, fcntl.LOCK_UN)


def read_journal(path, repo):
    if not path.exists():
        return {"version": 1, "repo_id": repo["id"], "repo": repo["full_name"], "seeds": {}}
    try:
        journal = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(journal, dict) or set(journal) != {"version", "repo_id", "repo", "seeds"} or type(journal["version"]) is not int or journal["version"] != 1 or type(journal["repo_id"]) is not int or not isinstance(journal["repo"], str) or not isinstance(journal["seeds"], dict):
            raise ValueError()
        if journal["repo_id"] != repo["id"]:
            fail("journal belongs to another repository")
        seen = set()
        for key, value in journal["seeds"].items():
            if not ID.fullmatch(key) or not isinstance(value, dict) or (value != {"state": "pending"} and not (set(value) == {"state", "number"} and value["state"] == "confirmed" and type(value["number"]) is int and value["number"] > 0 and value["number"] not in seen)):
                raise ValueError()
            if value["state"] == "confirmed":
                seen.add(value["number"])
        return journal
    except (OSError, ValueError, TypeError):
        fail("corrupt factory journal")


def save_journal(path, journal):
    fd, temp = tempfile.mkstemp(prefix="h2a-factory-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(journal, f, sort_keys=True)
            f.flush()
            os.fsync(f.fileno())
        os.replace(temp, path)
        if os.name != "nt":
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def inventory(target):
    repo = gh([f"repos/{target}"])
    if not isinstance(repo, dict) or type(repo.get("private")) is not bool or type(repo.get("id")) is not int or repo.get("id", 0) <= 0 or not isinstance(repo.get("full_name"), str) or not isinstance(repo.get("default_branch"), str) or not repo["default_branch"] or repo["full_name"].lower() != target.lower():
        fail("target must be the specified public or private repository")
    issues = gh([f"repos/{target}/issues?state=all&per_page=100"], paginated=True)
    labels = gh([f"repos/{target}/labels?per_page=100"], paginated=True)
    if any(not isinstance(i, dict) or type(i.get("number")) is not int or i["number"] <= 0 or not isinstance(i.get("body"), (str, type(None))) or not isinstance(i.get("state"), str) or not isinstance(i.get("html_url"), str) for i in issues) or any(not isinstance(x, dict) or not isinstance(x.get("name"), str) for x in labels):
        fail("incomplete issue or label inventory")
    all_numbers = [i["number"] for i in issues]
    if len(set(all_numbers)) != len(all_numbers):
        fail("duplicate issue in inventory")
    issues = [i for i in issues if "pull_request" not in i]
    by_number, markers = {}, {}
    for issue in issues:
        number = issue["number"]
        if number in by_number:
            fail("duplicate issue in inventory")
        by_number[number] = issue
        for key in MARKER.findall(issue.get("body") or ""):
            markers.setdefault(key, set()).add(number)
    return repo, by_number, markers, {x["name"] for x in labels}, set(all_numbers)


def reconcile(journal, issues, allow_pending=False):
    mapped = {}
    for key, entry in journal["seeds"].items():
        if entry["state"] == "pending":
            if allow_pending:
                continue
            fail(f"pending seed {key}; use recover after inspecting GitHub")
        number = entry["number"]
        if number not in issues:
            fail(f"mapped issue #{number} is missing")
        mapped[key] = number
    return mapped


def show_candidates(target, key, markers):
    numbers = sorted(markers.get(key, set()))
    if numbers:
        print(f"{key}: untrusted candidates " + ", ".join(f"#{n}" for n in numbers))
        print(f"  Inspect, then adopt: python3 scripts/factory.py adopt --repo {target} --id {key} --issue NUMBER")
        print(f"  Or assert absence: python3 scripts/factory.py seed --repo {target} --apply --confirm-absent {key}")


def body(seed, mapped, issues, repo):
    def bullets(items):
        return "\n".join("- " + x for x in items)
    deps = [issues[mapped[x]]["html_url"] for x in seed["depends_on"]]
    sources = []
    for source in seed["sources"]:
        if source.startswith("https://"):
            sources.append(source)
        else:
            path, separator, anchor = source.partition("#")
            url = f"https://github.com/{repo['full_name']}/blob/{quote(repo['default_branch'], safe='')}/{quote(path, safe='/')}"
            sources.append(f"[{source}]({url}{separator}{anchor})")
    return (f"<!-- h2a-seed:{seed['id']} -->\n\n## Intent\n\n{seed['intent']}\n\n"
            f"## Sources\n\n{bullets(sources)}\n\n## Scope\n\n{bullets(seed['scope'])}\n\n"
            f"## Acceptance\n\n{bullets(seed['acceptance'])}\n\n## Verification\n\n{bullets(seed['verification'])}\n\n"
            f"## Dependencies\n\n{bullets(deps) if deps else 'None.'}\n\n## Risk\n\n{seed['risk']}\n")


def run(args):
    manifest, seeds = load_manifest()
    if args.command == "validate":
        print(f"Valid manifest: {len(seeds)} seeds, {len(manifest['labels'])} labels")
        return
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo):
        fail("--repo must be owner/name")
    directory = common_dir()
    journal_path = directory / "h2a-factory-journal.json"
    mutating = args.command in ("recover", "adopt") or (args.command == "seed" and args.apply)
    with lock(directory) if mutating else contextlib.nullcontext():
        repo, issues, markers, labels, all_numbers = inventory(args.repo)
        journal = read_journal(journal_path, repo)
        known = {s["id"] for s in seeds}
        mapped = reconcile(journal, issues, allow_pending=True)
        if args.command in ("recover", "adopt"):
            entry = journal["seeds"].get(args.id)
            if args.id not in known:
                fail("unknown seed ID")
            if args.command == "recover" and entry != {"state": "pending"}:
                fail("recovery requires a pending seed")
            if args.command == "adopt" and entry is not None:
                fail("adoption requires an unmapped seed")
            show_candidates(args.repo, args.id, markers)
            if args.command == "recover" and args.confirm_absent:
                del journal["seeds"][args.id]
                save_journal(journal_path, journal)
                print(f"Cleared {args.id} after operator-confirmed absence; no issue was created")
                return
            if args.issue not in issues:
                fail("selected issue does not exist or is a pull request")
            if args.issue in mapped.values():
                fail("issue is already mapped")
            journal["seeds"][args.id] = {"state": "confirmed", "number": args.issue}
            save_journal(journal_path, journal)
            print(f"{'Recovered' if args.command == 'recover' else 'Adopted'} {args.id} as #{args.issue}")
            return
        assertions = args.confirm_absent if args.command == "seed" else []
        if assertions and not args.apply:
            fail("--confirm-absent requires --apply")
        if len(set(assertions)) != len(assertions):
            fail("duplicate --confirm-absent ID")
        for key in assertions:
            if key not in known or key in journal["seeds"] or not markers.get(key):
                fail(f"invalid --confirm-absent ID: {key}; expected an unmapped seed with candidates")

        for label in manifest["labels"]:
            if label["name"] not in labels:
                print(f"Missing label {label['name']}")
        for seed in seeds:
            key = seed["id"]
            if journal["seeds"].get(key, {}).get("state") == "pending":
                print(f"{key}: pending recovery")
                show_candidates(args.repo, key, markers)
            elif key in mapped:
                issue = issues[mapped[key]]
                print(f"{key}: #{issue['number']} {issue['state']} {issue['html_url']}")
            else:
                print(f"{key}: missing")
                show_candidates(args.repo, key, markers)
        if not mutating:
            return
        reconcile(journal, issues)
        blocked = [s["id"] for s in seeds if s["id"] not in mapped and markers.get(s["id"]) and s["id"] not in assertions]
        if blocked:
            fail("untrusted candidates require adoption or --confirm-absent: " + ", ".join(blocked))
        if not isinstance(repo.get("permissions"), dict) or repo["permissions"].get("push") is not True:
            fail("seed --apply requires repository push permission")
        for label in manifest["labels"]:
            if label["name"] not in labels:
                gh([f"repos/{args.repo}/labels", "--method", "POST"], label)
        for seed in seeds:
            key = seed["id"]
            if key in mapped:
                continue
            journal["seeds"][key] = {"state": "pending"}
            save_journal(journal_path, journal)
            issue = gh([f"repos/{args.repo}/issues", "--method", "POST"], {"title": seed["title"], "body": body(seed, mapped, issues, repo), "labels": seed["labels"]})
            if not isinstance(issue, dict) or type(issue.get("number")) is not int or issue["number"] <= 0 or not isinstance(issue.get("html_url"), str):
                fail(f"uncertain create for {key}; inspect GitHub and recover")
            number = issue["number"]
            if number in all_numbers:
                fail(f"uncertain create for {key}; duplicate issue number")
            journal["seeds"][key] = {"state": "confirmed", "number": number}
            save_journal(journal_path, journal)
            all_numbers.add(number)
            mapped[key], issues[number] = number, issue
            print(f"Created {key}: {issue['html_url']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog="Retain the Git common directory journal. If a create result is uncertain, inspect GitHub and recover its issue before seeding again. One publisher across machines is required.")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("validate", help="validate the manifest offline")
    for name in ("plan", "seed", "recover", "adopt"):
        sub = commands.add_parser(name)
        sub.add_argument("--repo", required=True, help="repository owner/name")
        if name == "seed":
            sub.add_argument("--apply", action="store_true", help="create missing labels and issues")
            sub.add_argument("--confirm-absent", action="append", default=[], metavar="ID", help="assert no candidate is the previously published issue")
        if name == "adopt":
            sub.add_argument("--id", required=True, help="unmapped seed ID")
            sub.add_argument("--issue", type=int, required=True, help="existing issue number")
        if name == "recover":
            sub.add_argument("--id", required=True, help="pending seed ID")
            recovery = sub.add_mutually_exclusive_group(required=True)
            recovery.add_argument("--issue", type=int, help="existing issue number")
            recovery.add_argument("--confirm-absent", action="store_true", help="clear a pending attempt only after confirming the request settled without creating an issue")
    args = parser.parse_args()
    try:
        run(args)
    except (FactoryError, OSError) as e:
        print(f"factory: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
