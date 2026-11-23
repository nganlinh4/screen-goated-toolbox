#include "namespace.h"

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/file.h>
#include <sys/syscall.h>
#include <unistd.h>

static int valid_leaf(const char *name) {
    return name != NULL && name[0] != '\0' &&
        strnlen(name, 4097) <= 4096 && strchr(name, '/') == NULL &&
        strcmp(name, ".") != 0 && strcmp(name, "..") != 0;
}

static int checked_descriptor(int fd, int kind) {
    if (fd < 0) return -errno;
    struct stat information;
    if (fstat(fd, &information) != 0) {
        int error = errno;
        close(fd);
        return -error;
    }
    if ((kind == 1 && !S_ISDIR(information.st_mode)) ||
        (kind != 1 && !S_ISREG(information.st_mode))) {
        close(fd);
        return -EINVAL;
    }
    return fd;
}

int sgt_open_directory(const char *path) {
    if (path == NULL || path[0] == '\0' || strnlen(path, 4097) > 4096) return -EINVAL;
    return checked_descriptor(open(path, O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC), 1);
}

int sgt_open_entry(int directory, const char *name, int kind) {
    if (!valid_leaf(name) || kind < 0 || kind > 3) return -EINVAL;
    int flags = O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK;
    if (kind == 1) flags |= O_PATH | O_DIRECTORY;
    else if (kind == 2) flags |= O_RDWR | O_CREAT | O_EXCL;
    else if (kind == 3) flags |= O_RDWR;
    else flags |= O_RDONLY;
    return checked_descriptor(openat(directory, name, flags, 0600), kind);
}

int sgt_mkdir_entry(int directory, const char *name) {
    if (!valid_leaf(name)) return EINVAL;
    return mkdirat(directory, name, 0700) == 0 ? 0 : errno;
}

int sgt_sync_directory(int directory) {
    int fd = checked_descriptor(openat(directory, ".", O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC), 1);
    if (fd < 0) return -fd;
    int result = fsync(fd) == 0 ? 0 : errno;
    if (close(fd) != 0 && result == 0) result = errno;
    return result;
}

int sgt_lock_file(int descriptor, int acquire) {
    if (acquire != 0 && acquire != 1) return EINVAL;
    struct stat information;
    if (fstat(descriptor, &information) != 0) return errno;
    if (!S_ISREG(information.st_mode)) return EINVAL;
    return flock(descriptor, acquire ? LOCK_EX | LOCK_NB : LOCK_UN) == 0 ? 0 : errno;
}

int sgt_publish_entry(int directory, const char *source, const char *target, int exchange) {
    if (!valid_leaf(source) || !valid_leaf(target) || (exchange != 0 && exchange != 1)) return EINVAL;
    unsigned int flags = exchange ? RENAME_EXCHANGE : RENAME_NOREPLACE;
    return syscall(SYS_renameat2, directory, source, directory, target, flags) == 0 ? 0 : errno;
}

int sgt_move_entry(int source_directory, const char *source, int target_directory, const char *target) {
    if (!valid_leaf(source) || !valid_leaf(target)) return EINVAL;
    return syscall(SYS_renameat2, source_directory, source, target_directory, target, RENAME_NOREPLACE) == 0 ? 0 : errno;
}

/* This only unlinks within an app-owned staging namespace. */
int sgt_unlink_staged_file(int directory, const char *name, int expected_fd) {
    if (!valid_leaf(name)) return EINVAL;
    struct stat expected;
    struct stat entry;
    if (fstat(expected_fd, &expected) != 0 || fstatat(directory, name, &entry, AT_SYMLINK_NOFOLLOW) != 0) return errno;
    if (!S_ISREG(entry.st_mode) || entry.st_dev != expected.st_dev || entry.st_ino != expected.st_ino) return ESTALE;
    return unlinkat(directory, name, 0) == 0 ? 0 : errno;
}
