"""Exercise the real registry resolver/writer with only Blender data stubbed."""

import argparse
import importlib
import json
import os
import stat
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace


def load_package(name, path):
    # Bypass the composition root, which imports UI classes requiring Blender.
    # Relative imports still load the actual modules from the supplied payload.
    package = ModuleType(name)
    package.__path__ = [str(path)]
    sys.modules[name] = package


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("extension", type=Path)
    parser.add_argument("--server", type=Path)
    args = parser.parse_args()
    os.umask(int(os.environ.get("UMASK", "022"), 8))

    bpy = ModuleType("bpy")
    bpy.app = SimpleNamespace(version_string="fixture", binary_path="")
    bpy.data = SimpleNamespace(filepath="first.blend", is_dirty=False)
    bpy.context = SimpleNamespace(
        scene=SimpleNamespace(name="Fixture"),
        preferences=SimpleNamespace(addons={}),
    )
    sys.modules["bpy"] = bpy
    load_package("registry_extension", args.extension)
    lifecycle = importlib.import_module("registry_extension.bridge.lifecycle")
    bridge = lifecycle.BridgeLifecycleMixin()
    bridge.running = True
    bridge.socket = object()  # The writer only checks that the listener is live.
    bridge.port = 19876
    bridge._write_registry_record()
    bpy.data.filepath = "second.blend"
    bridge._write_registry_record()

    record = Path(bridge.registry_path)
    registry = record.parent
    server_registry = None
    if args.server is not None:
        load_package("registry_server", args.server)
        instances = importlib.import_module("registry_server.transport.instances")
        server_registry = str(instances.runtime_directory())
    print(
        json.dumps(
            {
                "registry": str(registry),
                "server_registry": server_registry,
                "record": str(record),
                "contents": json.loads(record.read_text(encoding="utf-8")),
                "modes": {
                    name: format(stat.S_IMODE(path.stat().st_mode), "03o")
                    for name, path in (
                        ("parent", registry.parent),
                        ("registry", registry),
                        ("record", record),
                    )
                },
                "temporaries": sorted(path.name for path in registry.glob("*.tmp")),
            }
        )
    )


if __name__ == "__main__":
    main()
