"""Canix Blender (MCP) session bootstrap.

Registers a dedicated, session-local ``canix`` extension repository pointing
at the immutable packaged extension tree, then enables it with safe session
defaults. There is no install step: Blender discovers the unpacked tree
through the repository directly, so nothing is copied, reinstalled, or
version-drifted at launch.

Guarantees:
- The normal user extension root is never overridden or written.
- Opt-in sessions disable automatic preference persistence. Everyday sessions
  preserve the user's preference-saving setting and use a stable managed repo.
  The bootstrap itself never calls ``wm.save_userpref``.
- An existing ``canix`` repository pointing anywhere else is a loud failure,
  never silently adopted or replaced.
- An existing manual ``allow_ai_control = False`` reservation is preserved.
- Any bootstrap failure raises loudly; the exit code of a ``--python``
  script must never be used for failure signaling because exiting such a
  script quits the whole Blender session.

Run with: blender --python blender-mcp-startup.py [file].
"""

from __future__ import annotations

import os
import sys

REPO_MODULE = os.environ.get("CANIX_BLENDER_MCP_REPO", "canix")
REPO_NAME = "Canix"
PKG_ID = "blender_mcp"


def _fail(message: str) -> None:
    raise RuntimeError(f"canix blender-mcp: {message}")


def _addon_module() -> str:
    return f"bl_ext.{REPO_MODULE}.{PKG_ID}"


def _ensure_repo() -> str:
    """Return the validated repository directory for our module id.

    The directory must be the immutable packaged extension tree holding the
    ``blender_mcp`` payload; repository caching is disabled so Blender never
    attempts writes beside immutable code.
    """
    import bpy

    expected = os.environ.get("CANIX_BLENDER_MCP_REPO_DIR", "")
    if not expected or not os.path.isabs(expected):
        _fail(f"missing absolute CANIX_BLENDER_MCP_REPO_DIR: {expected!r}")
    expected = os.path.normpath(expected)
    manifest = os.path.join(expected, PKG_ID, "blender_manifest.toml")
    if not os.path.isfile(manifest):
        _fail(f"repository directory has no extension payload: {manifest!r}")

    repos = bpy.context.preferences.extensions.repos
    for repo in repos:
        if repo.module == REPO_MODULE:
            actual = os.path.normpath(repo.directory or "")
            if os.path.realpath(actual) != os.path.realpath(expected):
                _fail(
                    f"existing {REPO_MODULE!r} repository points at {actual!r}, "
                    f"expected {expected!r}; refusing to adopt or replace it"
                )
            # A saved stable HM path and its current store target identify
            # the same payload; preserve the existing repository settings.
            repo.use_cache = False
            return expected
    repo = repos.new(
        name=REPO_NAME,
        module=REPO_MODULE,
        custom_directory=expected,
        remote_url="",
        source="USER",
    )
    repo.use_cache = False
    return expected


def _payload_version(repo_dir: str) -> str:
    """Read the packaged manifest version pinned into the immutable tree."""
    import tomllib

    manifest = os.path.join(repo_dir, PKG_ID, "blender_manifest.toml")
    try:
        with open(manifest, "rb") as handle:
            data = tomllib.load(handle)
    except (OSError, ValueError) as error:
        _fail(f"cannot read packaged manifest {manifest!r}: {error}")
    version = data.get("version", "")
    if not isinstance(version, str) or not version:
        _fail(f"packaged manifest has no version: {manifest!r}")
    return version


def main() -> int:
    import bpy

    # Everyday sessions use a stable HM-managed repository path, so saving
    # preferences must keep working. Explicit opt-in sessions remain temporary.
    persistent = os.environ.get("CANIX_BLENDER_MCP_PERSISTENT") == "1"
    if not persistent:
        bpy.context.preferences.use_preferences_save = False

    repo_dir = _ensure_repo()

    expected_version = os.environ.get("CANIX_BLENDER_MCP_VERSION", "")
    if not expected_version:
        _fail("missing CANIX_BLENDER_MCP_VERSION")
    payload_version = _payload_version(repo_dir)
    if payload_version != expected_version:
        _fail(f"packaged extension is {payload_version}, expected {expected_version}")

    module = _addon_module()
    if module not in bpy.context.preferences.addons:
        try:
            bpy.ops.preferences.addon_enable(module=module)
        except Exception as error:  # noqa: BLE001 - re-raised with context
            _fail(f"addon_enable failed for {module}: {error}")

    addon = bpy.context.preferences.addons.get(module)
    if addon is None:
        _fail(f"{module} not present after addon_enable")
    assert addon is not None

    # Privacy default for AI sessions. Deliberately leaves auto_connect and
    # allow_ai_control untouched so existing user choices, including a
    # manual AI-control reservation, are preserved.
    if hasattr(addon.preferences, "telemetry_consent"):
        addon.preferences.telemetry_consent = False

    # Start the bridge deterministically instead of relying on the add-on's
    # startup timer; the timer remains as an upstream fallback.
    server = None
    try:
        import importlib

        existing = getattr(bpy.types, "blendermcp_server", None)
        if existing is not None and getattr(existing, "running", False):
            existing.ensure_heartbeat_timer()
            server = existing
        else:
            package = importlib.import_module(module)
            server = package.BlenderMCPServer()
            bpy.types.blendermcp_server = server
            server.start()
    except Exception:  # noqa: BLE001 - re-raised with context
        import traceback

        traceback.print_exc()
        _fail("bridge start failed")

    running = bool(getattr(server, "running", False))
    print(
        "canix blender-mcp: extension enabled "
        f"(bridge running: {running}, everyday: {persistent}); bootstrap did not save preferences"
    )
    sys.stdout.flush()
    if not running:
        _fail("bridge is not running after explicit start")
    return 0


if __name__ == "__main__":
    # NOTE: never sys.exit() here: exiting a Blender --python script quits
    # the whole Blender session. Failures raise.
    if main() != 0:
        raise RuntimeError("canix blender-mcp: bootstrap failed")
