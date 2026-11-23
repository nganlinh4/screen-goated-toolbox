package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio;

import android.os.ParcelFileDescriptor;
import android.system.Os;
import android.system.OsConstants;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CyclicBarrier;
import java.util.concurrent.atomic.AtomicInteger;

/** Isolated Android shell probe of the compiled Kotlin JNI binding and native library. */
public final class NamespaceSmoke {
    private static final FileNamespaceNative NATIVE = FileNamespaceNative.INSTANCE;
    private static int checks;

    private static byte[] bytes(String value) {
        return value.getBytes(StandardCharsets.UTF_8);
    }

    private static void check(boolean condition, String label) {
        checks++;
        if (!condition) throw new AssertionError(label);
    }

    private static ParcelFileDescriptor open(int directory, String name, int kind) {
        int fd = NATIVE.openEntry(directory, bytes(name), kind);
        check(fd >= 0, "open " + name + ": " + fd);
        return ParcelFileDescriptor.adoptFd(fd);
    }

    private static void create(int directory, String name, String content) throws Exception {
        try (ParcelFileDescriptor file = open(directory, name, 2)) {
            byte[] data = bytes(content);
            int offset = 0;
            while (offset < data.length) {
                int count = Os.write(file.getFileDescriptor(), data, offset, data.length - offset);
                check(count > 0, "write progress");
                offset += count;
            }
            Os.fsync(file.getFileDescriptor());
        }
    }

    private static String read(int directory, String name) throws Exception {
        try (ParcelFileDescriptor file = open(directory, name, 0)) {
            byte[] data = new byte[1024];
            int count = Os.pread(file.getFileDescriptor(), data, 0, data.length, 0);
            return new String(data, 0, count, StandardCharsets.UTF_8);
        }
    }

    private static void race(int directory, int iteration) throws Exception {
        String first = "race-a-" + iteration;
        String second = "race-b-" + iteration;
        String target = "winner-" + iteration;
        create(directory, first, "first complete bytes");
        create(directory, second, "second complete bytes");
        CyclicBarrier start = new CyclicBarrier(3);
        AtomicInteger firstResult = new AtomicInteger(-1);
        AtomicInteger secondResult = new AtomicInteger(-1);
        Thread a = new Thread(() -> publishAfterBarrier(directory, first, target, start, firstResult));
        Thread b = new Thread(() -> publishAfterBarrier(directory, second, target, start, secondResult));
        a.start();
        b.start();
        start.await();
        a.join(5000);
        b.join(5000);
        check(!a.isAlive() && !b.isAlive(), "concurrent publication settled");
        check((firstResult.get() == 0 && secondResult.get() == OsConstants.EEXIST) ||
            (secondResult.get() == 0 && firstResult.get() == OsConstants.EEXIST), "one create-only winner");
        check(read(directory, target).equals(firstResult.get() == 0 ?
            "first complete bytes" : "second complete bytes"), "complete winner");
        check(read(directory, firstResult.get() == 0 ? second : first).equals(firstResult.get() == 0 ?
            "second complete bytes" : "first complete bytes"), "losing source retained");
    }

    private static void publishAfterBarrier(int directory, String source, String target,
                                          CyclicBarrier barrier, AtomicInteger result) {
        try {
            barrier.await();
            result.set(NATIVE.publish(directory, bytes(source), bytes(target), false));
        } catch (Exception error) {
            result.set(-1);
        }
    }

    private static void descriptorIo(File root) throws Exception {
        AndroidFileTransactionIo io = AndroidFileTransactionIo.INSTANCE;
        try (TransactionDirectory parent = io.directory(root.getPath())) {
            FileObjectId baseline;
            try (TransactionFile original = io.file(parent, "descriptor-target", true)) {
                original.writeSynced(bytes("descriptor original"), () -> false);
                baseline = original.getIdentity();
                try {
                    original.readBounded(3);
                    throw new AssertionError("bounded read admitted oversized file");
                } catch (Exception expected) {
                    check(expected instanceof FileSnapshotLimitExceeded, "typed snapshot limit");
                    check(((FileSnapshotLimitExceeded) expected).getMaximumBytes() == 3, "native read byte budget");
                }
                try (TransactionFile stage = io.file(parent, "descriptor-stage", true)) {
                    byte[] large = new byte[128 * 1024 + 3];
                    java.util.Arrays.fill(large, (byte) 0xA5);
                    stage.writeSynced(large, () -> false);
                    check(java.util.Arrays.equals(large, stage.readBounded(large.length)), "multi-chunk synced descriptor bytes");
                    io.publish(parent, "descriptor-stage", "descriptor-target", true);
                    parent.sync();
                    check(new String(original.readBounded(1024), StandardCharsets.UTF_8).equals("descriptor original"),
                        "original descriptor survives exchange");
                    try (TransactionFile backup = io.file(parent, "descriptor-stage", false);
                         TransactionFile current = io.file(parent, "descriptor-target", false)) {
                        check(backup.getIdentity().equals(baseline), "wrapper recovery identity");
                        check(current.getIdentity().equals(stage.getIdentity()), "wrapper publication identity");
                        check(java.util.Arrays.equals(large, current.readBounded(large.length)), "wrapper publication bytes");
                    }
                }
            }
            try (TransactionFile cancelled = io.file(parent, "descriptor-cancelled", true)) {
                try {
                    cancelled.writeSynced(bytes("cancelled bytes"), () -> true);
                    throw new AssertionError("cancelled write proceeded");
                } catch (Exception expected) {
                    check(expected instanceof FileWriteCancelled, "typed staging cancellation");
                    check(cancelled.readBounded(0).length == 0, "cancelled stage stays empty");
                }
            }
            try {
                io.file(parent, "link", false);
                throw new AssertionError("wrapper followed symlink");
            } catch (Exception expected) {
                check(expected instanceof FileNamespaceException, "typed namespace failure");
                check(((FileNamespaceException) expected).getErrorNumber() > 0, "typed native errno");
            }
            check(io.file(parent, "descriptor-absent", false) == null, "missing entry is explicit");
            check(parent.currentPath().equals(root.getPath()), "descriptor path observation");
            check(io.makeDirectory(parent, "descriptor-parent"), "descriptor mkdir");
            try (TransactionDirectory removed = io.directory(parent, "descriptor-parent")) {
                check(new File(root, "descriptor-parent").delete(), "remove empty owned test parent");
                check(removed.currentPath() == null, "deleted parent has no observed namespace path");
            }
        }
    }

    private static void writableAndLockIo(File root) throws Exception {
        AndroidFileTransactionIo io = AndroidFileTransactionIo.INSTANCE;
        try (TransactionDirectory parent = io.directory(root.getPath());
             TransactionFile writable = io.writableFile(parent, "descriptor-target")) {
            check(writable != null, "existing writable descriptor");
            writable.requireSingleLink();
            try (TransactionLock lease = writable.lockExclusive()) {
                check(lease != null, "owned lock lease");
                try (TransactionFile contender = io.writableFile(parent, "descriptor-target")) {
                    try {
                        contender.lockExclusive().close();
                        throw new AssertionError("concurrent descriptor lock acquired");
                    } catch (Exception expected) {
                        check(expected instanceof FileNamespaceException, "typed busy lock");
                        check(((FileNamespaceException) expected).getErrorNumber() == OsConstants.EAGAIN, "nonblocking lock busy");
                    }
                }
                writable.writeSynced(bytes("short"), () -> false);
                check(new String(writable.readBounded(1024), StandardCharsets.UTF_8).equals("short"),
                    "whole-buffer replacement removes old tail");
            }
            TransactionFile original = io.writableFile(parent, "descriptor-target");
            boolean originalClosed = false;
            TransactionLock retained = null;
            try {
                retained = original.lockExclusive();
                original.close();
                originalClosed = true;
                try (TransactionFile contender = io.writableFile(parent, "descriptor-target")) {
                    try {
                        contender.lockExclusive().close();
                        throw new AssertionError("lock lost with original descriptor");
                    } catch (Exception expected) {
                        check(expected instanceof FileNamespaceException, "duplicate descriptor retains lock");
                    }
                }
            } finally {
                try { if (retained != null) retained.close(); } finally { if (!originalClosed) original.close(); }
            }
            try (TransactionLock reacquired = writable.lockExclusive()) {
                check(reacquired != null, "lock released with retained descriptor");
            }
        }
    }

    private static void retirementIo(File root) throws Exception {
        AndroidFileTransactionIo io = AndroidFileTransactionIo.INSTANCE;
        try (TransactionDirectory source = io.directory(root.getPath())) {
            check(io.makeDirectory(source, "retirement"), "create private retirement directory");
            try (TransactionDirectory staging = io.directory(source, "retirement");
                 TransactionFile owned = io.file(source, "owned-media", true)) {
                owned.writeSynced(bytes("same bytes"), () -> false);
                io.publish(source, "owned-media", "original-media", false);
                try (TransactionFile foreign = io.file(source, "owned-media", true)) {
                    foreign.writeSynced(bytes("same bytes"), () -> false);
                    io.moveNoReplace(source, "owned-media", staging, "slot");
                    try {
                        io.unlinkStagedFile(staging, "slot", owned);
                        throw new AssertionError("identical replacement was unlinked");
                    } catch (Exception expected) {
                        check(expected instanceof FileNamespaceException &&
                            ((FileNamespaceException) expected).getErrorNumber() == OsConstants.ESTALE,
                            "replacement fails staged identity check");
                    }
                    try (TransactionFile reoccupied = io.file(source, "owned-media", true)) {
                        try {
                            io.moveNoReplace(staging, "slot", source, "owned-media");
                            throw new AssertionError("restore overwrote another file");
                        } catch (Exception expected) {
                            check(expected instanceof FileEntryAlreadyExists, "restore respects occupied source");
                        }
                        check(reoccupied.readBounded(0).length == 0, "new source entry preserved");
                    }
                    check(new String(foreign.readBounded(32), StandardCharsets.UTF_8).equals("same bytes"),
                        "unrestored replacement survives in retirement");
                }
                io.moveNoReplace(source, "original-media", staging, "owned-slot");
                io.unlinkStagedFile(staging, "owned-slot", owned);
                check(io.file(staging, "owned-slot", false) == null, "exact staged object removed");
            }
        }
    }

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 1) throw new IllegalArgumentException("Expected a fresh empty owned directory");
        File root = new File(arguments[0]).getCanonicalFile();
        check(root.isDirectory() && root.list().length == 0, "empty owned probe root");
        java.io.FileDescriptor initial = Os.open(root.getPath(),
            OsConstants.O_RDONLY | OsConstants.O_CLOEXEC | OsConstants.O_NOFOLLOW, 0);
        try (ParcelFileDescriptor parent = ParcelFileDescriptor.dup(initial)) {
            Os.close(initial);
            int directory = parent.getFd();
            check(NATIVE.syncDirectory(directory) == 0, "directory sync binding");
            create(directory, "stage", "complete new bytes");
            check(NATIVE.publish(directory, bytes("stage"), bytes("target"), false) == 0, "no-replace install");
            check(read(directory, "target").equals("complete new bytes"), "installed bytes");
            check(NATIVE.openEntry(directory, bytes("stage"), 0) == -OsConstants.ENOENT, "stage consumed");
            create(directory, "stage", "replacement bytes");
            check(NATIVE.publish(directory, bytes("stage"), bytes("target"), false) == OsConstants.EEXIST, "no-replace collision");
            check(read(directory, "target").equals("complete new bytes"), "collision preserves destination");
            try (ParcelFileDescriptor original = open(directory, "target", 0);
                 ParcelFileDescriptor replacement = open(directory, "stage", 0)) {
                long originalInode = Os.fstat(original.getFileDescriptor()).st_ino;
                long replacementInode = Os.fstat(replacement.getFileDescriptor()).st_ino;
                check(NATIVE.publish(directory, bytes("stage"), bytes("target"), true) == 0, "atomic exchange");
                try (ParcelFileDescriptor current = open(directory, "target", 0);
                     ParcelFileDescriptor backup = open(directory, "stage", 0)) {
                    check(Os.fstat(current.getFileDescriptor()).st_ino == replacementInode, "installed identity");
                    check(Os.fstat(backup.getFileDescriptor()).st_ino == originalInode, "displaced identity");
                }
                check(read(directory, "target").equals("replacement bytes"), "exchange bytes");
                check(read(directory, "stage").equals("complete new bytes"), "recovery bytes");
            }
            create(directory, "unicode-\uD83D\uDCC4-\u00e9", "UTF-8 leaf");
            check(read(directory, "unicode-\uD83D\uDCC4-\u00e9").equals("UTF-8 leaf"), "UTF-8 survives JNI");
            for (String invalid : new String[] {"", ".", "..", "a/b", "a\u0000b"}) {
                check(NATIVE.openEntry(directory, bytes(invalid), 2) == -OsConstants.EINVAL, "invalid leaf rejected");
            }
            Os.symlink("target", new File(root, "link").getPath());
            check(NATIVE.openEntry(directory, bytes("link"), 0) < 0, "file symlink rejected");
            check(NATIVE.openEntry(directory, bytes("link"), 1) < 0, "directory symlink rejected");
            check(NATIVE.makeDirectory(directory, bytes("folder")) == 0, "relative mkdir");
            check(NATIVE.openEntry(directory, bytes("folder"), 0) < 0, "directory rejected as regular file");
            try (ParcelFileDescriptor retained = open(directory, "folder", 1)) {
                Os.rename(new File(root, "folder").getPath(), new File(root, "moved").getPath());
                check(NATIVE.makeDirectory(retained.getFd(), bytes("child")) == 0, "descriptor-relative mkdir after rename");
                check(new File(root, "moved/child").isDirectory(), "retained parent identity");
            }
            check(NATIVE.publish(directory, bytes("stage"), bytes("absent"), true) == OsConstants.ENOENT, "exchange requires target");
            check(read(directory, "stage").equals("complete new bytes"), "failed exchange preserves stage");
            for (int iteration = 0; iteration < 32; iteration++) race(directory, iteration);
        }
        descriptorIo(root);
        writableAndLockIo(root);
        retirementIo(root);
        System.out.println("FILE_NAMESPACE_SMOKE_PASS checks=" + checks + " create_races=32");
    }
}
