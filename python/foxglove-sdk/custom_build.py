"""
A PEP 517 build backend that wraps maturin, in order to run codegen for the
notebook frontend.

This build backend MUST be a transparent wrapper around maturin when building
from sdist. In other words, running `maturin build` against the sdist MUST yield
exactly the same result as running `pip wheel`. The CI pipeline currently
depends on this property, because it uses PyO3/maturin-action to build against
the sdist, and effectively bypasses this build backend.
"""

import os
import subprocess
import sys
from pathlib import Path

import maturin  # type: ignore
from maturin import get_requires_for_build_editable  # noqa: F401
from maturin import get_requires_for_build_sdist  # noqa: F401
from maturin import get_requires_for_build_wheel  # noqa: F401
from maturin import prepare_metadata_for_build_wheel  # noqa: F401

FRONTEND_TARGETS = ["python/foxglove/notebook/static/widget.js"]


def _assert_frontend_targets_exist() -> None:
    for target in FRONTEND_TARGETS:
        path = Path.cwd() / target
        assert path.exists(), f"{target} not found"


def _frontend_codegen(editable: bool = False) -> None:
    # Suppress warning about `build-backend` not being set to `maturin` in
    # pyproject.toml.
    os.environ["MATURIN_NO_MISSING_BUILD_BACKEND_WARNING"] = "1"

    # If we're building from an sdist, there's nothing to do here. We packaged
    # the compiled frontend assets when packaging the sdist, and we omit the
    # sources.
    notebook_frontend = Path.cwd() / "notebook-frontend"
    if not notebook_frontend.exists():
        print("[custom-build] no frontend sources; skipping frontend codegen.")
        _assert_frontend_targets_exist()
        return

    # For editable installs, don't minify.
    build_target = "build" if editable else "build:prod"
    cmds = [
        ["yarn", "install"],
        ["yarn", build_target],
    ]
    for cmd in cmds:
        print(f"[custom-build] running {' '.join(cmd)}")
        sys.stdout.flush()
        subprocess.run(cmd, cwd=notebook_frontend, check=True)

    _assert_frontend_targets_exist()


def build_wheel(*args, **kwargs):  # type: ignore
    _frontend_codegen()
    return maturin.build_wheel(*args, **kwargs)


def build_sdist(*args, **kwargs):  # type: ignore
    _frontend_codegen()
    return maturin.build_sdist(*args, **kwargs)


def build_editable(*args, **kwargs):  # type: ignore
    _frontend_codegen(editable=True)
    return maturin.build_editable(*args, **kwargs)
