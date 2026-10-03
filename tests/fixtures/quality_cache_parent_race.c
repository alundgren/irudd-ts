#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int swapped;
static void redirect_cache_parent(const char *path, int flags) {
    const char *parent = getenv("ARCHGUARD_REVIEW_CACHE_PARENT");
    const char *source = getenv("ARCHGUARD_REVIEW_CACHE_SOURCE");
    if (!parent || !source || swapped || !(flags & O_CREAT)) return;
    const char *leaf = strrchr(path, '/');
    leaf = leaf ? leaf + 1 : path;
    if (strncmp(leaf, ".archguard-dryer-", strlen(".archguard-dryer-"))) return;
    char saved[4096];
    if (snprintf(saved, sizeof(saved), "%s.saved", parent) >= sizeof(saved)) _exit(91);
    swapped = 1;
    if (rename(parent, saved) || symlink(source, parent)) _exit(92);
}
static mode_t mode_argument(int flags, va_list arguments) {
    return flags & O_CREAT ? va_arg(arguments, mode_t) : 0;
}
int open64(const char *path, int flags, ...) {
    static int (*original)(const char *, int, ...);
    if (!original) original = dlsym(RTLD_NEXT, "open64");
    va_list arguments; va_start(arguments, flags);
    mode_t mode = mode_argument(flags, arguments); va_end(arguments);
    redirect_cache_parent(path, flags);
    return original(path, flags, mode);
}
int openat64(int directory, const char *path, int flags, ...) {
    static int (*original)(int, const char *, int, ...);
    if (!original) original = dlsym(RTLD_NEXT, "openat64");
    va_list arguments; va_start(arguments, flags);
    mode_t mode = mode_argument(flags, arguments); va_end(arguments);
    redirect_cache_parent(path, flags);
    return original(directory, path, flags, mode);
}
int openat(int directory, const char *path, int flags, ...) {
    static int (*original)(int, const char *, int, ...);
    if (!original) original = dlsym(RTLD_NEXT, "openat");
    va_list arguments; va_start(arguments, flags);
    mode_t mode = mode_argument(flags, arguments); va_end(arguments);
    redirect_cache_parent(path, flags);
    return original(directory, path, flags, mode);
}
