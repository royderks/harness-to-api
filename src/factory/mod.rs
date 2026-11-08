use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn fields(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
fn id(s: &str) -> bool {
    Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
        .unwrap()
        .is_match(s)
}
fn root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    cwd.ancestors()
        .find(|p| p.join("factory/seeds.json").is_file())
        .map(Path::to_path_buf)
        .context("cannot locate factory/seeds.json in current directory or ancestors")
}
fn strings<'a>(v: &'a Value, key: &str) -> Result<Vec<&'a str>> {
    let a = v
        .get(key)
        .and_then(Value::as_array)
        .context("invalid array")?;
    let mut seen = HashSet::new();
    a.iter()
        .map(|x| {
            let s = x
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("invalid array entry")?;
            if !seen.insert(s) {
                bail!("duplicate array entry");
            }
            Ok(s)
        })
        .collect()
}
fn load(root: &Path) -> Result<(Value, Vec<Value>)> {
    let data: Value = serde_json::from_slice(
        &fs::read(root.join("factory/seeds.json")).context("cannot read manifest")?,
    )
    .context("cannot read manifest")?;
    if !fields(&data, &["version", "labels", "seeds"]) || data["version"].as_u64() != Some(1) {
        bail!("manifest must have version 1, labels, and seeds");
    }
    let labels = data["labels"]
        .as_array()
        .context("labels and seeds must be arrays")?;
    let seeds = data["seeds"]
        .as_array()
        .context("labels and seeds must be arrays")?;
    let statuses = [
        "factory:triage",
        "factory:ready",
        "factory:running",
        "factory:review",
        "factory:blocked",
    ];
    let priorities = ["priority:p1", "priority:p2", "priority:p3"];
    let types = ["type:maintenance", "type:feature", "type:decision"];
    let mut names = HashSet::new();
    for label in labels {
        if !fields(label, &["name", "color", "description"])
            || ["name", "color", "description"]
                .iter()
                .any(|k| label[*k].as_str().is_none_or(|s| s.trim().is_empty()))
        {
            bail!("invalid label");
        }
        let name = text(label, "name");
        let color = text(label, "color");
        if !statuses.contains(&name) && !priorities.contains(&name) && !types.contains(&name)
            || !names.insert(name)
            || color.len() != 6
            || !color.bytes().all(|c| c.is_ascii_hexdigit())
        {
            bail!("invalid or duplicate label: {name}");
        }
    }
    let canonical = root.canonicalize()?;
    let mut by_id = HashMap::new();
    for seed in seeds {
        if !fields(
            seed,
            &[
                "id",
                "title",
                "intent",
                "sources",
                "scope",
                "acceptance",
                "verification",
                "depends_on",
                "labels",
                "risk",
            ],
        ) {
            bail!("invalid seed fields");
        }
        let key = text(seed, "id");
        if ["id", "title", "intent", "risk"]
            .iter()
            .any(|k| seed[*k].as_str().is_none_or(|s| s.trim().is_empty()))
            || !id(key)
            || by_id.insert(key, seed).is_some()
        {
            bail!("invalid or duplicate seed ID or text");
        }
        for k in [
            "sources",
            "scope",
            "acceptance",
            "verification",
            "depends_on",
            "labels",
        ] {
            let a = strings(seed, k).with_context(|| format!("invalid {k} in {key}"))?;
            if ["sources", "scope", "acceptance", "verification"].contains(&k) && a.is_empty() {
                bail!("empty required list in {key}");
            }
        }
        if seed.as_object().unwrap().values().any(|v| {
            if let Some(a) = v.as_array() {
                a.iter()
                    .any(|x| x.as_str().unwrap_or("").contains("<!-- h2a-seed:"))
            } else {
                v.as_str().unwrap_or("").contains("<!-- h2a-seed:")
            }
        }) {
            bail!("reserved publication marker in seed text: {key}");
        }
        let ls = strings(seed, "labels")?;
        if !["low", "medium", "high"].contains(&text(seed, "risk"))
            || ls.iter().any(|s| !names.contains(s))
            || ls.iter().filter(|s| statuses.contains(s)).count() != 1
            || ls.iter().filter(|s| priorities.contains(s)).count() != 1
        {
            bail!("invalid risk or labels in {key}");
        }
        for source in strings(seed, "sources")? {
            if !source.starts_with("https://") {
                let path = source.split('#').next().unwrap();
                let resolved = root
                    .join(path)
                    .canonicalize()
                    .with_context(|| format!("invalid source: {source}"))?;
                if path.is_empty() || !resolved.starts_with(&canonical) || !resolved.is_file() {
                    bail!("invalid source: {source}");
                }
            }
        }
    }
    fn visit<'a>(
        key: &'a str,
        all: &HashMap<&'a str, &'a Value>,
        visiting: &mut HashSet<&'a str>,
        done: &mut HashSet<&'a str>,
        out: &mut Vec<Value>,
    ) -> Result<()> {
        if visiting.contains(key) {
            bail!("dependency cycle");
        }
        if done.contains(key) {
            return Ok(());
        }
        let seed = all
            .get(key)
            .with_context(|| format!("unknown dependency: {key}"))?;
        visiting.insert(key);
        for dep in strings(seed, "depends_on")? {
            visit(dep, all, visiting, done, out)?;
        }
        visiting.remove(key);
        done.insert(key);
        out.push((*seed).clone());
        Ok(())
    }
    let mut ordered = Vec::new();
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    for seed in seeds {
        visit(
            text(seed, "id"),
            &by_id,
            &mut visiting,
            &mut done,
            &mut ordered,
        )?;
    }
    Ok((data, ordered))
}

fn gh(args: &[String], payload: Option<&Value>, paginated: bool) -> Result<Value> {
    let mut cmd = Command::new("gh");
    cmd.arg("api")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(if payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    if paginated {
        cmd.args(["--paginate", "--slurp"]);
    }
    if payload.is_some() {
        cmd.args(["--input", "-"]);
    }
    let mut child = cmd.spawn().context("cannot run gh")?;
    let stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.take(32 * 1024 * 1024).read_to_end(&mut b);
        b
    });
    if let Some(value) = payload {
        let mut stdin = child.stdin.take().unwrap();
        let bytes = serde_json::to_vec(value)?;
        thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < Duration::from_secs(60) => {
                thread::sleep(Duration::from_millis(20))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("gh API request timed out; inspect pending publications before retrying");
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("gh API request failed; inspect gh authentication and repository access");
            }
        }
    };
    while !reader.is_finished() {
        if start.elapsed() >= Duration::from_secs(60) {
            bail!("gh API request timed out; inspect pending publications before retrying");
        }
        thread::sleep(Duration::from_millis(20));
    }
    let output = reader.join().unwrap_or_default();
    if !status.success() {
        bail!("gh API request failed; inspect gh authentication and repository access");
    }
    let value: Value =
        serde_json::from_slice(&output).context("gh returned incomplete or invalid JSON")?;
    if !paginated {
        return Ok(value);
    }
    let pages = value
        .as_array()
        .filter(|p| !p.is_empty())
        .context("gh returned incomplete or invalid JSON")?;
    let mut flat = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let a = page
            .as_array()
            .context("gh returned incomplete or invalid JSON")?;
        if a.len() > 100 || (i + 1 < pages.len() && a.len() != 100) {
            bail!("gh returned incomplete or invalid JSON");
        }
        flat.extend(a.iter().cloned());
    }
    Ok(Value::Array(flat))
}
fn endpoint(s: String, payload: Option<&Value>, paginated: bool) -> Result<Value> {
    let mut args = vec![s];
    if payload.is_some() {
        args.extend(["--method".into(), "POST".into()]);
    }
    gh(&args, payload, paginated)
}
fn common_dir(root: &Path) -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(root)
        .output()
        .context("cannot locate Git common directory")?;
    if !out.status.success() {
        bail!("cannot locate Git common directory");
    }
    Ok(root
        .join(String::from_utf8_lossy(&out.stdout).trim())
        .canonicalize()?)
}
struct Lock {
    _file: File,
}
impl Lock {
    fn acquire(dir: &Path) -> Result<Self> {
        let f = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(dir.join("h2a-factory.lock"))?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                bail!("another factory publisher holds the lock");
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
            };
            use windows_sys::Win32::System::IO::OVERLAPPED;
            let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
            if unsafe {
                LockFileEx(
                    f.as_raw_handle() as _,
                    LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                    0,
                    1,
                    0,
                    &mut ov,
                )
            } == 0
            {
                bail!("another factory publisher holds the lock");
            }
        }
        Ok(Self { _file: f })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self._file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}
fn journal(path: &Path, repo: &Value) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({"version":1,"repo_id":repo["id"],"repo":repo["full_name"],"seeds":{}}));
    }
    let j: Value = serde_json::from_slice(&fs::read(path).context("corrupt factory journal")?)
        .context("corrupt factory journal")?;
    if !fields(&j, &["version", "repo_id", "repo", "seeds"])
        || j["version"].as_u64() != Some(1)
        || j["repo_id"].as_i64().is_none()
        || !j["repo"].is_string()
        || !j["seeds"].is_object()
    {
        bail!("corrupt factory journal");
    }
    if j["repo_id"] != repo["id"] {
        bail!("journal belongs to another repository");
    }
    let mut seen = HashSet::new();
    for (k, v) in j["seeds"].as_object().unwrap() {
        if !id(k)
            || !(v == &json!({"state":"pending"})
                || fields(v, &["state", "number"])
                    && text(v, "state") == "confirmed"
                    && v["number"]
                        .as_i64()
                        .is_some_and(|n| n > 0 && seen.insert(n)))
        {
            bail!("corrupt factory journal");
        }
    }
    Ok(j)
}
fn save(path: &Path, j: &Value) -> Result<()> {
    let parent = path.parent().unwrap();
    let mut f = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut f, j)?;
    f.flush()?;
    f.as_file().sync_all()?;
    f.persist(path)
        .map_err(|_| anyhow::anyhow!("cannot persist factory journal"))?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
struct Inventory {
    repo: Value,
    issues: HashMap<i64, Value>,
    markers: HashMap<String, BTreeSet<i64>>,
    labels: HashSet<String>,
    numbers: HashSet<i64>,
}
fn inventory(target: &str) -> Result<Inventory> {
    let repo = endpoint(format!("repos/{target}"), None, false)?;
    if !repo.is_object()
        || !repo["private"].is_boolean()
        || !repo["id"].as_i64().is_some_and(|n| n > 0)
        || !repo["full_name"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(target))
        || !repo["default_branch"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    {
        bail!("target must be the specified public or private repository");
    }
    let issues = endpoint(
        format!("repos/{target}/issues?state=all&per_page=100"),
        None,
        true,
    )?;
    let labels = endpoint(format!("repos/{target}/labels?per_page=100"), None, true)?;
    let mut inv = Inventory {
        repo,
        issues: HashMap::new(),
        markers: HashMap::new(),
        labels: HashSet::new(),
        numbers: HashSet::new(),
    };
    let marker = Regex::new(r"<!-- h2a-seed:([a-z0-9]+(?:-[a-z0-9]+)*) -->").unwrap();
    for x in issues.as_array().unwrap() {
        let n = x["number"]
            .as_i64()
            .filter(|n| *n > 0)
            .context("incomplete issue or label inventory")?;
        if !x.is_object()
            || !(x["body"].is_string() || x.get("body").is_some_and(Value::is_null))
            || !x["state"].is_string()
            || !x["html_url"].is_string()
        {
            bail!("incomplete issue or label inventory");
        }
        if !inv.numbers.insert(n) {
            bail!("duplicate issue in inventory");
        }
        if x.get("pull_request").is_some() {
            continue;
        }
        for cap in marker.captures_iter(text(x, "body")) {
            inv.markers.entry(cap[1].to_string()).or_default().insert(n);
        }
        inv.issues.insert(n, x.clone());
    }
    for x in labels.as_array().unwrap() {
        let s = x
            .get("name")
            .and_then(Value::as_str)
            .context("incomplete issue or label inventory")?;
        inv.labels.insert(s.to_string());
    }
    Ok(inv)
}
fn candidates(target: &str, key: &str, markers: &HashMap<String, BTreeSet<i64>>, pending: bool) {
    let nums = markers.get(key);
    if let Some(nums) = nums.filter(|s| !s.is_empty()) {
        println!(
            "{key}: untrusted candidates {}",
            nums.iter()
                .map(|n| format!("#{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if pending {
        println!(
            "  Inspect, then recover: h2a factory recover --repo {target} --id {key} --issue NUMBER"
        );
        println!(
            "  Or confirm settled absence: h2a factory recover --repo {target} --id {key} --confirm-absent"
        );
    } else if nums.is_some_and(|s| !s.is_empty()) {
        println!(
            "  Inspect, then adopt: h2a factory adopt --repo {target} --id {key} --issue NUMBER"
        );
        println!(
            "  Or assert absence: h2a factory seed --repo {target} --apply --confirm-absent {key}"
        );
    }
}
fn encode(s: &str, slash: bool) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || slash && b == b'/' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn body(seed: &Value, mapped: &HashMap<String, i64>, inv: &Inventory) -> Result<String> {
    let bullets = |v: Vec<String>| {
        v.into_iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let sources = strings(seed, "sources")?
        .into_iter()
        .map(|s| {
            if s.starts_with("https://") {
                s.to_owned()
            } else {
                let (p, a) = s.split_once('#').map_or((s, None), |(p, a)| (p, Some(a)));
                let anchor = a.map_or(String::new(), |a| format!("#{a}"));
                format!(
                    "[{s}](https://github.com/{}/blob/{}/{}{anchor})",
                    text(&inv.repo, "full_name"),
                    encode(text(&inv.repo, "default_branch"), false),
                    encode(p, true)
                )
            }
        })
        .collect();
    let deps = strings(seed, "depends_on")?
        .into_iter()
        .map(|s| {
            mapped
                .get(s)
                .and_then(|n| inv.issues.get(n))
                .map(|v| text(v, "html_url").to_owned())
                .context("missing dependency")
        })
        .collect::<Result<Vec<_>>>()?;
    let list = |key| -> Result<String> {
        Ok(bullets(
            strings(seed, key)?.into_iter().map(str::to_owned).collect(),
        ))
    };
    Ok(format!(
        "<!-- h2a-seed:{} -->\n\n## Intent\n\n{}\n\n## Sources\n\n{}\n\n## Scope\n\n{}\n\n## Acceptance\n\n{}\n\n## Verification\n\n{}\n\n## Dependencies\n\n{}\n\n## Risk\n\n{}\n",
        text(seed, "id"),
        text(seed, "intent"),
        bullets(sources),
        list("scope")?,
        list("acceptance")?,
        list("verification")?,
        if deps.is_empty() {
            "None.".into()
        } else {
            bullets(deps)
        },
        text(seed, "risk")
    ))
}
#[derive(Default)]
struct Args {
    cmd: String,
    repo: String,
    key: String,
    issue: Option<i64>,
    apply: bool,
    absent: Vec<String>,
    confirm: bool,
}
fn parse(args: &[String]) -> Result<Args> {
    let mut a = Args {
        cmd: args.first().context("factory command required")?.clone(),
        ..Default::default()
    };
    if !["validate", "plan", "seed", "recover", "adopt"].contains(&a.cmd.as_str()) {
        bail!("unknown factory command");
    }
    let mut i = 1;
    let mut seen = HashSet::new();
    while i < args.len() {
        let flag = args[i].as_str();
        if flag != "--confirm-absent" && !seen.insert(flag.to_owned()) {
            bail!("duplicate argument: {flag}");
        }
        match flag {
            "--apply" if a.cmd == "seed" => a.apply = true,
            "--confirm-absent" if a.cmd == "recover" => {
                if a.confirm {
                    bail!("duplicate --confirm-absent");
                }
                a.confirm = true;
            }
            "--repo" | "--id" | "--issue" | "--confirm-absent" => {
                i += 1;
                let v = args
                    .get(i)
                    .filter(|s| !s.starts_with("--"))
                    .context("missing argument value")?;
                match flag {
                    "--repo" => a.repo = v.clone(),
                    "--id" if ["recover", "adopt"].contains(&a.cmd.as_str()) => a.key = v.clone(),
                    "--issue" if ["recover", "adopt"].contains(&a.cmd.as_str()) => {
                        a.issue = Some(v.parse().context("invalid issue number")?)
                    }
                    "--confirm-absent" if a.cmd == "seed" => a.absent.push(v.clone()),
                    _ => bail!("invalid argument: {flag}"),
                }
            }
            _ => bail!("invalid argument: {flag}"),
        }
        i += 1;
    }
    if a.cmd == "validate" {
        if args.len() != 1 {
            bail!("validate takes no arguments");
        }
    } else if a.repo.is_empty() {
        bail!("--repo required");
    }
    if ["recover", "adopt"].contains(&a.cmd.as_str())
        && (a.key.is_empty() || (a.issue.is_some() == a.confirm) || a.cmd == "adopt" && a.confirm)
    {
        bail!("invalid recovery or adoption arguments");
    }
    Ok(a)
}
pub fn run(args: &[String]) -> Result<()> {
    let args = parse(args)?;
    let root = root()?;
    let (manifest, seeds) = load(&root)?;
    if args.cmd == "validate" {
        println!(
            "Valid manifest: {} seeds, {} labels",
            seeds.len(),
            manifest["labels"].as_array().unwrap().len()
        );
        return Ok(());
    }
    if !Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
        .unwrap()
        .is_match(&args.repo)
    {
        bail!("--repo must be owner/name");
    }
    let dir = common_dir(&root)?;
    let path = dir.join("h2a-factory-journal.json");
    let mutating =
        ["recover", "adopt"].contains(&args.cmd.as_str()) || args.cmd == "seed" && args.apply;
    let _lock = if mutating {
        Some(Lock::acquire(&dir)?)
    } else {
        None
    };
    let mut inv = inventory(&args.repo)?;
    let mut j = journal(&path, &inv.repo)?;
    let known: HashSet<&str> = seeds.iter().map(|s| text(s, "id")).collect();
    let mut mapped = HashMap::new();
    for (key, entry) in j["seeds"].as_object().unwrap() {
        if text(entry, "state") == "confirmed" {
            let n = entry["number"].as_i64().unwrap();
            if !inv.issues.contains_key(&n) {
                bail!("mapped issue #{n} is missing");
            }
            mapped.insert(key.clone(), n);
        }
    }
    if ["recover", "adopt"].contains(&args.cmd.as_str()) {
        if !known.contains(args.key.as_str()) {
            bail!("unknown seed ID");
        }
        let entry = j["seeds"].get(&args.key);
        if args.cmd == "recover" && entry != Some(&json!({"state":"pending"})) {
            bail!("recovery requires a pending seed");
        }
        if args.cmd == "adopt" && entry.is_some() {
            bail!("adoption requires an unmapped seed");
        }
        candidates(&args.repo, &args.key, &inv.markers, args.cmd == "recover");
        if args.confirm {
            j["seeds"].as_object_mut().unwrap().remove(&args.key);
            save(&path, &j)?;
            println!(
                "Cleared {} after operator-confirmed absence; no issue was created",
                args.key
            );
            return Ok(());
        }
        let n = args.issue.unwrap();
        if !inv.issues.contains_key(&n) {
            bail!("selected issue does not exist or is a pull request");
        }
        if mapped.values().any(|x| *x == n) {
            bail!("issue is already mapped");
        }
        j["seeds"][&args.key] = json!({"state":"confirmed","number":n});
        save(&path, &j)?;
        println!(
            "{} {} as #{n}",
            if args.cmd == "recover" {
                "Recovered"
            } else {
                "Adopted"
            },
            args.key
        );
        return Ok(());
    }
    if !args.absent.is_empty() && !args.apply {
        bail!("--confirm-absent requires --apply");
    }
    let mut assertions = HashSet::new();
    for key in &args.absent {
        if !assertions.insert(key) {
            bail!("duplicate --confirm-absent ID");
        }
        if !known.contains(key.as_str())
            || j["seeds"].get(key).is_some()
            || inv.markers.get(key).is_none_or(|s| s.is_empty())
        {
            bail!("invalid --confirm-absent ID: {key}; expected an unmapped seed with candidates");
        }
    }
    for label in manifest["labels"].as_array().unwrap() {
        if !inv.labels.contains(text(label, "name")) {
            println!("Missing label {}", text(label, "name"));
        }
    }
    for seed in &seeds {
        let key = text(seed, "id");
        if text(&j["seeds"][key], "state") == "pending" {
            println!("{key}: pending recovery");
            candidates(&args.repo, key, &inv.markers, true);
        } else if let Some(n) = mapped.get(key) {
            let issue = &inv.issues[n];
            println!(
                "{key}: #{n} {} {}",
                text(issue, "state"),
                text(issue, "html_url")
            );
        } else {
            println!("{key}: missing");
            candidates(&args.repo, key, &inv.markers, false);
        }
    }
    if !mutating {
        return Ok(());
    }
    for (key, entry) in j["seeds"].as_object().unwrap() {
        if text(entry, "state") == "pending" {
            bail!("pending seed {key}; use recover after inspecting GitHub");
        }
    }
    let blocked = seeds
        .iter()
        .map(|s| text(s, "id"))
        .filter(|key| {
            !mapped.contains_key(*key)
                && inv.markers.get(*key).is_some_and(|s| !s.is_empty())
                && !assertions.contains(&key.to_string())
        })
        .collect::<Vec<_>>();
    if !blocked.is_empty() {
        bail!(
            "untrusted candidates require adoption or --confirm-absent: {}",
            blocked.join(", ")
        );
    }
    if inv.repo["permissions"]["push"].as_bool() != Some(true) {
        bail!("seed --apply requires repository push permission");
    }
    for label in manifest["labels"].as_array().unwrap() {
        if !inv.labels.contains(text(label, "name")) {
            endpoint(format!("repos/{}/labels", args.repo), Some(label), false)?;
        }
    }
    for seed in &seeds {
        let key = text(seed, "id");
        if mapped.contains_key(key) {
            continue;
        }
        let issue_body = body(seed, &mapped, &inv)?;
        j["seeds"][key] = json!({"state":"pending"});
        save(&path, &j)?;
        let issue = endpoint(
            format!("repos/{}/issues", args.repo),
            Some(&json!({"title":seed["title"],"body":issue_body,"labels":seed["labels"]})),
            false,
        )?;
        let n = issue["number"]
            .as_i64()
            .filter(|n| *n > 0)
            .context(format!(
                "uncertain create for {key}; inspect GitHub and recover"
            ))?;
        if !issue["html_url"].is_string() {
            bail!("uncertain create for {key}; inspect GitHub and recover");
        }
        if !inv.numbers.insert(n) {
            bail!("uncertain create for {key}; duplicate issue number");
        }
        j["seeds"][key] = json!({"state":"confirmed","number":n});
        save(&path, &j)?;
        mapped.insert(key.to_owned(), n);
        println!("Created {key}: {}", text(&issue, "html_url"));
        inv.issues.insert(n, issue);
    }
    Ok(())
}
