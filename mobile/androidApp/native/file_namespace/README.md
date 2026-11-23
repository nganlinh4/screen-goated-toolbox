# File namespace primitive

The Phone Control file transaction owns preparation, authorization, staging,
version audits and recovery receipts in `provider/fileio/`. This library supplies
only descriptor-relative namespace operations. Both app flavors use it. History
media creation also uses the descriptor wrapper without the Phone Control
authorization or publication lifecycle. Its creation proof comes from the open
file descriptor, with a bounded streaming fingerprint before it is closed.
History database/settings publication uses the same namespace wrapper and owns
its commit boundary separately from Phone Control authorization.

`openEntry` rejects symlinks and enforces regular-file or directory type.
Directory handles use `O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, for both
path-based `openDirectory` and descriptor-relative lookup. Retaining a location
requires no enumeration access; child reads and mutations retain their normal
search/read/write checks. Data file handles keep their existing access modes.
`syncDirectory` opens `.` relative to the retained directory with a separate
read-capable, no-follow directory descriptor, calls `fsync`, and closes that
temporary descriptor on success or failure. Retention itself still uses O_PATH;
sync requires its own read/search permissions and filesystem capability. Errors
have no path-reopen or no-op fallback. File-content sync and containing-directory
sync are separate durability boundaries.
Existing writable opens use O_RDWR without create or truncate; callers verify
the descriptor identity before replacing its contents. The descriptor writer
checks cancellation before truncation, writes the entire buffer, and fsyncs it.
`lockFile` validates a regular-file descriptor and uses nonblocking exclusive
`flock`, or unlock. Kotlin retains a duplicate descriptor for the lock lease so
release cannot act on a reused descriptor number. Busy/unsupported/error results
are explicit failures; there is no process-local locking fallback.
The permission distinction follows the Linux
[`open(2)` contract](https://man7.org/linux/man-pages/man2/open.2.html).
Exclusive stage creation uses mode 0600. `makeDirectory` uses mode 0700.
`publish` uses `renameat2` with no-replace for an initially absent destination,
or exchange for an existing destination. Errors return errno; unsupported atomic
operations have no alternate writer. The direct syscall keeps the API 29 build
independent from the newer libc entry point. UTF-8 byte arrays preserve native
filenames and reject embedded NUL and invalid leaves.

Open descriptors retain object identity; they do not exclude external writers.
Exchange retains the displaced entry for the transaction's audit. Production
code never unlinks a staging/recovery name without identity-bound ownership.
History retirement uses descriptor-relative no-replace moves into app-owned
staging on the same filesystem. It audits the moved file before unlinking it
inside that staging directory, and restores an unexpected file only to a
vacant source name.

The app's Gradle `externalNativeBuildFullDebug` and
`externalNativeBuildPlayDebug` tasks build the library. The existing
`creationAcceptanceNativeAbi` property selects the native ABI for development.
The CMake target enables strict C warnings and 16 KiB ELF segment alignment.

`tests/NamespaceSmoke.java` is a standalone Android shell test, excluded from
the library and app payload. Compile it with the app's compiled Kotlin file-IO
classes, Kotlin standard library and Android SDK, then dex those same inputs
with a compatible D8. Run it with `app_process`, the built library on the native
library path, and one fresh empty test-owned directory. It checks the actual JNI
binding, descriptor wrapper, bounded reads, synced writes, symlink rejection,
exchange identities and concurrent create-only publication. Use an isolated
test environment; preserve application state and user files. JVM fault tests
cover the transaction's authorization, cancellation and recovery state machine.

From the repository root, a host compiler can check the C argument, permission
and descriptor contract with a test-only POSIX model:

```text
python -B mobile/androidApp/native/file_namespace/tests/run_host_contract.py --cc gcc
```

The six permission vectors come from
`parity-fixtures/history-ui/namespace-access.json`. The runner owns and removes
its temporary compiler output. Host modeling and ABI cross-compilation do not
establish Android/Linux kernel behavior, SELinux, FUSE, or device acceptance.
