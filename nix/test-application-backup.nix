{
  pkgs,
  module,
}: let
  pythonPeer = import ./test-python-package.nix {inherit pkgs;};
  identityPreload = import ./test-identity-preload.nix {inherit pkgs;};
  adapter = pkgs.writeText "backup-adapter.py" ''
    import hashlib, json, os, pathlib, sys, socket, array, time
    stage, backup, workspace = sys.argv[1:]
    assert os.geteuid() != 0, "capture adapters must never execute as root"
    assert os.environ["USER"] == os.environ["LOGNAME"] == "demo"
    assert os.environ["HOME"] == workspace
    backup, workspace = pathlib.Path(backup), pathlib.Path(workspace)
    assert (pathlib.Path(os.environ["CREDENTIALS_DIRECTORY"]) / "token").read_text().strip() == "fixture-token"
    if stage == "capture":
        backup.mkdir()
        (backup / "records").write_text("retained revision seven")
        (workspace / "capture-origin").write_text(str(backup))
        if pathlib.Path("/run/backup-fd/fail-copy").exists():
            # Real ENOSPC during the protected copy leaves a partial seal.
            for name in ("blob-one", "blob-two"):
                with open(backup / name, "wb") as stream:
                    for _ in range(40): stream.write(b"x" * (1024 * 1024))
            digest = hashlib.sha256((backup / "records").read_bytes()).hexdigest()
            (backup / "capture.json").write_text(json.dumps({"version": 1, "consistency": "quiesced", "semantic_sha256": digest}))
            sys.exit(0)
        connection = socket.socket(socket.AF_UNIX)
        connection.connect("/run/backup-fd/socket")
        with open(backup / "records", "r+b") as mutable:
            connection.sendmsg([b"fd"], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [mutable.fileno()]))])
        connection.close()
        digest = hashlib.sha256((backup / "records").read_bytes()).hexdigest()
        (backup / "capture.json").write_text(json.dumps({"version": 1, "consistency": "quiesced", "semantic_sha256": digest}))
    elif stage == "restore":
        # The competing account keeps an fd from capture and changes its inode
        # only while restore is reading. Certification must use protected inodes.
        assert backup.name.endswith(".sealed"), "certifier used mutable staging"
        assert backup.stat().st_uid == 0 and (backup.stat().st_mode & 0o022) == 0
        for entry in backup.rglob("*"):
            assert entry.stat().st_uid == 0 and (entry.stat().st_mode & 0o022) == 0
        root = pathlib.Path("/run/backup-fd")
        (root / "certify-attack").touch()
        deadline = time.monotonic() + 15
        while not (root / "certify-written").exists():
            assert time.monotonic() < deadline, "concurrent mutation did not run"
            time.sleep(0.05)
        (workspace / "records").write_bytes((backup / "records").read_bytes())
        (workspace / "restore-origin").write_text(str(backup))
        assert (workspace / "records").read_text() == "retained revision seven"
        (root / "certify-undo").touch()
        while not (root / "certify-restored").exists():
            assert time.monotonic() < deadline, "staging inode was not restored"
            time.sleep(0.05)
        for name in ("certify-attack", "certify-written", "certify-undo", "certify-restored"):
            (root / name).unlink()
    elif stage == "verify":
        assert (workspace / "records").read_bytes() == (backup / "records").read_bytes()
        print(json.dumps({"version": 1, "status": "verified", "semantic_sha256": hashlib.sha256((workspace / "records").read_bytes()).hexdigest()}))
    elif stage == "cleanup":
        origin = workspace / "restore-origin"
        if not origin.exists(): origin = workspace / "capture-origin"
        assert origin.read_text() == str(backup), "cleanup retry changed its durable backup argument"
        if pathlib.Path("/run/backup-fd/fail-cleanup").exists():
            raise RuntimeError("interrupted protected cleanup")
  '';
  holder = pkgs.writeText "backup-fd-holder.py" ''
    import array, json, os, pathlib, socket, time
    root = pathlib.Path("/run/backup-fd")
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(root / "socket")); listener.listen()
    while True:
      connection, _ = listener.accept()
      _, ancillary, _, _ = connection.recvmsg(16, socket.CMSG_SPACE(array.array("i").itemsize))
      received = array.array("i"); received.frombytes(ancillary[0][2][:received.itemsize])
      fd = received[0]; info = os.fstat(fd)
      (root / "inode").write_text(json.dumps([info.st_dev, info.st_ino]))
      while not (root / "certify-attack").exists(): time.sleep(0.05)
      os.lseek(fd, 0, os.SEEK_SET); original = os.read(fd, info.st_size)
      attack = b"uncertified hostile records"
      os.lseek(fd, 0, os.SEEK_SET); os.write(fd, attack); os.ftruncate(fd, len(attack))
      (root / "certify-written").touch()
      while not (root / "certify-undo").exists(): time.sleep(0.05)
      os.lseek(fd, 0, os.SEEK_SET); os.write(fd, original); os.ftruncate(fd, len(original))
      (root / "certify-restored").touch()
      while not (root / "attack").exists(): time.sleep(0.05)
      os.lseek(fd, 0, os.SEEK_SET); os.write(fd, b"modified through pre-publication fd")
      os.close(fd); connection.close()
      (root / "written").touch()
  '';
  admission = pkgs.writeText "protected-backup-command-admission.py" ''
    import hashlib, json, pathlib, subprocess, sys
    source, native, peer = sys.argv[1:]
    config = json.loads(pathlib.Path(source).read_text())
    root = pathlib.Path(config["root"])
    def retained():
      result = {}
      for path in [root, *sorted(root.rglob("*"))]:
        info = path.lstat()
        result[str(path)] = [info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode,
                            hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None]
      return result
    before = retained()
    adapter = pathlib.Path("/var/lib/worker-adapter/executable")
    alias = adapter.with_name("store-alias")
    for engine, binary in enumerate((native, peer)):
      for stage in ("capture", "restore", "verify", "cleanup"):
        for kind, executable in enumerate((adapter, alias)):
          selected = json.loads(json.dumps(config))
          selected["commands"][stage][0] = str(executable)
          policy = pathlib.Path("/run/rejected-backup-policy.json")
          policy.write_text(json.dumps(selected))
          result = subprocess.run([binary, "--config", str(policy), "capture", "--attempt", f"rejected-{engine}-{stage}-{kind}"], capture_output=True, text=True)
          assert result.returncode != 0 and "immutable store" in result.stderr, (engine, stage, kind, result.stderr)
          assert retained() == before, "rejected admission mutated protected backup state"
      for kind, credential in enumerate((
          "/var/lib/worker-adapter/secret", "/var/lib/worker-adapter/root-secret", "/var/lib/worker-adapter/secret-link",
          "/run/credential-trust/worker-link", "/run/credential-writable/secret", "/run/credential-trust/group-secret",
          "/run/credential-trust/oversize", "/run/credential-trust/fifo", "/run/credential-trust/loop",
      )):
        selected = json.loads(json.dumps(config))
        selected["credentials"] = {"token": credential}
        policy = pathlib.Path("/run/rejected-backup-policy.json")
        policy.write_text(json.dumps(selected))
        result = subprocess.run([binary, "--config", str(policy), "capture", "--attempt", f"credential-rejected-{engine}-{kind}"], capture_output=True, text=True, timeout=30)
        assert result.returncode != 0 and "credential" in result.stderr, (engine, credential, result.stderr)
        assert retained() == before, "rejected credential source mutated protected backup state"
    assert not adapter.with_name("executed").exists(), "mutable adapter executed"
  '';
in
  pkgs.testers.runNixOSTest {
    extraDriverArgs = ["--junit-xml" "junit.xml"];
    name = "harbor-db-application-backup";
    nodes.machine = {lib, ...}: {
      imports = [module];
      virtualisation.emptyDiskImages = [256];
      virtualisation.fileSystems."/var/lib/demo-backup" = {
        device = "/dev/vdb";
        fsType = "ext4";
        autoFormat = true;
        options = ["nofail"];
      };
      users.users = {
        demo = {
          isSystemUser = true;
          group = "demo";
        };
        reader = {
          isSystemUser = true;
          group = "reader";
        };
      };
      users.groups = {
        demo = {};
        reader = {};
      };
      environment.etc."backup-fixture-token" = {
        text = "fixture-token";
        mode = "0600";
      };
      environment.systemPackages = [pkgs.python3];
      system.extraDependencies = [pythonPeer identityPreload];
      systemd.services.backup-fd-holder = {
        serviceConfig = {
          User = "demo";
          Group = "demo";
          RuntimeDirectory = "backup-fd";
          ExecStart = "${pkgs.python3}/bin/python3 ${holder}";
        };
      };
      # The restore fixture synchronizes its concurrent inode mutation through
      # these markers. ProtectSystem=strict otherwise makes /run read-only in
      # the scheduled backup service, even though the holder can write there.
      systemd.services.harbor-db-demo-backup.serviceConfig.ReadWritePaths = ["/run/backup-fd"];
      services.harbor-db.projects.demo.backup = {
        enable = true;
        user = "demo";
        group = "demo-backup";
        directory = "/var/lib/demo-backup";
        readers = ["reader"];
        commands = lib.genAttrs ["capture" "restore" "verify" "cleanup"] (stage: ["${pkgs.python3}/bin/python3" (toString adapter) stage "{backup}" "{workspace}"]);
        executableFiles = [(toString adapter)];
        credentials.token = "/etc/backup-fixture-token";
      };
      systemd.timers.harbor-db-demo-backup.wantedBy = lib.mkForce [];
      system.stateVersion = "26.05";
    };
    testScript = ''
      import shlex
      # reboot() reconnects only when QEMU was started with reboot support.
      machine.start(allow_reboot=True)
      machine.wait_for_unit("multi-user.target")
      machine.wait_until_succeeds("mountpoint -q /var/lib/demo-backup")
      machine.succeed("systemd-tmpfiles --create")
      machine.succeed("systemctl start backup-fd-holder.service")
      machine.wait_until_succeeds("test -S /run/backup-fd/socket")
      machine.succeed("install -d -m 0700 /run/python-backup-credentials; install -m 0400 /etc/backup-fixture-token /run/python-backup-credentials/token")
      points = []
      machine.succeed("install -d -m 0700 /run/credential-trust/secret.d; install -m 0400 /etc/backup-fixture-token /run/credential-trust/secret.d/token; ln -s secret.d /run/credential-trust/directory-alias; ln -s directory-alias/token /run/credential-trust/token-source; ln -s token-source /run/credential-trust/token-alias; ln -s ../token-alias /run/credential-trust/secret.d/parent-alias")
      machine.succeed("python3 -c 'import json; p=json.load(open(\"/etc/harbor-db/demo-backup.json\")); p[\"credentials\"]={\"token\":\"/run/credential-trust/secret.d/parent-alias\"}; json.dump(p,open(\"/run/forced-backup-policy.json\",\"w\"))'")
      for capture in ["systemctl start harbor-db-demo-backup.service", "CREDENTIALS_DIRECTORY=/run/python-backup-credentials ${pythonPeer}/bin/harbor-db-application-backup --config $(readlink -f /etc/harbor-db/demo-backup.json) capture --attempt python-peer", "harbor-db-application-backup --config /run/forced-backup-policy.json capture --attempt native-forced", "${pythonPeer}/bin/harbor-db-application-backup --config /run/forced-backup-policy.json capture --attempt python-forced"]:
          machine.succeed(capture)
          point = machine.succeed("python3 -c 'import json; print(json.load(open(\"/var/lib/demo-backup/LAST_SUCCESS\"))[\"attempt\"])'").strip()
          points.append(point)
          machine.succeed(f"runuser -u reader -- cat /var/lib/demo-backup/{point}/records")
          machine.succeed(f"test $(stat -c %a /var/lib/demo-backup/{point}/records) = 440")
          machine.fail(f"runuser -u demo -- sh -c 'echo changed >> /var/lib/demo-backup/{point}/records'")
          machine.fail(f"runuser -u demo -- chmod u+w /var/lib/demo-backup/{point}/records")
          machine.fail(f"runuser -u demo -- mv /var/lib/demo-backup/{point}/records /var/lib/demo-backup/{point}/records.replaced")
          machine.fail(f"runuser -u demo -- mv /var/lib/demo-backup/{point} /var/lib/demo-backup/replaced-point")
          machine.fail(f"runuser -u demo -- chmod u+w /var/lib/demo-backup/{point}")
          machine.succeed("touch /run/backup-fd/attack")
          machine.wait_until_succeeds("test -f /run/backup-fd/written")
          machine.succeed(f"test \"$(cat /var/lib/demo-backup/{point}/records)\" = 'retained revision seven'")
          machine.succeed(f"python3 -c 'import json,os; i=os.stat(\"/var/lib/demo-backup/{point}/records\"); assert [i.st_dev,i.st_ino]!=json.load(open(\"/run/backup-fd/inode\"))'")
          machine.succeed("rm /run/backup-fd/attack /run/backup-fd/written")
      for engine, command in (("native", "harbor-db-application-backup"), ("python", "${pythonPeer}/bin/harbor-db-application-backup")):
          point = engine + "-cleanup-retry"
          capture = command + " --config /run/forced-backup-policy.json capture --attempt " + point
          machine.succeed("cp /var/lib/demo-backup/LAST_SUCCESS /run/cleanup-last-success; touch /run/backup-fd/fail-cleanup")
          machine.fail(capture)
          machine.succeed("cmp /run/cleanup-last-success /var/lib/demo-backup/LAST_SUCCESS; test -d /var/lib/demo-backup/" + point + ".sealed; test -f /var/lib/demo-backup/" + point + ".restore-workspace/restore-origin; touch /run/backup-fd/attack")
          machine.wait_until_succeeds("test -f /run/backup-fd/written")
          machine.succeed("rm /run/backup-fd/fail-cleanup /run/backup-fd/attack /run/backup-fd/written")
          machine.succeed(capture + " --retry-incomplete")
          machine.succeed("touch /run/backup-fd/attack")
          machine.wait_until_succeeds("test -f /run/backup-fd/written")
          machine.succeed("rm /run/backup-fd/attack /run/backup-fd/written")
          machine.succeed(command + " --config /run/forced-backup-policy.json inspect /var/lib/demo-backup/" + point)
          machine.succeed("test ! -d /var/lib/demo-backup/" + point + ".restore-workspace; test ! -d /var/lib/demo-backup/" + point + ".sealed; test $(find /var/lib/demo-backup -maxdepth 1 -name '" + point + ".abandoned-*' | wc -l) = 1")
          points.append(point)
      # Namespace-root is needed to perform the actual mount/hostname attack.
      # The certifier must reject that namespace before account/restore admission.
      machine.succeed("install -d -m 0700 -o reader -g reader /var/lib/backup-certifier; install -m 0600 -o reader -g reader /dev/null /var/lib/backup-certifier/lock; printf 'caller-chosen-machine-id\\n' > /run/backup-machine-id")
      for engine, command in (("native", "harbor-db-application-backup"), ("python", "${pythonPeer}/bin/harbor-db-application-backup")):
          certify = command + " --config /run/forced-backup-policy.json certify /var/lib/demo-backup/" + points[2] + " --state /var/lib/backup-certifier"
          rejected = machine.fail("runuser -u reader -- unshare --user --map-root-user --mount --uts -- sh -ec " + shlex.quote("mount --bind /run/backup-machine-id /etc/machine-id; hostname namespace-remote; test \"$(cat /etc/machine-id)\" = caller-chosen-machine-id; test \"$(hostname)\" = namespace-remote; " + certify) + " 2>&1")
          assert "caller-controlled namespaces" in rejected, (engine, rejected)
      # A caller-controlled loader cannot forge native machine custody. Prove
      # that the same interposer really substitutes Python's hostname/read.
      machine.succeed("runuser -u reader -- env LD_PRELOAD=${identityPreload}/lib/identity.so HARBOR_FAKE_MACHINE_ID=/run/backup-machine-id python3 -c 'import socket; assert socket.gethostname()==\"loader-chosen-remote\"; assert open(\"/etc/machine-id\").read().strip()==\"caller-chosen-machine-id\"'")
      rejected = machine.fail("runuser -u reader -- env LD_PRELOAD=${identityPreload}/lib/identity.so HARBOR_FAKE_MACHINE_ID=/run/backup-machine-id harbor-db-application-backup --config /run/forced-backup-policy.json certify /var/lib/demo-backup/" + points[2] + " --state /var/lib/backup-certifier 2>&1")
      assert "different machine" in rejected, rejected
      machine.succeed("test $(find /var/lib/backup-certifier -mindepth 1 -maxdepth 1 | wc -l) = 1")
      for engine, command in (("native", "harbor-db-application-backup"), ("python", "${pythonPeer}/bin/harbor-db-application-backup")):
          root = "/srv/" + engine + "-incomplete-copy"
          machine.succeed("install -d -m 2750 -g demo-backup " + root + "; mount -t tmpfs -o size=96M,mode=2750,gid=$(getent group demo-backup | cut -d: -f3) tmpfs " + root + "; install -m 0640 -g demo-backup /dev/null " + root + "/lock")
          policy = "/run/" + engine + "-incomplete-copy.json"
          machine.succeed("python3 -c 'import json; p=json.load(open(\"/run/forced-backup-policy.json\")); p[\"root\"]=\"" + root + "\"; json.dump(p,open(\"" + policy + "\",\"w\"))'")
          capture = command + " --config " + policy + " capture --attempt interrupted"
          machine.succeed("touch /run/backup-fd/fail-copy /run/backup-fd/fail-cleanup")
          machine.fail(capture)
          machine.succeed("test -d " + root + "/interrupted.sealed; test -f " + root + "/interrupted.restore-workspace/capture-origin; test ! -e " + root + "/LAST_SUCCESS")
          machine.succeed("python3 -c 'import json; assert json.load(open(\"" + root + "/interrupted.intent.json\"))[\"cleanup_origin\"]==\"partial\"'")
          machine.succeed("rm /run/backup-fd/fail-copy /run/backup-fd/fail-cleanup; mount -o remount,size=256M " + root)
          machine.succeed(capture + " --retry-incomplete")
          machine.succeed(command + " --config " + policy + " inspect " + root + "/interrupted")
          machine.succeed("touch /run/backup-fd/attack")
          machine.wait_until_succeeds("test -f /run/backup-fd/written")
          machine.succeed("rm /run/backup-fd/attack /run/backup-fd/written")
          machine.succeed("test $(find " + root + " -maxdepth 1 -name 'interrupted.abandoned-*' | wc -l) = 1; umount " + root)
      machine.succeed("test $(stat -c %a /var/lib/demo-backup/lock) = 640")
      machine.succeed("install -d -m 0750 -o demo -g demo /var/lib/worker-adapter; printf '#!/bin/sh\\ntouch /var/lib/worker-adapter/executed\\n' > /var/lib/worker-adapter/executable; chown demo:demo /var/lib/worker-adapter/executable; chmod 0750 /var/lib/worker-adapter/executable; ln -s ${pkgs.python3}/bin/python3 /var/lib/worker-adapter/store-alias")
      machine.succeed("install -m 0400 -o demo -g demo /etc/backup-fixture-token /var/lib/worker-adapter/secret; install -m 0600 /etc/backup-fixture-token /var/lib/worker-adapter/root-secret; ln -s /etc/backup-fixture-token /var/lib/worker-adapter/secret-link; chown -h demo:demo /var/lib/worker-adapter/secret-link")
      machine.succeed("ln -s /etc/backup-fixture-token /run/credential-trust/worker-link; chown -h demo:demo /run/credential-trust/worker-link; install -d -m 0770 -g demo /run/credential-writable; install -m 0400 /etc/backup-fixture-token /run/credential-writable/secret; install -m 0660 -g demo /etc/backup-fixture-token /run/credential-trust/group-secret; truncate -s 1048577 /run/credential-trust/oversize; chmod 0600 /run/credential-trust/oversize; mkfifo -m 0600 /run/credential-trust/fifo; ln -s loop /run/credential-trust/loop")
      machine.succeed("python3 ${admission} $(readlink -f /etc/harbor-db/demo-backup.json) $(command -v harbor-db-application-backup) ${pythonPeer}/bin/harbor-db-application-backup")
      machine.fail("runuser -u reader -- cat /etc/backup-fixture-token")
      machine.succeed("systemctl start harbor-db-demo-backup.service")
      machine.reboot()
      machine.wait_for_unit("multi-user.target")
      machine.wait_until_succeeds("mountpoint -q /var/lib/demo-backup")
      for point in points:
          machine.succeed(f"runuser -u reader -- cat /var/lib/demo-backup/{point}/records")
      machine.succeed("systemctl start backup-fd-holder.service")
      machine.wait_until_succeeds("test -S /run/backup-fd/socket")
      mount = machine.succeed("systemd-escape --path --suffix=mount /var/lib/demo-backup").strip()
      machine.succeed(f"systemctl stop {shlex.quote(mount)}; systemctl mask --runtime {shlex.quote(mount)}")
      machine.succeed("install -d -m 2750 -o root -g demo-backup /var/lib/demo-backup; install -m 0640 -o root -g demo-backup /dev/null /var/lib/demo-backup/lock")
      machine.fail("systemctl start harbor-db-demo-backup.service")
      machine.succeed("test ! -e /var/lib/demo-backup/LAST_SUCCESS; test $(find /var/lib/demo-backup -mindepth 1 -maxdepth 1 | wc -l) = 1")
    '';
  }
