#ifndef SGT_NAMESPACE_TEST_PLATFORM_H
#define SGT_NAMESPACE_TEST_PLATFORM_H

#include <errno.h>

#ifndef ESTALE
#define ESTALE 116
#endif

/* Host permission/descriptor model only; these declarations are not kernel evidence. */
#define O_RDONLY 0
#define O_RDWR 2
#define O_CREAT 64
#define O_EXCL 128
#define O_NONBLOCK 2048
#define O_DIRECTORY 65536
#define O_NOFOLLOW 131072
#define O_CLOEXEC 524288
#define O_PATH 2097152
#define RENAME_NOREPLACE 1U
#define RENAME_EXCHANGE 2U
#define AT_SYMLINK_NOFOLLOW 256
#define SYS_renameat2 1L
#define LOCK_EX 2
#define LOCK_NB 4
#define LOCK_UN 8
#define S_IFREG 0100000U
#define S_IFDIR 0040000U
#define S_IFLNK 0120000U
#define S_IFIFO 0010000U
#define S_ISDIR(mode) (((mode) & 0170000U) == S_IFDIR)
#define S_ISREG(mode) (((mode) & 0170000U) == S_IFREG)

struct stat { unsigned int st_mode; int st_dev, st_ino; };
int open(const char *path, int flags, ...);
int openat(int parent, const char *name, int flags, ...);
int close(int descriptor);
int fstat(int descriptor, struct stat *information);
int fstatat(int directory, const char *name, struct stat *information, int flags);
int unlinkat(int directory, const char *name, int flags);
int fsync(int descriptor);
int flock(int descriptor, int flags);
int mkdirat(int parent, const char *name, unsigned int mode);
long syscall(long number, ...);

#endif
