#include "namespace.h"
#include "namespace_test_platform.h"
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

struct contract_case {
    const char *name;
    int directory_read, directory_search, directory_write, file_read, file_write;
    int expected_directory_open, expected_file_open, expected_create, expected_directory_sync, expected_writable;
};
#include "namespace_cases.h"
static const struct contract_case structural_case = {"structural", 1, 1, 1, 1, 1, 1, 1, 1, 1, 1};

struct descriptor { int active, flags; unsigned int mode; int locked; };
static struct descriptor descriptors[256];
static const struct contract_case *current;
static int next_descriptor = 4, failures, checks, calls, fail_stat, created;
static int publish_error, publish_calls;
static int expected_target_parent = 3, staged_identity, staged_unlinks;
static unsigned int published_flags;
static int sync_error, close_error, sync_calls, synced_descriptor, opened_parent, opened_dot;
static int lock_error, lock_calls;

static void check(int condition, const char *label) {
    checks++;
    if (!condition) { failures++; fprintf(stderr, "%s: %s\n", current->name, label); }
}

static int allocate(unsigned int mode, int flags) {
    int descriptor = next_descriptor++;
    if (descriptor >= (int)(sizeof(descriptors) / sizeof(descriptors[0]))) return -1;
    descriptors[descriptor] = (struct descriptor){1, flags, mode, 0};
    return descriptor;
}

static int model_open(const char *name, int flags, int parent) {
    calls++;
    opened_parent = parent;
    opened_dot = strcmp(name, ".") == 0;
    if (parent != 3 && (parent < 0 || parent >= next_descriptor || !descriptors[parent].active)) {
        errno = EBADF; return -1;
    }
    if (parent != 3 && !S_ISDIR(descriptors[parent].mode)) { errno = ENOTDIR; return -1; }
    if (parent != 3 && !current->directory_search) { errno = EACCES; return -1; }
    if (opened_dot || strcmp(name, "directory") == 0 || strcmp(name, "/directory") == 0 || strcmp(name, "/") == 0) {
        if (!(flags & O_PATH) && !current->directory_read) { errno = EACCES; return -1; }
        return allocate(S_IFDIR, flags);
    }
    if (strcmp(name, "symbolic") == 0) {
        if (!(flags & O_PATH)) { errno = EACCES; return -1; }
        return allocate(S_IFLNK, flags);
    }
    if (strcmp(name, "pipe") == 0) return allocate(S_IFIFO, flags);
    if (flags & O_CREAT) {
        if (!current->directory_write) { errno = EACCES; return -1; }
        if (created) { errno = EEXIST; return -1; }
        created = 1;
    } else if (!current->file_read || ((flags & O_RDWR) && !current->file_write)) { errno = EACCES; return -1; }
    return allocate(S_IFREG, flags);
}

int open(const char *path, int flags, ...) { return model_open(path, flags, 3); }
int openat(int parent, const char *name, int flags, ...) { return model_open(name, flags, parent); }
int close(int descriptor) {
    descriptors[descriptor].active = 0;
    descriptors[descriptor].locked = 0;
    if (close_error) { errno = close_error; return -1; }
    return 0;
}
int fstat(int descriptor, struct stat *information) {
    if (fail_stat) { errno = EIO; return -1; }
    if (descriptor < 0 || descriptor >= next_descriptor || !descriptors[descriptor].active) { errno = EBADF; return -1; }
    information->st_mode = descriptors[descriptor].mode;
    information->st_dev = 1;
    information->st_ino = descriptor;
    return 0;
}
int fstatat(int directory, const char *name, struct stat *information, int flags) {
    check(directory == 3 && strcmp(name, "slot") == 0 && flags == AT_SYMLINK_NOFOLLOW, "staged lookup is relative and no-follow");
    information->st_mode = S_IFREG;
    information->st_dev = 1;
    information->st_ino = staged_identity;
    return 0;
}
int unlinkat(int directory, const char *name, int flags) {
    check(directory == 3 && strcmp(name, "slot") == 0 && flags == 0, "unlink uses only the staged file");
    staged_unlinks++;
    return 0;
}
int fsync(int descriptor) {
    sync_calls++;
    synced_descriptor = descriptor;
    check(descriptors[descriptor].active && S_ISDIR(descriptors[descriptor].mode), "sync owns a directory descriptor");
    check(descriptors[descriptor].flags == (O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC), "sync has separate read-capable directory flags");
    if (sync_error) { errno = sync_error; return -1; }
    return 0;
}
int flock(int descriptor, int flags) {
    lock_calls++;
    check(descriptors[descriptor].active && S_ISREG(descriptors[descriptor].mode), "lock dispatch owns regular-file descriptor");
    check(flags == (LOCK_EX | LOCK_NB) || flags == LOCK_UN, "exclusive lock is nonblocking; release is explicit");
    if (lock_error) { errno = lock_error; return -1; }
    descriptors[descriptor].locked = flags != LOCK_UN;
    return 0;
}
int mkdirat(int parent, const char *name, unsigned int mode) {
    (void)parent; (void)name;
    check(mode == 0700U, "directory creation mode");
    return 0;
}
long syscall(long number, ...) {
    va_list arguments;
    va_start(arguments, number);
    int parent = va_arg(arguments, int);
    const char *source = va_arg(arguments, const char *);
    int target_parent = va_arg(arguments, int);
    const char *target = va_arg(arguments, const char *);
    published_flags = va_arg(arguments, unsigned int);
    va_end(arguments);
    check(number == SYS_renameat2 && parent == 3 && target_parent == expected_target_parent && source != NULL && target != NULL, "atomic syscall contract");
    publish_calls++;
    if (publish_error) { errno = publish_error; return -1; }
    return 0;
}

static void permissions(void) {
    int absolute = sgt_open_directory("/directory");
    check(absolute >= 0, "path-based directory pin without enumeration");
    if (absolute >= 0) {
        check((descriptors[absolute].flags & (O_PATH | O_CLOEXEC | O_NOFOLLOW | O_DIRECTORY)) ==
              (O_PATH | O_CLOEXEC | O_NOFOLLOW | O_DIRECTORY), "path-based directory descriptor capabilities");
        close(absolute);
    }
    int directory = sgt_open_entry(3, "directory", 1);
    check((directory >= 0) == current->expected_directory_open, "pin directory without enumeration");
    if (directory < 0) return;
    check((descriptors[directory].flags & (O_PATH | O_CLOEXEC | O_NOFOLLOW | O_DIRECTORY)) ==
          (O_PATH | O_CLOEXEC | O_NOFOLLOW | O_DIRECTORY), "directory descriptor capabilities");
    int before_sync = sync_calls;
    check((sgt_sync_directory(directory) == 0) == current->expected_directory_sync,
          "directory durability requires separate read/search capability");
    check(opened_parent == directory && opened_dot, "sync stays relative to retained directory");
    check(sync_calls - before_sync == current->expected_directory_sync, "permission failure cannot become a no-op sync");
    check(descriptors[directory].active, "sync does not close retained location");
    int file = sgt_open_entry(directory, "file", 0);
    check((file >= 0) == current->expected_file_open, "child read permission remains required");
    if (file >= 0) {
        check(!(descriptors[file].flags & O_PATH), "file remains readable");
        check((descriptors[file].flags & (O_CLOEXEC | O_NOFOLLOW)) == (O_CLOEXEC | O_NOFOLLOW), "data descriptor ownership flags");
        check(sgt_lock_file(file, 1) == 0 && descriptors[file].locked, "read-only descriptor can coordinate without write authority");
        check(sgt_lock_file(file, 0) == 0 && !descriptors[file].locked, "read-only descriptor lease released");
        close(file);
    }
    int writable = sgt_open_entry(directory, "file", 3);
    check((writable >= 0) == current->expected_writable, "existing writable open requires file read/write and parent search");
    if (writable >= 0) {
        check(descriptors[writable].flags == (O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK | O_RDWR),
              "writable open does not create or truncate");
        check(sgt_lock_file(writable, 1) == 0 && descriptors[writable].locked, "exclusive descriptor lock acquired");
        check(sgt_lock_file(writable, 0) == 0 && !descriptors[writable].locked, "descriptor lock released");
        close(writable);
    }
    int output = sgt_open_entry(directory, "created", 2);
    check((output >= 0) == current->expected_create, "child write permission remains required");
    if (output >= 0) {
        check((descriptors[output].flags & (O_CREAT | O_EXCL | O_RDWR)) == (O_CREAT | O_EXCL | O_RDWR), "exclusive read-write create");
        close(output);
        check(sgt_open_entry(directory, "created", 2) == -EEXIST, "creation cannot overwrite");
    }
    close(directory);
}

static void structural_guards(void) {
    const char *invalid[] = {NULL, "", ".", "..", "nested/file"};
    for (unsigned int i = 0; i < sizeof(invalid) / sizeof(invalid[0]); i++) {
        int before = calls;
        check(sgt_open_entry(3, invalid[i], 1) == -EINVAL && calls == before, "invalid leaf rejected before dispatch");
    }
    check(sgt_open_entry(3, "symbolic", 1) < 0, "directory symlink rejected");
    check(sgt_open_directory("symbolic") < 0, "path-based directory symlink rejected");
    check(sgt_open_directory("file") < 0, "path-based non-directory rejected");
    int root = sgt_open_directory("/");
    check(root >= 0, "root directory pin");
    if (root >= 0) close(root);
    int before_open = calls;
    check(sgt_open_directory(NULL) == -EINVAL && sgt_open_directory("") == -EINVAL && calls == before_open,
          "invalid directory path rejected before dispatch");
    char oversized[4098];
    memset(oversized, 'x', sizeof(oversized));
    oversized[sizeof(oversized) - 1] = '\0';
    check(sgt_open_directory(oversized) == -EINVAL && calls == before_open, "oversized directory path rejected before dispatch");
    check(sgt_open_entry(3, "directory", -1) == -EINVAL && sgt_open_entry(3, "directory", 4) == -EINVAL,
          "invalid entry kind rejected");
    check(sgt_open_entry(3, "file", 1) < 0, "ordinary file cannot be a directory");
    check(sgt_open_entry(3, "pipe", 0) < 0, "non-regular file rejected");
    fail_stat = 1;
    check(sgt_open_entry(3, "directory", 1) == -EIO, "stat failure propagated");
    fail_stat = 0;
    check(sgt_mkdir_entry(3, "child") == 0, "directory creation");
    check(sgt_publish_entry(3, "stage", "target", 0) == 0 && published_flags == RENAME_NOREPLACE, "no-replace publication");
    check(sgt_publish_entry(3, "stage", "target", 1) == 0 && published_flags == RENAME_EXCHANGE, "exchange publication");
    expected_target_parent = 4;
    check(sgt_move_entry(3, "file", 4, "slot") == 0 && published_flags == RENAME_NOREPLACE, "cross-directory move cannot overwrite");
    expected_target_parent = 3;
    int staged = sgt_open_entry(3, "slot", 0);
    staged_identity = staged + 1;
    check(sgt_unlink_staged_file(3, "slot", staged) == ESTALE && staged_unlinks == 0, "foreign staging object remains");
    staged_identity = staged;
    check(sgt_unlink_staged_file(3, "slot", staged) == 0 && staged_unlinks == 1, "remove exact staged object");
    close(staged);
    int before = publish_calls;
    publish_error = ENOSYS;
    check(sgt_publish_entry(3, "stage", "target", 0) == ENOSYS && publish_calls == before + 1, "unsupported atomic publication has no fallback");
    publish_error = 0;
    int retained = sgt_open_directory("/directory");
    check(retained >= 0, "retained directory for sync faults");
    int sync_before = sync_calls;
    fail_stat = 1;
    check(sgt_sync_directory(retained) == EIO && sync_calls == sync_before, "failed type verification prevents sync");
    fail_stat = 0;
    sync_error = EIO;
    check(sgt_sync_directory(retained) == EIO && !descriptors[synced_descriptor].active,
          "sync error propagated and temporary descriptor closed");
    sync_error = ENOSYS;
    check(sgt_sync_directory(retained) == ENOSYS && !descriptors[synced_descriptor].active,
          "unsupported sync has no fallback");
    close_error = EACCES;
    check(sgt_sync_directory(retained) == ENOSYS, "sync error survives cleanup error");
    sync_error = 0;
    check(sgt_sync_directory(retained) == EACCES, "close error after sync is propagated");
    close_error = 0;
    check(descriptors[retained].active, "all faults preserve retained handle");
    close(retained);
    check(sgt_sync_directory(retained) == EBADF && sgt_sync_directory(-1) == EBADF, "invalid sync descriptors rejected");
    int regular = sgt_open_entry(3, "file", 0);
    check(regular >= 0 && sgt_sync_directory(regular) == ENOTDIR, "file descriptor cannot stand in for directory sync");
    if (regular >= 0) close(regular);
    int locked = sgt_open_entry(3, "file", 3);
    check(locked >= 0, "owned file for lock faults");
    int before_lock = lock_calls;
    check(sgt_lock_file(locked, 2) == EINVAL && lock_calls == before_lock, "invalid lock operation rejected before dispatch");
    fail_stat = 1;
    check(sgt_lock_file(locked, 1) == EIO && lock_calls == before_lock, "lock stat error propagated without dispatch");
    fail_stat = 0;
    lock_error = EAGAIN;
    check(sgt_lock_file(locked, 1) == EAGAIN && lock_calls == before_lock + 1, "busy lock is not retried or replaced");
    lock_error = ENOSYS;
    check(sgt_lock_file(locked, 1) == ENOSYS && descriptors[locked].active, "unsupported locking preserves caller descriptor");
    lock_error = 0;
    check(sgt_lock_file(locked, 1) == 0, "lock acquired before release error");
    lock_error = EIO;
    check(sgt_lock_file(locked, 0) == EIO && descriptors[locked].active, "release failure preserves caller descriptor");
    lock_error = 0;
    check(sgt_lock_file(locked, 0) == 0, "release after fault");
    close(locked);
    check(sgt_lock_file(-1, 1) == EBADF && sgt_lock_file(locked, 1) == EBADF, "invalid or closed lock descriptors rejected");
    int directory = sgt_open_directory("/directory");
    before_lock = lock_calls;
    check(sgt_lock_file(directory, 1) == EINVAL && lock_calls == before_lock, "directory cannot become file lease");
    close(directory);
    for (int i = 4; i < next_descriptor; i++) check(!descriptors[i].active, "owned descriptor released");
}

int main(void) {
    for (unsigned int i = 0; i < sizeof(contract_cases) / sizeof(contract_cases[0]); i++) {
        current = &contract_cases[i];
        created = 0;
        permissions();
    }
    current = &structural_case;
    structural_guards();
    printf("{\"checks\":%d,\"failures\":%d,\"cases\":%u,\"kernel_execution\":false}\n",
        checks, failures, (unsigned int)(sizeof(contract_cases) / sizeof(contract_cases[0])));
    return failures ? 1 : 0;
}
