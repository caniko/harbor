"""
Pandoc ODT export plugin for the anx article toolchain.

Converts a LaTeX manuscript to ODT via the pandoc-syndb fork, applying
a Lua filter for composite figure injection and referencing a BibTeX
bibliography.

Protocol
--------
CLI argument::

    --plugin-context <json>

Input JSON fields
    action : str
        Operation to perform (``"export-odt"``).
    article_dir : str
        Path to the article root directory.
    tex_main : str
        Filename of the LaTeX main file (relative to *article_dir*).
    output : str
        Desired ODT filename (relative to *article_dir*).
    pandoc_binary : str, optional
        Pandoc executable (default ``"pandoc"``).
    bibliography : str, optional
        BibTeX file (relative to *article_dir*; default ``"references.bib"``).
    lua_filter : str, optional
        Lua filter path (relative to *article_dir*).
    reference_odt : str, optional
        Reference style ODT (relative to *article_dir*).

Output JSON (stdout)
    ``{"status": "ok", "output": "<absolute-path-to-odt>"}``
    or ``{"status": "error", "error": "<message>"}``
"""

from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
from pathlib import Path

__all__ = ["export_odt", "main"]

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Context parsing
# ---------------------------------------------------------------------------

def parse_context(argv: list[str] | None = None) -> dict:
    """Return the ``--plugin-context`` JSON parsed into a dict."""
    parser = argparse.ArgumentParser(
        prog="anx-plugin-pandoc",
        description="Pandoc ODT export plugin for the anx article toolchain",
    )
    parser.add_argument(
        "--plugin-context",
        required=True,
        help="JSON string with action, article_dir, tex_main, output",
    )
    args, _unknown = parser.parse_known_args(argv)
    return json.loads(args.plugin_context)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _resolve(article_dir: Path, path_str: str | None) -> Path | None:
    """Resolve a relative path against *article_dir*, or return ``None``."""
    if not path_str:
        return None
    p = Path(path_str)
    return (article_dir / p).resolve() if not p.is_absolute() else p.resolve()


# ---------------------------------------------------------------------------
# Actions
# ---------------------------------------------------------------------------

def export_odt(context: dict) -> dict:
    """Convert a LaTeX manuscript to ODT via pandoc.

    Returns a ``{"status": …, …}`` result dict for JSON serialisation.
    """
    article_dir = Path(context["article_dir"]).resolve()

    tex_main = context["tex_main"]
    output = context["output"]
    pandoc_binary = context.get("pandoc_binary", "pandoc")
    bibliography = context.get("bibliography", "references.bib")
    lua_filter = context.get("lua_filter")
    reference_odt = context.get("reference_odt")

    tex_path = (article_dir / tex_main).resolve()
    output_path = (article_dir / output).resolve()

    # ----- Validate inputs ------------------------------------------------
    if not tex_path.is_file():
        return {"status": "error", "error": f"TeX main not found: {tex_path}"}

    # ----- Build pandoc command -------------------------------------------
    cmd = [
        pandoc_binary,
        str(tex_path),
        "--from", "latex",
        "--to", "odt",
        "--output", str(output_path),
    ]

    # Bibliography  (optional; missing file is a warning, not a hard error)
    bib_path = _resolve(article_dir, bibliography)
    if bib_path and bib_path.is_file():
        cmd.extend(["--bibliography", str(bib_path)])
    elif bibliography:
        logger.warning("Bibliography not found: %s", bib_path)

    # Lua filter for composite figures  (optional)
    lua_path = _resolve(article_dir, lua_filter)
    if lua_path and lua_path.is_file():
        cmd.extend(["--lua-filter", str(lua_path)])
    elif lua_filter:
        logger.warning("Lua filter not found: %s", lua_path)

    # Reference style ODT  (optional)
    ref_path = _resolve(article_dir, reference_odt)
    if ref_path and ref_path.is_file():
        cmd.extend(["--reference-doc", str(ref_path)])
    elif reference_odt:
        logger.warning("Reference ODT not found: %s", ref_path)

    # ----- Execute --------------------------------------------------------
    logger.info("Running: %s", " ".join(str(c) for c in cmd))

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return {
            "status": "error",
            "error": f"pandoc binary not found: {pandoc_binary}",
        }

    if proc.returncode != 0:
        return {
            "status": "error",
            "error": (
                f"pandoc failed (exit {proc.returncode}):\n"
                f"{proc.stderr.strip()}"
            ),
        }

    if not output_path.is_file():
        return {"status": "error", "error": f"Output not created: {output_path}"}

    return {"status": "ok", "output": str(output_path)}


# ---------------------------------------------------------------------------
# CLI entry point  (also invoked via ``python -m anx_plugin_pandoc``)
# ---------------------------------------------------------------------------

def main(argv: list[str] | None = None) -> None:
    """CLI entry point.

    Reads ``--plugin-context``, dispatches to the requested *action*, and
    writes a JSON result to stdout.
    """
    logging.basicConfig(
        level=logging.INFO,
        format="%(levelname)s: %(message)s",
        stream=sys.stderr,
    )

    try:
        context = parse_context(argv)
        action = context.get("action")

        if action == "export-odt":
            result = export_odt(context)
        else:
            result = {"status": "error", "error": f"Unknown action: {action!r}"}

    except Exception as exc:
        result = {"status": "error", "error": str(exc)}

    json.dump(result, sys.stdout)
    sys.stdout.flush()

    if result.get("status") == "error":
        sys.exit(1)


if __name__ == "__main__":
    main()
