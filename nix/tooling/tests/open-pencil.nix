# OpenPencil target toolchain: focused packages, a static package-smoke
# check, and a display-isolated desktop/MCP runtime gate.
#
# The sandbox uses Mesa surfaceless EGL with explicit GLVND vendor
# discovery and Xvfb for windowing. Native edit, export, and reopen are
# required assertions, including text pixels from inactive documents.
{
  pkgs,
  openPencilCli,
  openPencilDesktop,
}: let
  driver = pkgs.writeText "open-pencil-mcp-proof.py" ''
    import json
    import base64
    import io
    import subprocess
    import sys
    from PIL import Image

    FIG = sys.argv[1]
    MARKER = "MCP-EDIT-PROOF"
    # Optional third argument: assert-only mode used after restart.
    # Skips mutation and asserts MARKER is already present.
    ASSERT_ONLY = len(sys.argv) > 2 and sys.argv[2] == "assert"
    # Optional fourth argument: seconds to poll open_file while the
    # server reports no connected app. Socket existence only proves
    # the server started, not that the document is ready.
    WAIT_READY = int(sys.argv[3]) if len(sys.argv) > 3 else 0

    def rpc(proc, method, params=None, ident=1, timeout=60):
        import time
        proc.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "id": ident, "method": method,
             "params": params or {}}) + "\n")
        proc.stdin.flush()
        deadline = time.time() + timeout
        pending = b""
        # stdio bridges may emit progress notifications before the
        # matching response; skip non-matching ids within the deadline.
        fd = proc.stdout.fileno()
        while True:
            message, pending = read_message_from(
                proc.stdout, fd, pending, deadline, method)
            if message.get("id") == ident:
                if "error" in message:
                    raise RuntimeError(
                        f"{method} JSON-RPC error: "
                        f"{json.dumps(message['error'])[:300]}")
                return message


    def read_message_from(stream, fd, pending, deadline, method):
        import select
        import time
        buffered = pending
        while True:
            if b"\n" in buffered:
                line, rest = buffered.split(b"\n", 1)
                return json.loads(line), rest
            import os
            remaining = deadline - time.time()
            if remaining <= 0:
                raise RuntimeError(
                    f"timed out waiting for {method} response")
            ready, _, _ = select.select([fd], [], [], remaining)
            if not ready:
                raise RuntimeError(
                    f"timed out waiting for {method} response")
            chunk = os.read(fd, 65536)
            if not chunk:
                raise RuntimeError(f"stdio closed waiting for {method}")
            buffered += chunk


    def call(proc, tool, arguments, ident):
        response = rpc(proc, "tools/call",
                       {"name": tool, "arguments": arguments}, ident=ident)
        result = response.get("result", {})
        # Protocol-correct failure surface: top-level isError first,
        # then legacy item-level flags, then connectivity text.
        if result.get("isError") is True:
            raise RuntimeError(
                f"{tool} failed: {json.dumps(result)[:300]}")
        results = result.get("content", [])
        assert isinstance(results, list) and results, \
            f"{tool}: unexpected result shape: {json.dumps(response)[:300]}"
        failures = [item.get("text", "") for item in results
                    if isinstance(item, dict) and item.get("isError")]
        assert not failures, f"{tool} failed: {failures[0][:300] if failures else response}"
        text = json.dumps(response).lower()
        assert "not connected" not in text, \
            f"{tool}: app not connected: {text[:300]}"
        return response

    def content(response):
        return json.loads(response["result"]["content"][0]["text"])

    def export_text(proc, target, text_id, ident):
        result = call(proc, "export_image", {
            **target, "ids": [text_id], "format": "PNG", "maxEdge": 4096
        }, ident=ident)
        images = [item for item in result["result"]["content"]
                  if item.get("type") == "image"]
        assert len(images) == 1, "expected one exported image"
        data = base64.b64decode(images[0]["data"], validate=True)
        image = Image.open(io.BytesIO(data)).convert("RGBA")
        alpha = image.getchannel("A")
        ink = sum(count for value, count in enumerate(alpha.histogram()) if value)
        assert ink > 50, f"text export is blank ({ink} nontransparent pixels)"
        assert ink < image.width * image.height, "expected transparent text background"
        return data

    proc = subprocess.Popen(
        ["openpencil-mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        text=True)
    try:
        hello = rpc(proc, "initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "nix-check", "version": "0"}})
        assert hello["result"]["serverInfo"]["name"] == "open-pencil", hello
        proc.stdin.write(json.dumps(
            {"jsonrpc": "2.0",
             "method": "notifications/initialized"}) + "\n")
        proc.stdin.flush()
        import time as _time
        opened = None
        deadline = _time.time() + WAIT_READY
        while True:
            try:
                opened = call(proc, "open_file", {"path": FIG}, ident=2)
                break
            except RuntimeError as exc:
                if "not connected" in str(exc).lower() \
                        and _time.time() < deadline:
                    _time.sleep(5)
                    continue
                raise
        resolved = content(opened)["target"]
        target = {"document_id": resolved["documentId"], "page_id": resolved["pageId"]}
        tree = call(proc, "get_page_tree", target, ident=3)
        # Find a TEXT node id through the document tree.
        ids = []

        def collect(node):
            if isinstance(node, dict):
                if node.get("type") == "TEXT":
                    ids.append(node.get("id"))
                for child in node.get("children", []) or []:
                    collect(child)

        try:
            doc = json.loads(tree["result"]["content"][0]["text"])
            collect(doc)
        except (KeyError, IndexError, json.JSONDecodeError):
            pass
        assert ids, \
            f"no TEXT node id discovered: {json.dumps(tree)[:300]}"
        if ASSERT_ONLY:
            node = content(call(proc, "get_node", {**target, "id": ids[0], "depth": 0}, ident=4))
            assert node.get("characters") == MARKER, \
                f"marker missing after reopen: {json.dumps(node)[:300]}"
            export_text(proc, target, ids[0], ident=5)
            print(json.dumps({"reopened": True, "textId": ids[0]}))
        else:
            edited = call(proc, "set_text",
                          {**target, "id": ids[0], "text": MARKER}, ident=4)
            baseline = export_text(proc, target, ids[0], ident=5)
            other = content(call(proc, "create_page", {**target, "name": "Other page"}, ident=6))
            call(proc, "switch_page", {**target, "page": other["id"]}, ident=7)
            assert export_text(proc, target, ids[0], ident=8) == baseline
            call(proc, "new_document", {}, ident=9)
            before = call(proc, "list_documents", {}, ident=10)
            assert export_text(proc, target, ids[0], ident=11) == baseline
            assert call(proc, "list_documents", {}, ident=12)["result"] == before["result"]
            call(proc, "switch_page", {**target, "page": target["page_id"]}, ident=13)
            saved = call(proc, "save_file", target, ident=14)
            print(json.dumps({"textId": ids[0], "inactiveExport": True, "nonCurrentPageExport": True}))
    finally:
        proc.kill()
        try:
            proc.wait(timeout=10)
        except Exception:
            pass
  '';
in {
  open-pencil-dependency-links =
    pkgs.runCommand "open-pencil-dependency-links" {
      nativeBuildInputs = [pkgs.bun];
    } ''
      export TMPDIR="$PWD/temporary"
      mkdir -p "$TMPDIR"
      cp ${../../packages/open-pencil-desktop/normalize-links.ts} normalize-links.ts
      cp ${../../packages/open-pencil-desktop/normalize-links.test.ts} normalize-links.test.ts
      bun test normalize-links.test.ts
      touch $out
    '';
  # Static gate: linkage, desktop entry, icons, CLI round-trip.
  open-pencil-package-smoke =
    pkgs.runCommand "open-pencil-package-smoke" {
      nativeBuildInputs = [openPencilCli openPencilDesktop];
    } ''
      bin=${openPencilDesktop}/bin/.OpenPencil-wrapped
      test -x "$bin"
      # Closure linkage: the trace itself must be non-empty (a failed
      # trace is not "zero missing libraries"), then zero unresolved.
      trace=$(LD_TRACE_LOADED_OBJECTS=1 "$bin" 2>/dev/null)
      test -n "$trace"
      missing=$(printf '%s' "$trace" | grep -c "not found" || true)
      test "$missing" = "0"
      grep -Fq 'Exec=openpencil-desktop' \
        ${openPencilDesktop}/share/applications/OpenPencil.desktop
      test -s ${openPencilDesktop}/share/icons/hicolor/128x128/apps/OpenPencil.png
      test -L ${openPencilDesktop}/bin/openpencil-desktop
      cat > fixture.pen <<'EOF'
      {"version": "1", "name": "check",
       "children": [{"type": "frame", "id": "chk-root", "name": "Root",
                     "width": 400, "height": 200,
                     "children": [{"type": "text", "id": "chk-text",
                                   "content": "BEFORE", "fontSize": 16}]}]}
      EOF
      openpencil convert fixture.pen -o check.fig -f fig
      test -s check.fig
      openpencil info check.fig --json > info.json
      openpencil eval check.fig -c "return figma.currentPage.children.length" | grep -Fq 1
      touch $out
    '';
  # Runtime gate: display-isolated desktop open, MCP mutation, save,
  # reopen persistence. Fails loudly; never asserts less on failure.
  open-pencil-desktop-mcp =
    pkgs.runCommand "open-pencil-desktop-mcp" {
      nativeBuildInputs = [
        pkgs.xvfb
        pkgs.dbus
        pkgs.mesa
        openPencilCli
        openPencilDesktop
        (pkgs.python3.withPackages (ps: [ps.pillow]))
        pkgs.procps
        pkgs.util-linux
      ];
    } ''
      export HOME="$TMPDIR/home"
      mkdir -p "$HOME"
      chmod 700 "$HOME"
      export XDG_RUNTIME_DIR="$TMPDIR/rt"
      mkdir -p "$XDG_RUNTIME_DIR"
      chmod 700 "$XDG_RUNTIME_DIR"
      app=""
      dbus=""
      export FONTCONFIG_FILE="${pkgs.makeFontsConf {
        fontDirectories = [pkgs.dejavu_fonts];
      }}"
      # Mesa surfaceless EGL via explicit GLVND vendor discovery: without
      # this, libglvnd finds zero vendors and WebKit aborts EGL init.
      export __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d"
      export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri"

      cat > fixture.pen <<'EOF'
      {"version": "1", "name": "check",
       "children": [{"type": "frame", "id": "chk-root", "name": "Root",
                     "width": 400, "height": 200,
                     "children": [{"type": "text", "id": "chk-text",
                                   "content": "BEFORE", "fontSize": 16}]}]}
      EOF
      openpencil convert fixture.pen -o "$TMPDIR/check.fig" -f fig
      test -s "$TMPDIR/check.fig"

      # Plain Xvfb for windowing; GL comes from surfaceless EGL, so no
      # GLX visuals are required from the server.
      display=""
      for candidate in $(seq 99 112); do
        Xvfb ":$candidate" -screen 0 1280x800x24 >/dev/null 2>&1 &
        xvfb=$!
        sleep 1
        if [ -S "/tmp/.X11-unix/X$candidate" ] \
          && kill -0 "$xvfb" 2>/dev/null; then
          display="$candidate"
          break
        fi
        kill "$xvfb" 2>/dev/null || true
      done
      test -n "$display" || { echo "no free X display"; exit 1; }
      desktop_group() {
        ps -o pgid= -p "$app" 2>/dev/null | tr -d '[:space:]'
      }
      await_desktop_group() {
        local pgid=""
        for _ in $(seq 1 20); do
          pgid="$(desktop_group)"
          if [ "$pgid" = "$app" ]; then
            app_group="$pgid"
            return 0
          fi
          kill -0 "$app" 2>/dev/null || break
          sleep 0.1
        done
        echo "desktop did not start in its own process group (pid=$app pgid=$pgid)" >&2
        return 1
      }
      stop_desktop() {
        if [ -n "$app" ]; then
          if [ -n "$app_group" ]; then
            kill -- "-$app_group" 2>/dev/null || true
          else
            kill "$app" 2>/dev/null || true
          fi
          wait "$app" 2>/dev/null || true
          app=""
          app_group=""
        fi
      }
      cleanup() {
        kill "$xvfb" 2>/dev/null || true
        stop_desktop
        kill "$dbus" 2>/dev/null || true
      }
      trap cleanup EXIT
      app=""
      app_group=""
      export DISPLAY=":$display"
      export GDK_BACKEND=x11
      export LIBGL_ALWAYS_SOFTWARE=1
      export GSK_RENDERER=cairo
      export EGL_PLATFORM=surfaceless
      export WEBKIT_DISABLE_COMPOSITING_MODE=1
      export WEBKIT_DISABLE_DMABUF_RENDERER=1
      cat > "$TMPDIR/session.conf" <<EOF
      <!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
        "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
      <busconfig>
        <type>session</type>
        <listen>unix:path=$TMPDIR/dbus.sock</listen>
        <policy context="default">
          <allow send_destination="*" eavesdrop="true"/>
          <allow eavesdrop="true"/>
          <allow own="*"/>
        </policy>
      </busconfig>
      EOF
      # Foreground daemon under our PID tracking (no --fork: $! would
      # otherwise capture nothing useful).
      dbus-daemon --config-file="$TMPDIR/session.conf" \
        --print-address=1 >"$TMPDIR/dbus.addr" 2>"$TMPDIR/dbus.log" &
      dbus=$!
      export DBUS_SESSION_BUS_ADDRESS="unix:path=$TMPDIR/dbus.sock"
      # Drop any stale MCP state: a leftover socket would satisfy the wait
      # below instantly while pointing at a dead instance.
      rm -rf "$XDG_RUNTIME_DIR/openpencil" "$HOME/.openpencil"
      setsid openpencil-desktop "$TMPDIR/check.fig" >"$TMPDIR/app.log" 2>&1 &
      app=$!
      await_desktop_group

      # The MCP stack prefers $XDG_RUNTIME_DIR/openpencil/ and
      # falls back to ~/.openpencil/; pin the bridge to whichever appears.
      sock=""
      for _ in $(seq 1 120); do
        for candidate in "$XDG_RUNTIME_DIR/openpencil/mcp.sock" \
          "$HOME/.openpencil/mcp.sock"; do
          if [ -S "$candidate" ]; then
            sock="$candidate"
            break 2
          fi
        done
        kill -0 "$app" 2>/dev/null || {
          echo "desktop exited early:"; tail -20 "$TMPDIR/app.log"; exit 1; }
        sleep 1
      done
      test -n "$sock" || {
        echo "no MCP socket appeared:"; tail -20 "$TMPDIR/app.log"; exit 1; }
      export OPENPENCIL_MCP_SOCKET="$sock"
      echo "first MCP socket: $sock"

      result=$(timeout 300 python3 ${driver} "$TMPDIR/check.fig" mutate 120)
      echo "mcp proof: $result"
      stop_desktop
      echo "desktop processes after first exit:" >&2
      ps -eo pid,ppid,pgid,stat,comm | grep -Ei 'openpencil|webkit' >&2 || true

      openpencil eval "$TMPDIR/check.fig" -c "
        const found = [];
        function walk(n) {
          if (n.type === 'TEXT' && n.characters === 'MCP-EDIT-PROOF') found.push(n.id);
          (n.children || []).forEach(walk);
        }
        figma.currentPage.children.forEach(walk);
        if (found.length !== 1) throw new Error('mutation not persisted: ' + JSON.stringify(found));
        return found;
      " >/dev/null

      # Restart/reconnect on the saved document and require the marker
      # through a fresh desktop and MCP bridge, not just the headless parser.
      # Tauri's Linux single-instance plugin registers on the session bus.
      # Start a fresh bus too: the first app's WebKit/MCP children can retain
      # the old bus name after its main process exits and absorb the relaunch.
      kill "$dbus" 2>/dev/null || true
      wait "$dbus" 2>/dev/null || true
      sed "s|$TMPDIR/dbus.sock|$TMPDIR/dbus2.sock|" "$TMPDIR/session.conf" >"$TMPDIR/session2.conf"
      dbus-daemon --config-file="$TMPDIR/session2.conf" \
        --print-address=1 >"$TMPDIR/dbus2.addr" 2>"$TMPDIR/dbus2.log" &
      dbus=$!
      export DBUS_SESSION_BUS_ADDRESS="unix:path=$TMPDIR/dbus2.sock"
      # Each instance also gets a fresh runtime socket directory and HOME.
      # Reusing the first app's MCP socket could connect to an orphaned server.
      export HOME="$TMPDIR/home2"
      mkdir -p "$HOME"
      chmod 700 "$HOME"
      export XDG_RUNTIME_DIR="$TMPDIR/rt2"
      mkdir -p "$XDG_RUNTIME_DIR"
      chmod 700 "$XDG_RUNTIME_DIR"
      unset OPENPENCIL_MCP_SOCKET
      setsid openpencil-desktop "$TMPDIR/check.fig" >"$TMPDIR/app2.log" 2>&1 &
      app=$!
      await_desktop_group
      sock=""
      for _ in $(seq 1 120); do
        for candidate in "$XDG_RUNTIME_DIR/openpencil/mcp.sock" \
          "$HOME/.openpencil/mcp.sock"; do
          if [ -S "$candidate" ]; then
            sock="$candidate"
            break 2
          fi
        done
        kill -0 "$app" 2>/dev/null || {
          echo "desktop exited early on restart:"; tail -20 "$TMPDIR/app2.log"; exit 1; }
        sleep 1
      done
      test -n "$sock" || {
        echo "no MCP socket after restart:" >&2
        tail -20 "$TMPDIR/app2.log" >&2
        ps -eo pid,ppid,pgid,stat,comm | grep -Ei 'openpencil|webkit' >&2 || true
        find "$TMPDIR" -type s -print >&2
        exit 1
      }
      export OPENPENCIL_MCP_SOCKET="$sock"
      if ! reopened=$(timeout 300 python3 ${driver} "$TMPDIR/check.fig" assert 120); then
        echo "desktop log after failed MCP reconnect:" >&2
        tail -40 "$TMPDIR/app2.log" >&2
        exit 1
      fi
      echo "reopen proof: $reopened"
      stop_desktop
      touch $out
    '';
}
