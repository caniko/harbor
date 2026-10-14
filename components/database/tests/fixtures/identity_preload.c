#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char *identity(const char *path) {
    const char *fake = getenv("HARBOR_FAKE_MACHINE_ID");
    return fake && !strcmp(path, "/etc/machine-id") ? fake : path;
}
#define OPEN(name) \
int name(const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); } \
    int (*real)(const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    return real(identity(path), flags, mode); \
}
#define OPENAT(name) \
int name(int dir, const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); } \
    int (*real)(int, const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    return real(dir, identity(path), flags, mode); \
}
OPEN(open)
OPEN(open64)
OPENAT(openat)
OPENAT(openat64)
int gethostname(char *buffer, size_t size) {
    const char *host = "loader-chosen-remote";
    if (strlen(host) >= size) return -1;
    strcpy(buffer, host);
    return 0;
}
