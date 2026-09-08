import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("suite_git", ROOT / "tools" / "suite_git.py")
suite_git = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
sys.modules[SPEC.name] = suite_git
SPEC.loader.exec_module(suite_git)


def git(path: Path, *args: str) -> str:
    completed = subprocess.run(["git", "-C", str(path), *args], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return completed.stdout.strip()


class SuiteSyncIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        root = Path(self.temp.name)
        self.remote = root / "remote.git"
        self.seed = root / "seed"
        self.local = root / "local"
        subprocess.run(["git", "init", "--bare", "--initial-branch=main", str(self.remote)], check=True, stdout=subprocess.PIPE)
        subprocess.run(["git", "init", "--initial-branch=main", str(self.seed)], check=True, stdout=subprocess.PIPE)
        git(self.seed, "config", "user.email", "test@example.invalid")
        git(self.seed, "config", "user.name", "Test")
        (self.seed / "state.txt").write_text("one\n")
        git(self.seed, "add", "state.txt")
        git(self.seed, "commit", "-m", "initial")
        git(self.seed, "remote", "add", "origin", str(self.remote))
        git(self.seed, "push", "-u", "origin", "main")
        subprocess.run(["git", "clone", str(self.remote), str(self.local)], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        git(self.local, "config", "user.email", "test@example.invalid")
        git(self.local, "config", "user.name", "Test")

    def tearDown(self):
        self.temp.cleanup()

    def remote_commit(self, text="two\n"):
        (self.seed / "state.txt").write_text(text)
        git(self.seed, "add", "state.txt")
        git(self.seed, "commit", "-m", text.strip())
        git(self.seed, "push")

    def test_fast_forwards_clean_main(self):
        self.remote_commit()
        expected = git(self.seed, "rev-parse", "HEAD")
        result = suite_git.sync_repository("test", self.local, "origin", "main")
        self.assertEqual(result.state, "fast-forwarded")
        self.assertEqual(git(self.local, "rev-parse", "HEAD"), expected)

    def test_dirty_main_is_fetched_but_unchanged(self):
        before = git(self.local, "rev-parse", "HEAD")
        (self.local / "state.txt").write_text("dirty\n")
        self.remote_commit()
        result = suite_git.sync_repository("test", self.local, "origin", "main")
        self.assertEqual(result.state, "dirty")
        self.assertEqual(git(self.local, "rev-parse", "HEAD"), before)
        self.assertEqual(git(self.local, "rev-parse", "origin/main"), git(self.seed, "rev-parse", "HEAD"))

    def test_ahead_main_is_preserved(self):
        (self.local / "local.txt").write_text("local\n")
        git(self.local, "add", "local.txt")
        git(self.local, "commit", "-m", "local")
        before = git(self.local, "rev-parse", "HEAD")
        result = suite_git.sync_repository("test", self.local, "origin", "main")
        self.assertEqual(result.state, "ahead")
        self.assertEqual(git(self.local, "rev-parse", "HEAD"), before)

    def test_diverged_main_is_preserved(self):
        (self.local / "local.txt").write_text("local\n")
        git(self.local, "add", "local.txt")
        git(self.local, "commit", "-m", "local")
        self.remote_commit()
        before = git(self.local, "rev-parse", "HEAD")
        result = suite_git.sync_repository("test", self.local, "origin", "main")
        self.assertEqual(result.state, "diverged")
        self.assertEqual(git(self.local, "rev-parse", "HEAD"), before)

    def test_non_main_branch_is_never_updated(self):
        git(self.local, "switch", "-c", "feature")
        before = git(self.local, "rev-parse", "HEAD")
        self.remote_commit()
        result = suite_git.sync_repository("test", self.local, "origin", "main")
        self.assertEqual(result.state, "non-main")
        self.assertEqual(git(self.local, "rev-parse", "HEAD"), before)


class BranchClassificationTest(unittest.TestCase):
    def test_uses_pr_state_including_rebase_or_squash_merge(self):
        branches = [
            {"name": "main", "protected": True, "commit": {"sha": "m"}},
            {"name": "feature/merged", "protected": False, "commit": {"sha": "not-an-ancestor"}},
            {"name": "feature/open", "protected": False, "commit": {"sha": "o"}},
            {"name": "feature/closed", "protected": False, "commit": {"sha": "c"}},
            {"name": "feature/reused", "protected": False, "commit": {"sha": "new"}},
            {"name": "orphan", "protected": False, "commit": {"sha": "x"}},
        ]
        pulls = [
            {"number": 1, "state": "closed", "merged_at": "2026-09-01T00:00:00Z", "head": {"ref": "feature/merged", "label": "feature/merged", "sha": "not-an-ancestor"}},
            {"number": 2, "state": "open", "merged_at": None, "head": {"ref": "feature/open", "label": "feature/open"}},
            {"number": 3, "state": "closed", "merged_at": None, "head": {"ref": "feature/closed", "label": "feature/closed"}},
            {"number": 4, "state": "closed", "merged_at": "2026-09-02T00:00:00Z", "head": {"ref": "feature/reused", "label": "feature/reused", "sha": "old"}},
        ]
        result = {row["branch"]: row["disposition"] for row in suite_git.classify_branches(branches, pulls, "main")}
        self.assertEqual(result, {"feature/open": "active", "feature/merged": "delete-eligible", "main": "protected", "feature/closed": "review", "feature/reused": "review", "orphan": "review"})


if __name__ == "__main__":
    unittest.main()
