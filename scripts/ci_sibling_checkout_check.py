#!/usr/bin/env python3
"""ALICE-* sibling の固定 commit (`.github/siblings.toml`) と CI の checkout を検査する

text-to-print は ALICE-* を `../ALICE-Xxx` の path 依存で引く CI は sibling を
`.github/siblings.toml` に書いた commit で checkout する
(`.github/actions/alice-siblings`) 本 script は次を fail にする

  静的 (常に)
    1. siblings.toml の `[siblings]` が 0 件 / 値が 40 桁の SHA でない
    2. composite action が checkout する sibling の集合が siblings.toml と違う
    3. workflow の job が sibling を直接 checkout している (固定 commit を迂回する)
    4. composite action を使う job が 0 件 (検査が何も比較していない)
  cargo (`--no-cargo` で省略)
    5. `cargo metadata` が解決できない (sibling の欠落か版要求の不一致)
    6. cargo の解決に出る sibling が siblings.toml に無い / siblings.toml に解決に出ない
       sibling がある (集合は manifest を目で追わず cargo の解決結果から求める:
       optional の path 依存は有効化されない限り要らないため)
  `--verify-heads` (CI 用)
    7. checkout された各 sibling の HEAD が siblings.toml の commit と違う

想定する範囲: repo と sibling の directory の中身、`.cargo/config.toml` の file (repo の中と親)
対象外: CI の環境そのものによる差し替え (`CARGO_HOME` の config、`--config` を足す wrapper、
環境変数での上書き) これらは workflow の定義 (review と静的な許可リスト) で扱う

usage: ci_sibling_checkout_check.py [--root <repo>] [--siblings <dir>]
                                    [--no-cargo] [--verify-heads] [--self-test]
  --root      text-to-print の root (既定: この script の親の親)
  --siblings  sibling repo が並ぶ directory (既定: root の親)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SIBLING_RE = re.compile(r"repository:\s*[\w.-]+/(ALICE-[\w-]+)")
ACTION_USE_RE = re.compile(r"uses:\s*\./(?:[\w.-]+/)?\.github/actions/alice-siblings\s*$", re.M)
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
PIN_LINE_RE = re.compile(r'^ALICE-[A-Za-z0-9-]+ = "[0-9a-f]{40}"$')
PRIVATE_LINE_RE = re.compile(r'^ALICE-[A-Za-z0-9-]+ = "[a-z]+-key"$')
# checkout の step で資格情報を渡す key (actions/checkout の input)
CREDENTIAL_KEY_RE = re.compile(r"(?m)^[ \t]+(token|ssh-key|ssh-known-hosts|ssh-user)[ \t]*:")
# workflow の許可リスト (sibling は composite action だけが checkout する)
#   - `repository:` は書かない (書いてよい literal は ALLOWED_REPOSITORIES だけ、現在は無し)
#   - repo を取得する command (git clone / fetch / pull / submodule / remote / archive、gh repo clone、
#     gh api、svn) は書かない
#   - curl / wget 等で GitHub から取る URL は literal に限り、ALICE の名前を含めない
#   - `owner/ALICE-Xxx` 形の参照は大文字小文字を問わず不可 (`../ALICE-Xxx` の相対 path は可)
# 綴りを列挙して禁じるのでなく、許す形だけを通す (値に `${{ }}` / `$` を含むものは一律不可)
ALLOWED_REPOSITORIES: frozenset[str] = frozenset()
LOCAL_ACTION_PATH = ".github/actions/alice-siblings"
REPOSITORY_KEY_RE = re.compile(r"""(?i)(?<![\w-])["']?repository["']?\s*:""")
FETCH_CMD_RE = re.compile(
    r"(?i)\bgit\b[^\n]*\b(?:clone|fetch|pull|submodule|remote|archive)\b"
    r"|\bgh\s+(?:repo\s+clone|api)\b|\bsvn\s+(?:co|checkout|export)\b"
)
DOWNLOAD_RE = re.compile(r"(?i)\b(?:curl|wget|invoke-webrequest|iwr|invoke-restmethod|irm)\b")
GITHUB_HOST_RE = re.compile(r"(?i)github\.com|githubusercontent\.com|codeload\.github")
REMOTE_REF_RE = re.compile(r"(?i)(?<![.\w])(?:[^\s/'\"]*[^\s/'\".])?/(alice-[a-z0-9-]+)")
ALICE_NAME_RE = re.compile(r"(?i)alice-[a-z0-9-]+")
MANIFEST = Path(".github/siblings.toml")
ACTION = Path(".github/actions/alice-siblings/action.yml")


def norm(p: Path) -> Path:
    """symlink を解かずに正規化する"""
    return Path(os.path.normpath(os.path.abspath(p)))


def read_pins(root: Path) -> tuple[dict[str, str], list[str]]:
    errors: list[str] = []
    with (root / MANIFEST).open("rb") as f:
        pins = dict((tomllib.load(f).get("siblings") or {}).items())
    if not pins:
        errors.append(f"{MANIFEST} の [siblings] が 0 件")
    for name, sha in pins.items():
        if not isinstance(sha, str) or not SHA_RE.match(sha):
            errors.append(f"{MANIFEST}: {name} の値が 40 桁の commit SHA でない: {sha!r}")
    # [siblings] の各行は 1 通りの綴り (`ALICE-Xxx = "<40 桁>"`) に限る
    # 読み手は本 script 1 つだが、綴りを固定して手書きの揺れ (単引用符 / inline table /
    # 複数行) を入口で落とす
    section = None
    canonical = 0
    for i, line in enumerate((root / MANIFEST).read_text(encoding="utf-8").splitlines(), 1):
        t = line.strip()
        if not t or t.startswith("#"):
            continue
        if t.startswith("["):
            section = t
            continue
        if section == "[siblings]":
            if PIN_LINE_RE.match(t):
                canonical += 1
            else:
                errors.append(f"{MANIFEST}:{i} が `ALICE-Xxx = \"<40 桁の SHA>\"` の形でない")
        elif section == "[private]" and not PRIVATE_LINE_RE.match(t):
            errors.append(f"{MANIFEST}:{i} が `ALICE-Xxx = \"<名前>-key\"` の形でない")
    if canonical != len(pins):
        errors.append(f"{MANIFEST}: 正規形の行 {canonical} 件と読めた pin {len(pins)} 件が一致しない")
    return pins, errors


def cargo_metadata(root: Path, locked: bool) -> subprocess.CompletedProcess:
    """本 script の cargo の呼び出しは全てここを通す

    cargo は `.cargo/config.toml` を「実行した directory とその親」から探す (`--manifest-path`
    の位置ではない) build と同じく repo の root で実行しないと、repo の中の config
    (`paths` / `[patch]`) を見落とす / 呼び出しごとに違う config を見る
    """
    cmd = ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(root / "Cargo.toml")]
    if locked:
        cmd.insert(4, "--locked")
    return subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", cwd=root)


def required_siblings(root: Path, siblings: Path) -> tuple[set[str], str | None]:
    """cargo の解決結果に出る sibling repo 名の集合 (失敗時は error 文字列)

    optional の path 依存は有効化されない限り解決に出ず、checkout も要らない
    (`cargo fmt --all` は path 依存の workspace の全 member まで読むが、CI の fmt は
    本 repo の package だけを見る)
    """
    proc = cargo_metadata(root, locked=False)
    if proc.returncode != 0:
        return set(), proc.stderr.strip() or f"cargo metadata exit {proc.returncode}"
    real_sib = Path(os.path.realpath(siblings))
    real_root = Path(os.path.realpath(root))
    need: set[str] = set()
    for pkg in json.loads(proc.stdout)["packages"]:
        if pkg.get("source") is not None:
            continue  # registry / git の依存
        mp = Path(os.path.realpath(pkg["manifest_path"]))
        if mp.parent == real_root or real_root in mp.parents:
            continue
        try:
            need.add(mp.relative_to(real_sib).parts[0])
        except ValueError:
            continue
    return need, None


def split_jobs(text: str) -> dict[str, str]:
    """workflow の `jobs:` 直下の job ごとに本文を切り出す"""
    lines = text.splitlines()
    try:
        start = next(i for i, l in enumerate(lines) if l.rstrip() == "jobs:")
    except StopIteration:
        return {}
    jobs: dict[str, list[str]] = {}
    cur: str | None = None
    for line in lines[start + 1 :]:
        if line and not line.startswith((" ", "#")):
            break
        m = JOB_RE.match(line)
        if m:
            cur = m.group(1)
            jobs[cur] = []
        elif cur is not None:
            jobs[cur].append(line)
    return {k: "\n".join(v) for k, v in jobs.items()}


def read_private(root: Path, pins: dict[str, str]) -> tuple[dict[str, str], list[str]]:
    """manifest の [private]: 非公開の sibling と、その deploy key を受ける action の input 名"""
    with (root / MANIFEST).open("rb") as f:
        private = dict((tomllib.load(f).get("private") or {}).items())
    errors = [f"{MANIFEST}: [private] の {n} が [siblings] に無い" for n in sorted(set(private) - set(pins))]
    if len(set(private.values())) != len(private):
        errors.append(f"{MANIFEST}: [private] の input 名が重複している")
    return private, errors


def depkey_secret(name: str) -> str:
    """非公開の sibling の deploy key の secret 名 (ALICE-Bamboo → ALICE_DEPKEY_BAMBOO)"""
    return "ALICE_DEPKEY_" + name.removeprefix("ALICE-").upper().replace("-", "_")


def caller_key_errors(wf: str, job: str, body: str, private: dict[str, str]) -> list[str]:
    """composite action を使う job は、非公開の sibling の deploy key だけを、その secret から渡す"""
    given = dict(re.findall(r"(?m)^[ \t]+([a-z-]+):[ \t]*(\S.*?)[ \t]*$", body.split("alice-siblings", 1)[1].split("\n      - ", 1)[0]))
    want = {inp: f"${{{{ secrets.{depkey_secret(n)} }}}}" for n, inp in private.items()}
    if given != want:
        return [f"{wf}:{job} が composite action に渡す値 {given} が {want} でない"]
    return []


def static_checks(root: Path, pins: dict[str, str]) -> tuple[list[str], int]:
    errors: list[str] = []
    private, p_errors = read_private(root, pins)
    errors += p_errors
    errors += action_shape_errors(root / ACTION, pins, private)
    users = 0
    examined = 0
    for wf in sorted((root / ".github/workflows").glob("*.y*ml")):
        text = wf.read_text(encoding="utf-8")
        hits, steps = workflow_violations(text)
        examined += steps
        for lineno, reason in hits:
            errors.append(f"{wf.name}:{lineno}: {reason}")
        for job, body in split_jobs(text).items():
            if ACTION_USE_RE.search(body):
                users += 1
                errors += job_end_errors(wf.name, job, body)
                errors += caller_key_errors(wf.name, job, body, private)
    if examined == 0:
        errors.append("workflow の step を 1 つも見ていない (検査が何も見ていない)")
    if users == 0:
        errors.append("composite action を使う job が 0 件 (検査が何も比較していない)")
    return errors, users


def workflow_violations(text: str) -> tuple[list[tuple[int, str]], int]:
    """workflow 本文の許可リスト違反 (行番号, 理由) と、見た step 数

    `#` で始まる comment 行以外の全行が対象 (run の複数行 script も含む)
    """
    hits: list[tuple[int, str]] = []
    steps = 0
    for i, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("#"):
            continue
        if re.match(r"^\s+-\s+(?:name|uses|run|id|if|with|shell)\b", line):
            steps += 1
        key = REPOSITORY_KEY_RE.search(line)
        if key:  # block 形式でも flow 形式 (`with: {"repository": …}`) でも
            value = re.split(r"[,}]", line[key.end():], maxsplit=1)[0].strip().strip("'\"")
            if value not in ALLOWED_REPOSITORIES:
                hits.append((i, f"repository: の指定 ({value or '空'}) — sibling は composite action だけが checkout する"))
        if FETCH_CMD_RE.search(line):
            hits.append((i, "repo を取得する command — workflow では使わない"))
        if DOWNLOAD_RE.search(line) and GITHUB_HOST_RE.search(line):
            if "$" in line or ALICE_NAME_RE.search(line):
                hits.append((i, "GitHub からの download の URL が literal でない / ALICE の名前を含む"))
        # 本 repo の composite action の path (literal 1 つだけ) は sibling の参照ではない
        for m in REMOTE_REF_RE.finditer(line.replace(LOCAL_ACTION_PATH, "")):
            hits.append((i, f"{m.group(1)} を owner/名前 の形で参照している"))
    return sorted(set(hits)), steps


def job_end_errors(wf: str, job: str, body: str) -> list[str]:
    """composite action を使う job は、最後の step で HEAD を再照合する"""
    steps = re.split(r"(?m)^      - ", body)
    last = steps[-1] if len(steps) > 1 else ""
    if "--heads-only" not in last:
        return [f"{wf}:{job} の最後の step が sibling の HEAD 照合 (--heads-only) でない"]
    return []


def action_shape_errors(action: Path, pins: dict[str, str], private: dict[str, str] | None = None) -> list[str]:
    """composite action の各 checkout が manifest の同名 pin を ref に使っているか

    資格情報は [private] の sibling の checkout だけが、その input の deploy key を
    `ssh-key:` で受け、`persist-credentials: false` で step の後に残さない
    ほかの checkout (公開の sibling) は資格情報を何も受けない
    """
    private = private or {}
    errors: list[str] = []
    text = action.read_text(encoding="utf-8")
    steps = re.split(r"(?m)^    - ", text)
    seen: list[str] = []
    for step in steps:
        repo = re.search(r"(?m)^\s+repository:\s*ext-sakamoro/(ALICE-[A-Za-z0-9-]+)\s*$", step)
        if not repo:
            continue
        name = repo.group(1)
        seen.append(name)
        if not re.search(rf"(?m)^\s+path:\s*{re.escape(name)}\s*$", step):
            errors.append(f"{action.name}: {name} の checkout の path が {name} でない")
        if not re.search(rf"(?m)^\s+ref:\s*\$\{{{{ steps\.pins\.outputs\['{re.escape(name)}'\] }}}}\s*$", step):
            errors.append(f"{action.name}: {name} の checkout の ref が固定 commit (steps.pins.outputs) でない")
        creds = CREDENTIAL_KEY_RE.findall(step)
        if name in private:
            key = rf"(?m)^[ \t]+ssh-key:[ \t]*\$\{{{{ inputs\.{re.escape(private[name])} }}}}[ \t]*$"
            if creds != ["ssh-key"] or not re.search(key, step):
                errors.append(f"{action.name}: {name} の checkout が deploy key (inputs.{private[name]}) だけを受けていない")
            if not re.search(r"(?m)^[ \t]+persist-credentials:[ \t]*false[ \t]*$", step):
                errors.append(f"{action.name}: {name} の checkout が persist-credentials: false でない")
        elif creds:
            errors.append(f"{action.name}: 公開の {name} の checkout が資格情報 ({', '.join(creds)}) を受けている")
    if sorted(seen) != sorted(pins):
        errors.append(f"{action.name} の checkout {sorted(seen)} と {MANIFEST} {sorted(pins)} が不一致")
    block = re.search(r"(?ms)^inputs:[ \t]*\n(.*?)^\S", text)
    declared = set(re.findall(r"(?m)^  ([a-z-]+):[ \t]*$", block.group(1))) if block else set()
    if declared != set(private.values()):
        errors.append(f"{action.name} の inputs {sorted(declared)} が [private] の {sorted(private.values())} でない")
    if "--emit-pins" not in text or "--heads-only" not in text:
        errors.append(f"{action.name} が pin を本 script で読んでいない / 最後に HEAD を照合していない")
    return errors


# sibling の directory に置いてよい「追跡外」は cargo の build 出力 1 つだけ
# (CI では consumer 側の target に書くので通常は存在しない)
ALLOWED_IGNORED = frozenset({"target/"})


def verify_heads(siblings: Path, pins: dict[str, str]) -> list[str]:
    """各 sibling の中身が固定 commit の tree と完全に一致することを確かめる

    HEAD (pointer) だけでなく、cargo / rustup が実際に読む bytes を照合する:
      - HEAD が固定 commit
      - `git diff --quiet HEAD` (追跡 file の書き換え無し、index 含む)
      - `git status --porcelain --untracked-files=all --ignored=matching` が空
        (未追跡 file 無し、ignore されているが存在する file 無し、例外は ALLOWED_IGNORED)
    取得の方法 (clone / tarball / script) に依らず、中身が違えば fail する
    """
    errors: list[str] = []
    if not pins:
        return ["照合する sibling が 0 件"]
    for name, sha in sorted(pins.items()):
        d = str(siblings / name)
        proc = subprocess.run(["git", "-C", d, "rev-parse", "HEAD"], capture_output=True, text=True)
        head = proc.stdout.strip()
        if proc.returncode != 0 or head != sha:
            errors.append(f"{name} の HEAD {head or '(取得失敗)'} が固定 commit {sha} と違う")
            continue
        # status / diff から file を隠す index の flag (変更無視 = 小文字、skip flag = S) と
        # commit の中身を差し替える replace ref は、それ自体を不可とする
        flags = subprocess.run(["git", "-C", d, "ls-files", "-v"], capture_output=True, text=True)
        hidden = [l[2:] for l in flags.stdout.splitlines() if l[:1].islower() or l[:1] == "S"]
        if flags.returncode != 0 or hidden:
            errors.append(f"{name} に status から隠された追跡 file がある: {', '.join(hidden[:5]) or '(ls-files 失敗)'}")
        replaced = subprocess.run(["git", "-C", d, "replace", "-l"], capture_output=True, text=True)
        if replaced.returncode != 0 or replaced.stdout.strip():
            errors.append(f"{name} に replace ref がある (commit の中身が差し替えられうる)")
        diff = subprocess.run(["git", "-C", d, "diff", "--quiet", "HEAD"], capture_output=True)
        if diff.returncode != 0:
            errors.append(f"{name} の追跡 file が固定 commit から書き換えられている")
        st = subprocess.run(
            ["git", "-C", d, "status", "--porcelain", "--untracked-files=all", "--ignored=matching"],
            capture_output=True, text=True,
        )
        if st.returncode != 0:
            errors.append(f"{name} の git status が失敗した")
            continue
        extra = []
        for line in st.stdout.splitlines():
            if line.startswith("!! ") and line[3:] in ALLOWED_IGNORED:
                continue
            extra.append(line)
        if extra:
            shown = ", ".join(extra[:5]) + (f" ほか {len(extra) - 5} 件" if len(extra) > 5 else "")
            errors.append(f"{name} に固定 commit の tree に無い / 異なる file がある: {shown}")
    return errors


def check(root: Path, siblings: Path, use_cargo: bool = True, heads: bool = False) -> int:
    root, siblings = norm(root), norm(siblings)
    pins, errors = read_pins(root)
    st_errors, users = static_checks(root, pins)
    errors += st_errors
    need: set[str] = set()
    if use_cargo:
        need, err = required_siblings(root, siblings)
        if err is not None:
            errors.append(f"cargo metadata が失敗 (sibling の欠落か版要求の不一致):\n{err}")
        elif not need:
            errors.append("cargo が読む sibling が 0 件 (検査が何も見ていない)")
        else:
            if need - set(pins):
                errors.append(f"cargo が読むのに {MANIFEST} に無い sibling: {', '.join(sorted(need - set(pins)))}")
            if set(pins) - need:
                errors.append(f"{MANIFEST} にあるが cargo が読まない sibling: {', '.join(sorted(set(pins) - need))}")
    if heads:
        errors += verify_heads(siblings, pins)

    print(f"pinned siblings ({len(pins)}): {', '.join(sorted(pins))}")
    if use_cargo:
        print(f"cargo-required siblings ({len(need)}): {', '.join(sorted(need))}")
    print(f"jobs using the pinned checkout: {users}")
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    return 1 if errors else 0


def emit_pins(root: Path) -> int:
    """manifest を検査器と同じ parser で読み、`$GITHUB_OUTPUT` 用の行を出す (fail closed)"""
    pins, errors = read_pins(norm(root))
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    if errors or not pins:
        return 1
    out = sys.stdout
    if hasattr(out, "reconfigure"):
        out.reconfigure(newline="\n")  # Windows でも `$GITHUB_OUTPUT` に CR を混ぜない
    for name, sha in sorted(pins.items()):
        out.write(f"{name}={sha}\n")
    print(f"emitted {len(pins)} sibling pins", file=sys.stderr)
    return 0


def provided_packages(siblings: Path, pins: dict[str, str]) -> dict[str, str]:
    """固定した各 sibling の tree にある package 名 → sibling 名"""
    out: dict[str, str] = {}
    for name in pins:
        base = siblings / name
        for manifest in sorted(base.rglob("Cargo.toml")):
            rel = manifest.relative_to(base).parts
            if any(p == "target" or p.startswith(".") for p in rel[:-1]):
                continue
            try:
                with manifest.open("rb") as f:
                    pkg = (tomllib.load(f).get("package") or {}).get("name")
            except (OSError, tomllib.TOMLDecodeError):
                continue
            if isinstance(pkg, str):
                out[pkg] = name
    return out


def verify_resolution(root: Path, siblings: Path, pins: dict[str, str]) -> tuple[list[str], int]:
    """cargo が実際に解決した alice-* の package が、固定した sibling の directory から来ているか

    tree の照合は sibling の directory の中身しか見ないので、その外 (workspace の親の
    `.cargo/config.toml` の `paths` / `[patch]`、manifest の path の書き換え等) で
    別の場所の source に差し替えられると通ってしまう ⇒ 解決結果そのものを照合する
      - path の alice-* は、固定した sibling の directory の下にあること
      - 固定した sibling が提供する名前の package が registry / git から来ていないこと
      - 照合した alice-* が 0 件なら fail
    """
    errors: list[str] = []
    proc = cargo_metadata(root, locked=True)
    if proc.returncode != 0:
        return [f"cargo metadata --locked が失敗:\n{proc.stderr.strip()}"], 0
    return judge_resolution(json.loads(proc.stdout), root, siblings, pins)


def judge_resolution(meta: dict, root: Path, siblings: Path, pins: dict[str, str]) -> tuple[list[str], int]:
    errors: list[str] = []
    provided = provided_packages(siblings, pins)
    real_sib = Path(os.path.realpath(siblings))
    real_root = Path(os.path.realpath(root))
    examined = 0
    for pkg in meta["packages"]:
        if not pkg["name"].startswith("alice-"):
            continue
        examined += 1
        mp = Path(os.path.realpath(pkg["manifest_path"]))
        if pkg.get("source") is None:
            if mp.parent == real_root or real_root in mp.parents:
                continue
            try:
                owner = mp.relative_to(real_sib).parts[0]
            except ValueError:
                owner = None
            if owner not in pins:
                errors.append(f"{pkg['name']} が固定した sibling の外から解決されている: {mp.parent}")
            elif provided.get(pkg["name"]) not in (None, owner):
                errors.append(f"{pkg['name']} が {owner} から解決されているが、固定した tree では {provided[pkg['name']]} にある")
        elif pkg["name"] in provided:
            errors.append(f"{pkg['name']} は {provided[pkg['name']]} に固定しているのに {pkg['source']} から解決されている")
    if examined == 0:
        errors.append("解決結果に alice-* の package が 0 件 (照合が何も見ていない)")
    return errors, examined


def heads_only(root: Path, siblings: Path) -> int:
    """composite action の最後と各 job の最後: 各 sibling の中身と、cargo の解決先を照合する"""
    root, siblings = norm(root), norm(siblings)
    pins, errors = read_pins(root)
    errors += verify_heads(siblings, pins)
    res_errors, examined = verify_resolution(root, siblings, pins)
    errors += res_errors
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    ok = not errors
    print(f"verified {len(pins) if ok else 0}/{len(pins)} sibling trees (HEAD + clean)")
    print(f"verified {examined if ok else 0} alice-* package(s) resolve to the pinned trees")
    return 0 if ok else 1


def self_test() -> int:
    """架空の crate 群で歯を確かめる"""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)

        def w(rel: str, body: str) -> None:
            p = t / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(body, encoding="utf-8")

        def crate(rel: str, name: str, deps: str = "") -> None:
            w(f"{rel}/Cargo.toml", f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2021"\n[dependencies]\n{deps}')
            w(f"{rel}/src/lib.rs", "")

        def git_init(rel: str) -> str:
            d = str(t / rel)
            for cmd in (
                ["git", "init", "-q"],
                ["git", "add", "-A"],
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "x"],
            ):
                subprocess.run(["git", "-C", d, *cmd[1:]] if cmd[0] == "git" else cmd, check=True)
            return subprocess.run(["git", "-C", d, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()

        w("app/Cargo.toml", '[workspace]\nmembers=["c"]\nresolver="2"\n')
        crate("app/c", "c", 'alice-a={path="../../ALICE-A"}\n')
        crate("ALICE-A", "alice-a", 'alice-b={path="../ALICE-B"}\nalice-z={path="../ALICE-Z",optional=true}\n')
        crate("ALICE-B", "alice-b")
        crate("ALICE-Z", "alice-z")
        subprocess.run(["cargo", "generate-lockfile", "--manifest-path", str(t / "app/Cargo.toml")],
                       check=True, capture_output=True)
        w("ALICE-A/.gitignore", "target/\nrust-toolchain.toml\n.cargo/\n")
        sha_a, sha_b = git_init("ALICE-A"), git_init("ALICE-B")

        need, err = required_siblings(t / "app", t)
        assert err is None, err
        assert need == {"ALICE-A", "ALICE-B"}, need  # optional 未有効の Z は解決に出ない

        good_pins = f'[siblings]\nALICE-A = "{sha_a}"\nALICE-B = "{sha_b}"\n'

        def action_for(names: list[str], ref_of=None, tail: str = "--heads-only") -> str:
            out = "runs:\n  steps:\n    - name: pins\n      run: x.py --emit-pins\n"
            for n in names:
                ref = ref_of(n) if ref_of else f"${{{{ steps.pins.outputs['{n}'] }}}}"
                out += (
                    f"    - name: Checkout {n}\n      uses: actions/checkout@v5\n      with:\n"
                    f"        repository: ext-sakamoro/{n}\n        path: {n}\n        ref: {ref}\n"
                )
            return out + f"    - name: heads\n      run: x.py {tail}\n"

        good_action = action_for(["ALICE-A", "ALICE-B"])
        good_wf = (
            "jobs:\n  build:\n    steps:\n      - uses: ./app/.github/actions/alice-siblings\n"
            "      - run: cp ../ALICE-A/target/x dist/x\n"
            "      - run: wget -q https://github.com/o/tool/releases/download/v1/tool -O tool\n"
            "      - run: x.py --heads-only\n"
            "  lint:\n    steps:\n      - run: true\n"
        )

        def setup(pins: str = good_pins, action: str = good_action, wf: str = good_wf) -> None:
            w("app/.github/siblings.toml", pins)
            w("app/.github/actions/alice-siblings/action.yml", action)
            w("app/.github/workflows/ci.yml", wf)

        setup()
        assert check(t / "app", t, heads=True) == 0  # ../ALICE-A (checkout 済みの相対 path) は許す
        assert heads_only(t / "app", t) == 0

        setup(pins=good_pins.replace(f'ALICE-B = "{sha_b}"\n', ""), action=action_for(["ALICE-A"]))
        assert check(t / "app", t) == 1, "cargo が読むのに manifest に無い sibling を検出しない"

        setup(pins=good_pins + f'ALICE-Y = "{sha_a}"\n', action=action_for(["ALICE-A", "ALICE-B", "ALICE-Y"]))
        assert check(t / "app", t) == 1, "使われない manifest entry を検出しない"

        setup(action=action_for(["ALICE-A", "ALICE-X"]))
        assert check(t / "app", t, use_cargo=False) == 1, "action と manifest の不一致を検出しない"

        setup(action=action_for(["ALICE-A", "ALICE-B"], ref_of=lambda n: "main"))
        assert check(t / "app", t, use_cargo=False) == 1, "pin を使わない ref を検出しない"

        setup(action=action_for(["ALICE-A", "ALICE-B"], tail="true"))
        assert check(t / "app", t, use_cargo=False) == 1, "action 最後の HEAD 照合の欠落を検出しない"

        bypasses = {
            "直接 checkout": "      - with:\n          repository: o/ALICE-A\n",
            "引用符の repository": '      - with:\n          repository: "ext-sakamoro/ALICE-A"\n',
            "式の owner": "      - with:\n          repository: ${{ github.repository_owner }}/ALICE-A\n",
            "flow 形式の repository": '      - uses: actions/checkout@v5\n        with: {"repository": "someone/other", path: x}\n',
            "小文字の名前": "      - with:\n          repository: ext-sakamoro/alice-a\n",
            "matrix の名前": "    strategy:\n      matrix:\n        sib: [ALICE-A]\n    steps:\n      - with:\n          repository: ext-sakamoro/${{ matrix.sib }}\n",
            "env の名前": "    env:\n      SIB: ALICE-A\n    steps:\n      - with:\n          repository: ${{ github.repository_owner }}/${{ env.SIB }}\n",
            "本 repo 以外の repository (名前なし)": "      - with:\n          repository: someone/other\n",
            "run の git clone": "      - run: git clone https://github.com/ext-sakamoro/ALICE-A\n",
            "run の shell 変数の clone": '      - run: |\n          n=ALICE-A\n          git clone "https://github.com/ext-sakamoro/$n" "../$n"\n',
            "run の git -C fetch": "      - run: git -C ../ALICE-A fetch origin main\n",
            "run の gh repo clone": "      - run: gh repo clone ALICE-A\n",
            "run の ssh clone": "      - run: git clone git@github.com:ext-sakamoro/ALICE-A.git\n",
            "run の curl (変数の URL)": '      - run: curl -L "https://codeload.github.com/o/$N/tar.gz/main" | tar xz\n',
            "uses で sibling の action": "      - uses: ext-sakamoro/ALICE-A/.github/actions/x@main\n",
            "uses で sibling の action (小文字)": "      - uses: ext-sakamoro/alice-a/.github/actions/x@main\n",
            "run の curl (ALICE の URL)": "      - run: curl -L https://github.com/o/ALICE-A/archive/main.tar.gz | tar xz\n",
        }
        for label, step in bypasses.items():
            body = step if step.startswith("    ") and not step.startswith("      ") else "    steps:\n" + step
            setup(wf=good_wf + "  direct:\n" + body)
            assert check(t / "app", t, use_cargo=False) == 1, f"{label} を検出しない"

        setup(wf="jobs:\n  lint:\n    steps:\n      - run: true\n")
        assert check(t / "app", t, use_cargo=False) == 1, "比較 0 件で fail しない"

        # 非公開の sibling (B) だけが deploy key を受け、公開の A は資格情報を受けない
        priv_pins = good_pins + '[private]\nALICE-B = "b-key"\n'

        def priv_action(a_extra: str = "", b_with: str | None = None, inputs: str = "  b-key:\n    required: true\n") -> str:
            b_with = ("        ssh-key: ${{ inputs.b-key }}\n        persist-credentials: false\n"
                      if b_with is None else b_with)
            out = good_action.replace("runs:\n", f"inputs:\n{inputs}runs:\n" if inputs else "runs:\n")
            out = out.replace("        ref: ${{ steps.pins.outputs['ALICE-A'] }}\n",
                              "        ref: ${{ steps.pins.outputs['ALICE-A'] }}\n" + a_extra)
            return out.replace("        ref: ${{ steps.pins.outputs['ALICE-B'] }}\n",
                               "        ref: ${{ steps.pins.outputs['ALICE-B'] }}\n" + b_with)

        priv_wf = good_wf.replace("      - uses: ./app/.github/actions/alice-siblings\n",
                                  "      - uses: ./app/.github/actions/alice-siblings\n"
                                  "        with:\n          b-key: ${{ secrets.ALICE_DEPKEY_B }}\n")
        setup(pins=priv_pins, action=priv_action(), wf=priv_wf)
        assert check(t / "app", t, use_cargo=False) == 0, "正しい deploy key の構成を fail にした"
        key_cases = {
            "公開の checkout の ssh-key": dict(action=priv_action(a_extra="        ssh-key: ${{ inputs.b-key }}\n")),
            "公開の checkout の token": dict(action=priv_action(a_extra="        token: ${{ inputs.b-key }}\n")),
            "非公開の checkout の token": dict(action=priv_action(
                b_with="        token: ${{ inputs.b-key }}\n        persist-credentials: false\n")),
            "非公開の checkout の鍵の欠落": dict(action=priv_action(b_with="        persist-credentials: false\n")),
            "persist-credentials の欠落": dict(action=priv_action(b_with="        ssh-key: ${{ inputs.b-key }}\n")),
            "別の input の鍵": dict(action=priv_action(
                b_with="        ssh-key: ${{ inputs.x-key }}\n        persist-credentials: false\n")),
            "宣言されない input": dict(action=priv_action(inputs="")),
            "余分な input": dict(action=priv_action(inputs="  b-key:\n    required: true\n  eco-token:\n    required: true\n")),
            "呼び出し側の別の secret": dict(wf=priv_wf.replace("ALICE_DEPKEY_B", "ALICE_ECO_TOKEN")),
            "呼び出し側の鍵の欠落": dict(wf=good_wf),
            "呼び出し側の余分な値": dict(wf=priv_wf.replace(
                "          b-key:", "          eco-token: ${{ secrets.ALICE_ECO_TOKEN }}\n          b-key:")),
            "[private] の未知の sibling": dict(pins=priv_pins + 'ALICE-Q = "q-key"\n'),
            "[private] の正規形でない値": dict(pins=good_pins + '[private]\nALICE-B = "b"\n'),
        }
        for label, kw in key_cases.items():
            setup(**{"pins": priv_pins, "action": priv_action(), "wf": priv_wf, **kw})
            assert check(t / "app", t, use_cargo=False) == 1, f"{label} を検出しない"

        setup(wf="jobs: {}\n")
        st_errors, _ = static_checks(t / "app", read_pins(t / "app")[0])
        assert any("step を 1 つも見ていない" in e for e in st_errors), "step を 1 つも見ていない時に fail しない"
        assert workflow_violations(good_wf)[1] > 0

        setup(wf=good_wf.replace("      - run: x.py --heads-only\n", ""))
        assert check(t / "app", t, use_cargo=False) == 1, "job の最後の HEAD 再照合の欠落を検出しない"

        setup(pins=good_pins.replace(sha_b, sha_b[:7]))
        assert check(t / "app", t, use_cargo=False) == 1, "短縮 SHA を検出しない"

        setup(pins=good_pins.replace(f'"{sha_b}"', f"'{sha_b}'"))
        assert check(t / "app", t, use_cargo=False) == 1, "単引用符の pin (正規形でない) を検出しない"
        assert emit_pins(t / "app") == 1, "正規形でない manifest で pin を出力した"

        setup(pins=good_pins.replace(sha_b, sha_a))
        assert check(t / "app", t, heads=True) == 1, "HEAD と固定 commit の不一致を検出しない"
        assert heads_only(t / "app", t) == 1, "action の HEAD 照合が不一致を検出しない"

        # 中身の照合: HEAD が固定 commit のままでも、tree が違えば red
        setup()
        pins_now = read_pins(t / "app")[0]
        assert verify_heads(t, pins_now) == [], "clean な tree を red にした"
        (t / "ALICE-A/target/debug").mkdir(parents=True)
        (t / "ALICE-A/target/debug/x").write_text("build output", encoding="utf-8")
        assert verify_heads(t, pins_now) == [], "許可した target/ を red にした"

        def restore() -> None:
            for d in ("ALICE-A", "ALICE-B"):
                subprocess.run(["git", "-C", str(t / d), "checkout", "-q", "--", "."], check=True)
                subprocess.run(["git", "-C", str(t / d), "clean", "-qfdx", "-e", "target/"], check=True)

        tampers = {
            "追跡 file の上書き": lambda: (t / "ALICE-B/src/lib.rs").write_text("pub fn evil() {}\n", encoding="utf-8"),
            "未追跡の .rs": lambda: (t / "ALICE-A/src/extra.rs").write_text("pub fn x() {}\n", encoding="utf-8"),
            "tarball の展開 (別 tree)": lambda: (
                (t / "ALICE-A/Cargo.toml").write_text('[package]\nname="a"\nversion="9.0.0"\nedition="2021"\n', encoding="utf-8"),
                (t / "ALICE-A/README").write_text("from release tarball\n", encoding="utf-8"),
            ),
            "ignore されている rust-toolchain.toml": lambda: (t / "ALICE-A/rust-toolchain.toml").write_text('[toolchain]\nchannel="nightly"\n', encoding="utf-8"),
            "ignore されている .cargo/config.toml": lambda: (
                (t / "ALICE-A/.cargo").mkdir(exist_ok=True),
                (t / "ALICE-A/.cargo/config.toml").write_text("[build]\nrustflags=[]\n", encoding="utf-8"),
            ),
            "index の skip flag で隠した書き換え": lambda: (
                subprocess.run(["git", "-C", str(t / "ALICE-B"), "update-index", "--skip-worktree", "src/lib.rs"], check=True),
                (t / "ALICE-B/src/lib.rs").write_text("pub fn hidden() {}\n", encoding="utf-8"),
            ),
            "index の変更無視 flag で隠した書き換え": lambda: (
                subprocess.run(["git", "-C", str(t / "ALICE-B"), "update-index", "--assume-unchanged", "src/lib.rs"], check=True),
                (t / "ALICE-B/src/lib.rs").write_text("pub fn hidden() {}\n", encoding="utf-8"),
            ),
            "replace ref": lambda: subprocess.run(
                ["git", "-C", str(t / "ALICE-B"), "replace", "HEAD",
                 subprocess.run(["git", "-C", str(t / "ALICE-B"), "-c", "user.name=t", "-c", "user.email=t@t",
                                 "commit-tree", "HEAD^{tree}", "-m", "r"],
                                capture_output=True, text=True, check=True).stdout.strip()],
                check=True,
            ),
            "index だけの変更 (作業 tree は HEAD と同じ)": lambda: (
                (t / "ALICE-B/src/lib.rs").write_text("// staged\n", encoding="utf-8"),
                subprocess.run(["git", "-C", str(t / "ALICE-B"), "add", "src/lib.rs"], check=True),
                (t / "ALICE-B/src/lib.rs").write_text("", encoding="utf-8"),
            ),
        }
        for label, tamper in tampers.items():
            tamper()
            assert verify_heads(t, pins_now) != [], f"{label} を検出しない"
            assert heads_only(t / "app", t) == 1, f"action の照合が {label} を検出しない"
            for r in subprocess.run(["git", "-C", str(t / "ALICE-B"), "replace", "-l"],
                                    capture_output=True, text=True).stdout.split():
                subprocess.run(["git", "-C", str(t / "ALICE-B"), "replace", "-d", r], check=True, capture_output=True)
            for flag in ("--no-skip-worktree", "--no-assume-unchanged"):
                subprocess.run(["git", "-C", str(t / "ALICE-B"), "update-index", flag, "src/lib.rs"], check=True)
            subprocess.run(["git", "-C", str(t / "ALICE-B"), "reset", "-q", "--hard", "HEAD"], check=True)
            restore()
            assert verify_heads(t, pins_now) == [], f"{label} の後に元へ戻らない"
        assert verify_heads(t, {}) != [], "照合する sibling が 0 件で fail しない"

        # 解決先の照合: sibling の tree の外で source を差し替えると red
        errs, n = verify_resolution(t / "app", t, pins_now)
        assert errs == [] and n == 2, (errs, n)
        crate("fork-b", "alice-b")
        # `.cargo/config.toml` の paths は path 依存も差し替える (repo の中と親 directory の両方)
        # 2 つの cargo の呼び出しが同じ config を見る (呼び出し元の cwd に依らない)
        w("app/.cargo/config.toml", f'paths = ["{(t / "fork-b").as_posix()}"]\n')
        here = os.getcwd()
        try:
            os.chdir(tempfile.gettempdir())
            need_cfg, err = required_siblings(t / "app", t)
        finally:
            os.chdir(here)
        assert err is None, err
        assert "fork-b" in need_cfg, f"required_siblings が repo の中の config を見ていない: {need_cfg}"
        (t / "app/.cargo/config.toml").unlink()
        for cfg in ("app/.cargo/config.toml", ".cargo/config.toml"):
            w(cfg, f'paths = ["{(t / "fork-b").as_posix()}"]\n')
            assert verify_heads(t, pins_now) == [], "前提: sibling の tree は clean"
            errs, _ = verify_resolution(t / "app", t, pins_now)
            assert errs, f"{cfg} の paths による差し替えを検出しない"
            assert heads_only(t / "app", t) == 1, f"action の照合が {cfg} の paths を検出しない"
            (t / cfg).unlink()
            assert verify_resolution(t / "app", t, pins_now)[0] == [], f"{cfg} を外した後に元へ戻らない"
        # 実際の差し替えは他に manifest の path の書き換えと [patch] (registry の alice-*)
        # path の書き換えは実物で、[patch] は下の合成した解決結果で確かめる
        crate("fork-a", "alice-a", 'alice-b={path="../ALICE-B"}\n')
        c_manifest = (t / "app/c/Cargo.toml").read_text(encoding="utf-8")
        lock = t / "app/Cargo.lock"

        def relock() -> None:
            subprocess.run(["cargo", "generate-lockfile", "--manifest-path", str(t / "app/Cargo.toml")],
                           check=True, capture_output=True)

        # consumer の manifest の path を fork に向ける (sibling の tree は clean のまま)
        (t / "app/c/Cargo.toml").write_text(c_manifest.replace("../../ALICE-A", "../../fork-a"), encoding="utf-8")
        relock()
        assert verify_heads(t, pins_now) == [], "前提: sibling の tree は clean"
        errs, _ = verify_resolution(t / "app", t, pins_now)
        assert errs, "固定した sibling の外 (fork) への差し替えを検出しない"
        assert heads_only(t / "app", t) == 1, "action の照合が fork への差し替えを検出しない"
        (t / "app/c/Cargo.toml").write_text(c_manifest, encoding="utf-8")
        relock()
        assert verify_resolution(t / "app", t, pins_now)[0] == [], "差し替えを外した後に元へ戻らない"

        # Cargo.lock と解決が一致しない (--locked) と red
        saved = lock.read_text(encoding="utf-8")
        lock.unlink()
        assert verify_resolution(t / "app", t, pins_now)[0], "Cargo.lock が無い / 古い状態を検出しない"
        lock.write_text(saved, encoding="utf-8")
        assert verify_resolution(t / "app", t, pins_now)[0] == []

        def fake(pkgs: list[tuple[str, str | None, Path]]) -> dict:
            return {"packages": [{"name": n, "source": src, "manifest_path": str(mp)} for n, src, mp in pkgs]}

        reg = "registry+https://github.com/rust-lang/crates.io-index"
        ok = fake([("alice-a", None, t / "ALICE-A/Cargo.toml"), ("alice-b", None, t / "ALICE-B/Cargo.toml"),
                   ("alice-det", reg, t / "reg/alice-det/Cargo.toml")])
        patched = fake([("alice-a", None, t / "ALICE-A/Cargo.toml"), ("alice-det", None, t / "fork-det/Cargo.toml")])
        assert judge_resolution(patched, t / "app", t, pins_now)[0], "[patch] で registry の alice-* を手元の path に差し替えたのを検出しない"
        assert judge_resolution(ok, t / "app", t, pins_now) == ([], 3), "固定外の名前の registry 依存を red にした"
        swapped = fake([("alice-a", None, t / "ALICE-A/Cargo.toml"), ("alice-b", reg, t / "reg/alice-b/Cargo.toml")])
        assert judge_resolution(swapped, t / "app", t, pins_now)[0], "固定した名前の registry への差し替えを検出しない"
        moved = fake([("alice-a", None, t / "ALICE-A/Cargo.toml"), ("alice-b", None, t / "ALICE-A/sub/Cargo.toml")])
        assert judge_resolution(moved, t / "app", t, pins_now)[0], "別の sibling の tree からの解決を検出しない"
        outside = fake([("alice-a", None, t / "elsewhere/alice-a/Cargo.toml")])
        assert judge_resolution(outside, t / "app", t, pins_now)[0], "固定した sibling の外からの解決を検出しない"
        assert judge_resolution(fake([("serde", reg, t / "r/Cargo.toml")]), t / "app", t, pins_now)[0], \
            "alice-* が 0 件で fail しない"

        setup()
        (t / "ALICE-B/Cargo.toml").rename(t / "ALICE-B/Cargo.toml.off")
        assert check(t / "app", t) == 1, "存在しない path 依存を検出しない"
    print("self-test ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=norm(Path(__file__)).parent.parent)
    ap.add_argument("--siblings", type=Path, default=None)
    ap.add_argument("--no-cargo", action="store_true")
    ap.add_argument("--verify-heads", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--emit-pins", action="store_true", help="composite action 用: `ALICE-Xxx=<sha>` を出力")
    ap.add_argument("--heads-only", action="store_true", help="composite action 用: checkout 後の HEAD 照合だけ")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if a.emit_pins:
        return emit_pins(a.root)
    if a.heads_only:
        return heads_only(a.root, a.siblings or norm(a.root).parent)
    return check(a.root, a.siblings or norm(a.root).parent, not a.no_cargo, a.verify_heads)


if __name__ == "__main__":
    sys.exit(main())
