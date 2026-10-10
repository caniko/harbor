"""Root-coordinated copies isolate accepted bytes from worker-owned inodes."""
import os
import pwd
import shutil
import stat
from pathlib import Path


def require_namespace(root):
    if os.geteuid() != 0:
        raise ValueError("protected backup publication requires a root coordinator")
    for index, path in enumerate([root, *root.parents]):
        info = path.lstat()
        sticky = index != 0 and info.st_mode & stat.S_ISVTX
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or (info.st_mode & 0o022 and not sticky):
            raise ValueError("protected backup namespace has untrusted ownership or permissions")
    info = (root / "lock").lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or info.st_mode & 0o022:
        raise ValueError("protected backup lease is not root-owned")


def worker_directory(path, account):
    path.mkdir(mode=0o700)
    os.chown(path, account.pw_uid, account.pw_gid)


class Worker:
    def __init__(self, user, credentials):
        self.account = pwd.getpwnam(user)
        if self.account.pw_uid == 0:
            raise ValueError("backup adapters require an unprivileged worker account")
        self.credentials = None
        if source := os.environ.get("CREDENTIALS_DIRECTORY"):
            credentials.mkdir(mode=0o700)
            self.credentials = credentials
            try:
                for path in Path(source).iterdir():
                    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                    with os.fdopen(descriptor, "rb") as input_file:
                        if not stat.S_ISREG(os.fstat(input_file.fileno()).st_mode):
                            raise ValueError("credential source is not regular")
                        with (credentials / path.name).open("xb") as output:
                            shutil.copyfileobj(input_file, output)
                            os.fchmod(output.fileno(), 0o400)
                            os.fchown(output.fileno(), self.account.pw_uid, self.account.pw_gid)
                os.chown(credentials, self.account.pw_uid, self.account.pw_gid)
            except BaseException:
                self.close()
                raise

    def close(self):
        if self.credentials is not None:
            shutil.rmtree(self.credentials)
            self.credentials = None

    def identity(self):
        return {"user": self.account.pw_uid, "group": self.account.pw_gid,
                "extra_groups": os.getgrouplist(self.account.pw_name, self.account.pw_gid)}


def copy_directory(descriptor, destination):
    for name in os.listdir(descriptor):
        source = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, dir_fd=descriptor)
        try:
            info = os.fstat(source)
            target = destination / name
            if stat.S_ISDIR(info.st_mode):
                target.mkdir(mode=0o700)
                copy_directory(source, target)
            elif stat.S_ISREG(info.st_mode) and info.st_nlink == 1:
                with os.fdopen(os.dup(source), "rb") as input_file, target.open("xb") as output:
                    copied = 0
                    while chunk := input_file.read(128 * 1024):
                        copied += len(chunk)
                        if copied > 1 << 30:
                            raise ValueError("backup artifact exceeds the complete-file byte limit")
                        output.write(chunk)
            else:
                raise ValueError("backup contains redirected, shared or special files")
        finally:
            os.close(source)


def copy_tree(source, destination):
    descriptor = os.open(source, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        destination.mkdir(mode=0o700)
        copy_directory(descriptor, destination)
    finally:
        os.close(descriptor)


def reader_permissions(path, gid):
    for item in path.iterdir():
        if item.is_dir():
            reader_permissions(item, gid)
        else:
            os.chown(item, 0, gid)
            item.chmod(0o440)
    os.chown(path, 0, gid)
    path.chmod(0o750)
