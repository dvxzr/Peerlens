#!/usr/bin/env python3
"""Release helpers (stdlib only), used by .github/workflows/release.yml and by maintainers.

  release.py bump 0.2.0                  set the version in package.json, Cargo.toml, Cargo.lock, tauri.conf.json
  release.py check-version v0.2.0        fail unless the tag matches all of them
  release.py notes v0.2.0                changelog (Markdown) from conventional commits since the previous tag
  release.py latest-json --tag v0.2.0 --repo owner/name --sigs DIR --notes FILE
                                         Tauri updater manifest built from the release's *.sig files
  release.py self-test
"""
import datetime, json, pathlib, re, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FILES = {
    "package.json": ROOT / "package.json",
    "tauri.conf.json": ROOT / "src-tauri" / "tauri.conf.json",
    "Cargo.toml": ROOT / "src-tauri" / "Cargo.toml",
}
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")


def versions():
    out = {name: json.loads(p.read_text())["version"] for name, p in FILES.items() if p.suffix == ".json"}
    m = re.search(r'(?m)^version\s*=\s*"([^"]+)"', FILES["Cargo.toml"].read_text())
    out["Cargo.toml"] = m.group(1) if m else None
    return out


def bump(new):
    if not SEMVER.match(new):
        sys.exit(f"not a semver version: {new}")
    for name, p in FILES.items():
        text = p.read_text()
        if p.suffix == ".json":
            text = re.sub(r'("version"\s*:\s*)"[^"]+"', rf'\g<1>"{new}"', text, count=1)
        else:
            text = re.sub(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\g<1>"{new}"', text, count=1)
        p.write_text(text)
    lock = ROOT / "src-tauri" / "Cargo.lock"
    lock.write_text(re.sub(r'(name = "peerlens"\nversion = )"[^"]+"', rf'\g<1>"{new}"', lock.read_text()))
    print(f"version set to {new}: {', '.join(FILES)} and Cargo.lock")


def check_version(tag):
    want = tag.removeprefix("v")
    got = versions()
    bad = {k: v for k, v in got.items() if v != want}
    if bad:
        sys.exit(f"tag {tag} does not match versions {bad}. Run: scripts/release.py bump {want}")
    print(f"ok: {tag} matches {', '.join(got)}")


SECTIONS = [("feat", "Features"), ("fix", "Fixes"), ("perf", "Performance"), ("security", "Security")]


def notes_from_subjects(subjects):
    groups = {title: [] for _, title in SECTIONS}
    other = []
    for s in subjects:
        m = re.match(r"^(\w+)(?:\([^)]*\))?!?:\s*(.+)$", s)
        title = dict(SECTIONS).get(m.group(1)) if m else None
        if title:
            groups[title].append(m.group(2))
        elif m and m.group(1) in ("refactor", "build", "ci", "docs", "test", "chore", "style"):
            other.append(m.group(2))
        elif s.strip() and s.strip().lower() != "initial commit":
            other.append(s.strip())
    lines = []
    for title, items in list(groups.items()) + [("Other changes", other)]:
        if items:
            lines += [f"### {title}", *[f"- {i[0].upper() + i[1:]}" for i in items], ""]
    return "\n".join(lines).strip() or "Maintenance release."


def notes(tag):
    def git(*a):
        return subprocess.run(["git", *a], cwd=ROOT, capture_output=True, text=True)
    prev = git("describe", "--tags", "--abbrev=0", f"{tag}^").stdout.strip()
    rng = f"{prev}..{tag}" if prev else tag
    subjects = git("log", "--no-merges", "--pretty=%s", rng).stdout.splitlines()
    print(notes_from_subjects(subjects))


def platforms_for(asset):
    # The architecture must be explicit in the file name; never guess (a wrong-arch update would break installs).
    if re.search(r"aarch64|arm64", asset):
        arch = "aarch64"
    elif re.search(r"x86_64|x64|amd64", asset):
        arch = "x86_64"
    else:
        print(f"warning: skipping {asset}: no architecture in its name", file=sys.stderr)
        return []
    if asset.endswith(".app.tar.gz"):
        return [f"darwin-{arch}", f"darwin-{arch}-app"]
    if asset.endswith(".AppImage"):
        return [f"linux-{arch}", f"linux-{arch}-appimage"]
    if asset.endswith(".deb"):
        return [f"linux-{arch}-deb"]
    if asset.endswith(".rpm"):
        return [f"linux-{arch}-rpm"]
    return []


def latest_json(tag, repo, sig_dir, notes_text, now=None):
    platforms = {}
    for sig in sorted(pathlib.Path(sig_dir).glob("*.sig")):
        asset = sig.name[: -len(".sig")]
        url = f"https://github.com/{repo}/releases/download/{tag}/{asset}"
        for p in platforms_for(asset):
            platforms[p] = {"signature": sig.read_text().strip(), "url": url}
    if not platforms:
        sys.exit(f"no updater signatures (*.sig) found in {sig_dir}")
    now = now or datetime.datetime.now(datetime.timezone.utc)
    return {
        "version": tag.removeprefix("v"),
        "notes": notes_text,
        "pub_date": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }


def self_test():
    import tempfile
    n = notes_from_subjects(["feat: add updates", "fix(ui): align logo", "Initial commit", "ci: cache", "weird subject"])
    assert "### Features\n- Add updates" in n and "### Fixes\n- Align logo" in n, n
    assert "Initial commit" not in n and "- Cache" in n and "- Weird subject" in n, n
    assert platforms_for("Peerlens_aarch64.app.tar.gz") == ["darwin-aarch64", "darwin-aarch64-app"]
    assert platforms_for("Peerlens_x64.app.tar.gz") == ["darwin-x86_64", "darwin-x86_64-app"]
    assert platforms_for("Peerlens_0.2.0_amd64.AppImage") == ["linux-x86_64", "linux-x86_64-appimage"]
    assert platforms_for("Peerlens_0.2.0_amd64.deb") == ["linux-x86_64-deb"]
    assert platforms_for("Peerlens-0.2.0-1.x86_64.rpm") == ["linux-x86_64-rpm"]
    assert platforms_for("Peerlens_0.2.0_aarch64.dmg") == []
    assert platforms_for("Peerlens.app.tar.gz") == [], "no arch in the name → skipped"
    with tempfile.TemporaryDirectory() as d:
        for f in ["Peerlens_aarch64.app.tar.gz", "Peerlens_0.2.0_amd64.AppImage", "Peerlens-0.2.0-1.x86_64.rpm"]:
            pathlib.Path(d, f + ".sig").write_text(f"SIG-{f}\n")
        j = latest_json("v0.2.0", "o/r", d, "notes", datetime.datetime(2026, 1, 2, tzinfo=datetime.timezone.utc))
        assert j["version"] == "0.2.0" and j["pub_date"] == "2026-01-02T00:00:00Z"
        assert j["platforms"]["darwin-aarch64"]["url"] == "https://github.com/o/r/releases/download/v0.2.0/Peerlens_aarch64.app.tar.gz"
        assert j["platforms"]["linux-x86_64"]["signature"] == "SIG-Peerlens_0.2.0_amd64.AppImage.sig".replace(".sig", "")
        assert set(j["platforms"]) == {"darwin-aarch64", "darwin-aarch64-app", "linux-x86_64", "linux-x86_64-appimage", "linux-x86_64-rpm"}
    v = versions()
    assert len(set(v.values())) == 1, f"versions differ: {v}"
    print("self-test ok")


def main(argv):
    if not argv:
        sys.exit(__doc__)
    cmd, args = argv[0], argv[1:]
    if cmd == "bump" and len(args) == 1:
        bump(args[0])
    elif cmd == "check-version" and len(args) == 1:
        check_version(args[0])
    elif cmd == "notes" and len(args) == 1:
        notes(args[0])
    elif cmd == "latest-json":
        opts = dict(zip(args[::2], args[1::2]))
        notes_text = pathlib.Path(opts["--notes"]).read_text().strip()
        print(json.dumps(latest_json(opts["--tag"], opts["--repo"], opts["--sigs"], notes_text), indent=2))
    elif cmd == "self-test":
        self_test()
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
