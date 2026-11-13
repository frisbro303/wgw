"""MkDocs hook: put wgw (dist/) and the examples into the built site, so the docs' live figures run."""
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def on_post_build(config, **kwargs):
    if not (ROOT / "dist" / "wgw.js").exists():
        raise SystemExit("dist/ is missing: run ./build.sh before building the docs, whose figures run it")
    site = Path(config["site_dir"])
    shutil.copytree(ROOT / "dist", site / "wgw", dirs_exist_ok=True)
    shutil.copytree(ROOT / "examples", site / "examples", dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns("__pycache__", "index.html"))
