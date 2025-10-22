"""
A PEP 517 build backend that wraps maturin, in order to run codegen for the
notebook frontend.
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


def _frontend_codegen(editable: bool = False) -> None:
    if os.environ.get("SKIP_FRONTEND_CODEGEN"):
        print("[custom-build] SKIP_FRONTEND_CODEGEN set; skipping frontend codegen.")
        return

    # We package the compiled frontend assets in sdists, and omit the sources.
    notebook_frontend = Path.cwd() / "notebook-frontend"
    if not notebook_frontend.exists():
        print("[custom-build] no frontend sources; skipping frontend codegen.")
        return

    build_target = "build" if editable else "build:prod"
    cmds = [
        ["yarn"],
        ["yarn", build_target],
    ]

    for cmd in cmds:
        print(f"[custom-build] running {' '.join(cmd)}")
        sys.stdout.flush()
        subprocess.run(cmd, cwd=notebook_frontend, check=True)


def build_wheel(*args, **kwargs):  # type: ignore
    _frontend_codegen()
    return maturin.build_wheel(*args, **kwargs)


def build_sdist(*args, **kwargs):  # type: ignore
    _frontend_codegen()
    return maturin.build_sdist(*args, **kwargs)


def build_editable(*args, **kwargs):  # type: ignore
    _frontend_codegen(editable=True)
    return maturin.build_editable(*args, **kwargs)
