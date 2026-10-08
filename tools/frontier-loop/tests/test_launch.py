import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPT_DIR))
import launch  # noqa: E402


class LaunchTests(unittest.TestCase):
    def test_worker_home_is_writable_and_reuses_auth_config_without_copying(self):
        with tempfile.TemporaryDirectory() as temp_name:
            root = Path(temp_name)
            user_home = root / "user"
            source_home = user_home / ".codex"
            runtime_root = root / "runtime"
            source_home.mkdir(parents=True)
            (source_home / "auth.json").write_text("auth fixture", encoding="utf-8")
            (source_home / "config.toml").write_text("model = 'fixture'\n", encoding="utf-8")

            with (
                patch.object(launch.Path, "home", return_value=user_home),
                patch.dict(
                    os.environ,
                    {"CODEX_HOME": "", "FRONTIER_CODEX_HOME": str(runtime_root)},
                ),
            ):
                env = launch.codex_environment(94)

            runtime_home = runtime_root / "track-94"
            self.assertEqual(Path(env["CODEX_HOME"]), runtime_home)
            self.assertTrue(runtime_home.is_dir())
            for name in ("auth.json", "config.toml"):
                link = runtime_home / name
                self.assertTrue(link.is_symlink())
                self.assertEqual(Path(os.readlink(link)), source_home / name)

    def test_explicit_codex_home_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp_name:
            user_home = Path(temp_name) / "user"
            configured_home = Path(temp_name) / "configured"
            with (
                patch.object(launch.Path, "home", return_value=user_home),
                patch.dict(
                    os.environ,
                    {
                        "CODEX_HOME": str(configured_home),
                        "FRONTIER_CODEX_HOME": str(Path(temp_name) / "override"),
                    },
                ),
            ):
                env = launch.codex_environment(95)

            self.assertEqual(env["CODEX_HOME"], str(configured_home))
            self.assertFalse(configured_home.exists())


if __name__ == "__main__":
    unittest.main()
