{
  pkgs,
  module,
}: let
  pythonPeer = import ./test-python-package.nix {inherit pkgs;};
  adapter = pkgs.writeText "backup-adapter.py" ''
    import hashlib, json, os, pathlib, sys, socket, array
    stage, backup, workspace = sys.argv[1:]
    assert os.geteuid() != 0, "capture adapters must never execute as root"
    assert os.environ["USER"] == os.environ["LOGNAME"] == "demo"
    assert os.environ["HOME"] == workspace
    backup, workspace = pathlib.Path(backup), pathlib.Path(workspace)
    assert (pathlib.Path(os.environ["CREDENTIALS_DIRECTORY"]) / "token").read_text().strip() == "fixture-token"
    if stage == "capture":
        backup.mkdir()
        (backup / "records").write_text("retained revision seven")
        connection = socket.socket(socket.AF_UNIX)
        connection.connect("/run/backup-fd/socket")
        with open(backup / "records", "r+b") as mutable:
            connection.sendmsg([b"fd"], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [mutable.fileno()]))])
        connection.close()
        digest = hashlib.sha256((backup / "records").read_bytes()).hexdigest()
        (backup / "capture.json").write_text(json.dumps({"version": 1, "consistency": "quiesced", "semantic_sha256": digest}))
    elif stage == "restore":
        (workspace / "records").write_bytes((backup / "records").read_bytes())
    elif stage == "verify":
        assert (workspace / "records").read_bytes() == (backup / "records").read_bytes()
        print(json.dumps({"version": 1, "status": "verified", "semantic_sha256": hashlib.sha256((workspace / "records").read_bytes()).hexdigest()}))
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
      while not (root / "attack").exists(): time.sleep(0.05)
      os.lseek(fd, 0, os.SEEK_SET); os.write(fd, b"modified through pre-publication fd")
      os.close(fd); connection.close()
      (root / "written").touch()
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
      system.extraDependencies = [pythonPeer];
      systemd.services.backup-fd-holder = {
        serviceConfig = {
          User = "demo";
          Group = "demo";
          RuntimeDirectory = "backup-fd";
          ExecStart = "${pkgs.python3}/bin/python3 ${holder}";
        };
      };
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
      for capture in ["systemctl start harbor-db-demo-backup.service", "CREDENTIALS_DIRECTORY=/run/python-backup-credentials ${pythonPeer}/bin/harbor-db-application-backup --config $(readlink -f /etc/harbor-db/demo-backup.json) capture --attempt python-peer"]:
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
      machine.succeed("test $(stat -c %a /var/lib/demo-backup/lock) = 640")
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
