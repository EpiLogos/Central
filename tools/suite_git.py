#!/usr/bin/env python3
"""Conservative Git lifecycle tooling for the O:I product suite.

The two commands deliberately separate local checkout maintenance from hosted
branch retirement:

* ``sync`` fetches/prunes repositories and fast-forwards only clean ``main``
  checkouts that have no local commits.
* ``branches`` classifies hosted branches from pull-request state.  It can
  delete only branches whose pull request is already merged.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterable


DEFAULT_REPOSITORIES = {
    "O:I": "Work/O-I",
    "Central": "Work/Central",
    "AIKit": "Work/ai-kit",
    "Actuation": "Work/Actuation",
    "Factory": "Work/Software-Factory",
    "Workcell": "Work/Workcell",
    "QL": "Work/Quaternal-Logic",
}


def run_git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def git_text(repo: Path, *args: str) -> str:
    return run_git(repo, *args).stdout.strip()


@dataclass
class SyncResult:
    name: str
    path: str
    state: str
    detail: str
    before: str | None = None
    after: str | None = None
    ahead: int | None = None
    behind: int | None = None


def sync_repository(name: str, repo: Path, remote: str, branch: str) -> SyncResult:
    if not repo.exists():
        return SyncResult(name, str(repo), "missing", "repository path does not exist")
    if run_git(repo, "rev-parse", "--git-dir", check=False).returncode:
        return SyncResult(name, str(repo), "not-git", "path is not a Git repository")

    before = git_text(repo, "rev-parse", "HEAD")
    fetch = run_git(repo, "fetch", "--prune", "--no-tags", remote, check=False)
    if fetch.returncode:
        detail = fetch.stderr.strip() or fetch.stdout.strip() or "git fetch failed"
        return SyncResult(name, str(repo), "fetch-failed", detail, before=before, after=before)

    current = git_text(repo, "symbolic-ref", "--quiet", "--short", "HEAD") if run_git(
        repo, "symbolic-ref", "--quiet", "--short", "HEAD", check=False
    ).returncode == 0 else "DETACHED"
    if current != branch:
        return SyncResult(name, str(repo), "non-main", f"on {current}; fetched and preserved", before, before)
    if git_text(repo, "status", "--porcelain"):
        return SyncResult(name, str(repo), "dirty", "working tree has changes; fetched and preserved", before, before)

    remote_ref = f"refs/remotes/{remote}/{branch}"
    if run_git(repo, "show-ref", "--verify", "--quiet", remote_ref, check=False).returncode:
        return SyncResult(name, str(repo), "missing-remote-main", f"{remote}/{branch} does not exist", before, before)
    counts = git_text(repo, "rev-list", "--left-right", "--count", f"HEAD...{remote}/{branch}").split()
    ahead, behind = map(int, counts)
    if ahead and behind:
        return SyncResult(name, str(repo), "diverged", "local and remote histories differ; preserved", before, before, ahead, behind)
    if ahead:
        return SyncResult(name, str(repo), "ahead", "local commits are not on remote; preserved", before, before, ahead, behind)
    if not behind:
        return SyncResult(name, str(repo), "current", "already matches remote main", before, before, ahead, behind)

    update = run_git(repo, "merge", "--ff-only", f"{remote}/{branch}", check=False)
    if update.returncode:
        detail = update.stderr.strip() or update.stdout.strip() or "fast-forward failed"
        return SyncResult(name, str(repo), "ff-failed", detail, before, before, ahead, behind)
    after = git_text(repo, "rev-parse", "HEAD")
    return SyncResult(name, str(repo), "fast-forwarded", f"advanced by {behind} commit(s)", before, after, ahead, behind)


def load_repositories(ground: Path, config: Path | None) -> list[tuple[str, Path]]:
    values = DEFAULT_REPOSITORIES
    if config:
        decoded = json.loads(config.read_text())
        values = decoded.get("repositories", decoded)
        if not isinstance(values, dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in values.items()):
            raise ValueError("repository config must be an object of name-to-relative-path strings")
    return [(name, ground / relative) for name, relative in values.items()]


def sync_command(args: argparse.Namespace) -> int:
    ground = args.ground.resolve()
    results = [sync_repository(name, path, args.remote, args.branch) for name, path in load_repositories(ground, args.config)]
    payload = {
        "schema": "central.suite-git-sync/v1",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "ground": str(ground),
        "results": [asdict(result) for result in results],
    }
    emit(payload, sync_markdown(results), args)
    return 0


class GitHub:
    def __init__(self, token: str, api_url: str = "https://api.github.com") -> None:
        self.token = token
        self.api_url = api_url.rstrip("/")

    def request(self, method: str, path: str) -> Any:
        request = urllib.request.Request(
            f"{self.api_url}{path}",
            method=method,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {self.token}",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "central-suite-git/1",
            },
        )
        try:
            with urllib.request.urlopen(request) as response:
                body = response.read()
                return json.loads(body) if body else None
        except urllib.error.HTTPError as error:
            body = error.read().decode(errors="replace")
            raise RuntimeError(f"GitHub {method} {path} failed ({error.code}): {body}") from error

    def pages(self, path: str) -> Iterable[dict[str, Any]]:
        separator = "&" if "?" in path else "?"
        page = 1
        while True:
            rows = self.request("GET", f"{path}{separator}per_page=100&page={page}")
            if not isinstance(rows, list):
                raise RuntimeError(f"expected a list from GitHub endpoint {path}")
            yield from rows
            if len(rows) < 100:
                return
            page += 1


def classify_branches(branches: list[dict[str, Any]], pulls: list[dict[str, Any]], default_branch: str) -> list[dict[str, Any]]:
    by_head: dict[str, list[dict[str, Any]]] = {}
    for pull in pulls:
        head_data = pull.get("head", {})
        base_data = pull.get("base", {})
        head = head_data.get("ref")
        head_repo = (head_data.get("repo") or {}).get("full_name")
        base_repo = (base_data.get("repo") or {}).get("full_name")
        # Deleted branch metadata may omit head.repo. A matching base/head repo is
        # otherwise required so a fork PR cannot authorize deletion here.
        same_repository = head_repo == base_repo if head_repo and base_repo else ":" not in head_data.get("label", "")
        if head and same_repository:
            by_head.setdefault(head, []).append(pull)
    results = []
    for branch in branches:
        name = branch["name"]
        related = by_head.get(name, [])
        open_prs = [pr for pr in related if pr.get("state") == "open"]
        merged_prs = [pr for pr in related if pr.get("merged_at")]
        merged_at_current_tip = [pr for pr in merged_prs if pr.get("head", {}).get("sha") == branch["commit"]["sha"]]
        closed_prs = [pr for pr in related if pr.get("state") == "closed" and not pr.get("merged_at")]
        if name == default_branch or branch.get("protected"):
            disposition, reason = "protected", "default or protected branch"
        elif open_prs:
            disposition, reason = "active", f"open PR #{open_prs[-1]['number']}"
        elif merged_at_current_tip:
            disposition, reason = "delete-eligible", f"current tip belongs to merged PR #{merged_at_current_tip[-1]['number']}"
        elif merged_prs:
            disposition, reason = "review", f"branch advanced after merged PR #{merged_prs[-1]['number']}"
        elif closed_prs:
            disposition, reason = "review", f"closed unmerged PR #{closed_prs[-1]['number']} may contain unique work"
        else:
            disposition, reason = "review", "no pull-request history found"
        results.append({"branch": name, "sha": branch["commit"]["sha"], "disposition": disposition, "reason": reason})
    return sorted(results, key=lambda row: (row["disposition"], row["branch"]))


def branches_command(args: argparse.Namespace) -> int:
    token = args.token or os.environ.get("GITHUB_TOKEN")
    if not token:
        raise ValueError("GitHub token required via --token or GITHUB_TOKEN")
    github = GitHub(token, args.api_url)
    quoted_repo = urllib.parse.quote(args.repository, safe="/")
    metadata = github.request("GET", f"/repos/{quoted_repo}")
    branches = list(github.pages(f"/repos/{quoted_repo}/branches"))
    pulls = list(github.pages(f"/repos/{quoted_repo}/pulls?state=all"))
    classified = classify_branches(branches, pulls, metadata["default_branch"])
    deleted: list[str] = []
    if args.delete_merged:
        for row in classified:
            if row["disposition"] != "delete-eligible":
                continue
            encoded = urllib.parse.quote(row["branch"], safe="")
            current_ref = github.request("GET", f"/repos/{quoted_repo}/git/ref/heads/{encoded}")
            current_sha = current_ref.get("object", {}).get("sha")
            if current_sha != row["sha"]:
                row["disposition"] = "review"
                row["reason"] = "branch tip changed after classification; deletion refused"
                continue
            github.request("DELETE", f"/repos/{quoted_repo}/git/refs/heads/{encoded}")
            deleted.append(row["branch"])
    payload = {
        "schema": "central.branch-hygiene/v1",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "repository": args.repository,
        "default_branch": metadata["default_branch"],
        "delete_merged": args.delete_merged,
        "deleted": deleted,
        "branches": classified,
    }
    emit(payload, branches_markdown(args.repository, classified, deleted), args)
    return 0


def sync_markdown(results: list[SyncResult]) -> str:
    lines = ["# Suite Git sync", "", "| Repository | State | Detail |", "|---|---|---|"]
    lines.extend(f"| {r.name} | `{r.state}` | {r.detail.replace('|', '\\|')} |" for r in results)
    return "\n".join(lines) + "\n"


def branches_markdown(repository: str, rows: list[dict[str, Any]], deleted: list[str]) -> str:
    lines = [f"# Branch hygiene: {repository}", "", "Classification uses pull-request state, so squash- and rebase-merged branches are recognized.", ""]
    if deleted:
        lines += ["Deleted merged branches: " + ", ".join(f"`{name}`" for name in deleted), ""]
    lines += ["| Branch | Disposition | Evidence |", "|---|---|---|"]
    lines.extend(f"| `{r['branch']}` | `{r['disposition']}` | {r['reason']} |" for r in rows)
    return "\n".join(lines) + "\n"


def emit(payload: dict[str, Any], markdown: str, args: argparse.Namespace) -> None:
    encoded = json.dumps(payload, indent=2) + "\n"
    if args.json_output:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(encoded)
    if args.markdown_output:
        args.markdown_output.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_output.write_text(markdown)
    if not args.json_output and not args.markdown_output:
        sys.stdout.write(encoded if args.format == "json" else markdown)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subcommands = result.add_subparsers(required=True)
    sync = subcommands.add_parser("sync", help="fetch/prune and safely fast-forward suite main checkouts")
    sync.add_argument("--ground", type=Path, default=Path.cwd())
    sync.add_argument("--config", type=Path)
    sync.add_argument("--remote", default="origin")
    sync.add_argument("--branch", default="main")
    add_outputs(sync)
    sync.set_defaults(run=sync_command)
    branches = subcommands.add_parser("branches", help="classify remote branches using GitHub pull-request state")
    branches.add_argument("--repository", required=True, help="OWNER/REPOSITORY")
    branches.add_argument("--token")
    branches.add_argument("--api-url", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    branches.add_argument("--delete-merged", action="store_true")
    add_outputs(branches)
    branches.set_defaults(run=branches_command)
    return result


def add_outputs(command: argparse.ArgumentParser) -> None:
    command.add_argument("--format", choices=("json", "markdown"), default="markdown")
    command.add_argument("--json-output", type=Path)
    command.add_argument("--markdown-output", type=Path)


def main() -> int:
    args = parser().parse_args()
    try:
        return args.run(args)
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        print(f"suite-git: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
