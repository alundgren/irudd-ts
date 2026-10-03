#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static int matches = 0;
static int swapped = 0;

int open64(const char *path, int flags, ...) {
    static int (*original)(const char *, int, ...);
    if (!original) original = dlsym(RTLD_NEXT, "open64");
    const char *target = getenv("ARCHGUARD_QUALITY_FIFO_PATH");
    const char *hit = getenv("ARCHGUARD_QUALITY_FIFO_HIT");
    if (target && hit && !swapped && !(flags & O_CREAT) && !strcmp(path, target)) {
        matches++;
        if (matches == atoi(hit)) {
            swapped = 1;
            if (unlink(path) || mkfifo(path, 0600)) _exit(93);
        }
    }
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list arguments;
        va_start(arguments, flags);
        mode = va_arg(arguments, mode_t);
        va_end(arguments);
    }
    return original(path, flags, mode);
}
