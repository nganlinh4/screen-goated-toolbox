package dev.screengoated.toolbox.mobile.storage

import android.content.SharedPreferences
import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Test

class SecureBootstrapTest {
    @Test
    fun `existing encrypted store never causes missing master key creation`() {
        val checked = mutableListOf<String>()
        assertThrows(SecureStoreUnavailableException::class.java) {
            withSecureStoreAccess {
                existingSecureStore { name ->
                    checked += name
                    name.endsWith(".bak")
                }
            }
        }
        assertEquals(listOf("sgt_mobile_secure.xml", "sgt_mobile_secure.xml.bak"), checked)
        checked.clear()
        assertEquals(true, existingSecureStore { name -> checked += name; !name.endsWith(".bak") })
        assertEquals(listOf("sgt_mobile_secure.xml", "sgt_mobile_secure.xml.bak"), checked)
        assertThrows(SecureStoreUnavailableException::class.java) {
            withSecureStoreAccess { requireExistingKeyIfStoreExists(true) { false } }
        }
        requireExistingKeyIfStoreExists(true) { true }
        requireExistingKeyIfStoreExists(false) { error("new stores do not need an existing alias") }
        assertThrows(SecureStoreUnavailableException::class.java) {
            withSecureStoreAccess {
                existingSecureStore { throw java.io.IOException("injected metadata failure") }
            }
        }
        assertThrows(SecureStoreUnavailableException::class.java) {
            withSecureStoreAccess {
                requireExistingKeyIfStoreExists(true) { throw SecurityException("injected key failure") }
            }
        }
    }

    @Test
    fun `secure store file probe refuses directories and preserves existing files`() {
        val directory = java.nio.file.Files.createTempDirectory("secure-store-probe")
        try {
            val file = directory.resolve("settings.xml")
            assertEquals(false, secureStoreFileExists(file))
            java.nio.file.Files.write(file, "existing".toByteArray())
            assertEquals(true, secureStoreFileExists(file))
            assertThrows(SecureStoreUnavailableException::class.java) {
                withSecureStoreAccess { secureStoreFileExists(directory) }
            }
            assertEquals("existing", java.nio.file.Files.readAllBytes(file).toString(Charsets.UTF_8))
        } finally {
            java.nio.file.Files.delete(directory.resolve("settings.xml"))
            java.nio.file.Files.delete(directory)
        }
    }

    @Test
    fun `existing XML must contain both unique keysets without declarations or truncation`() {
        val key = "__androidx_security_crypto_encrypted_prefs_key_keyset__"
        val value = "__androidx_security_crypto_encrypted_prefs_value_keyset__"
        fun entry(name: String) = "<string name=\"$name\">aabb</string>"
        fun inspect(xml: String) = withSecureStoreAccess { inspectSecureStoreXml(xml.byteInputStream()) }
        inspect("<map>${entry(key)}${entry(value)}<string name=\"encrypted\">xyz</string></map>")
        listOf(
            "<map>${entry(key)}</map>",
            "<map>${entry(key)}${entry(key)}${entry(value)}</map>",
            "<!DOCTYPE map [<!ENTITY x SYSTEM 'file:///never-read'>]><map>${entry(key)}${entry(value)}</map>",
            "<map>${entry(key)}${entry(value)}",
            "<map>${entry(key)}<int name=\"$value\" value=\"1\" /></map>",
        ).forEach { xml -> assertThrows(SecureStoreUnavailableException::class.java) { inspect(xml) } }
        assertThrows(SecureStoreUnavailableException::class.java) {
            withSecureStoreAccess { inspectSecureStoreXml(ByteArray(16 * 1024 * 1024 + 1).inputStream()) }
        }
    }

    @Test
    fun `failed open retains existing data and retry opens it once`() {
        val persisted = mutableMapOf("value" to "existing")
        val attempts = AtomicInteger()
        val bootstrap = SecureBootstrap {
            if (attempts.incrementAndGet() == 1) {
                throw SecureStoreUnavailableException(IllegalStateException("injected"))
            }
            persisted
        }
        assertNull(bootstrap.retry())
        assertNull(bootstrap.current)
        assertEquals("existing", persisted["value"])
        assertSame(persisted, bootstrap.retry())
        assertSame(persisted, bootstrap.retry())
        assertEquals(2, attempts.get())
    }

    @Test
    fun `decrypt failure during initial read becomes unavailable without writes`() {
        val writes = AtomicInteger()
        val preferences = Proxy.newProxyInstance(
            SharedPreferences::class.java.classLoader,
            arrayOf(SharedPreferences::class.java),
        ) { _, method, _ ->
            when (method.name) {
                "getString" -> throw SecurityException("injected decrypt failure")
                "edit" -> { writes.incrementAndGet(); error("unexpected write") }
                else -> error("unexpected preference call")
            }
        } as SharedPreferences
        val bootstrap = SecureBootstrap {
            val store = SecureSettingsStore(preferences, Json)
            store.loadApiKey()
            store
        }
        assertNull(bootstrap.retry())
        assertEquals(0, writes.get())
    }

    @Test
    fun `unrelated programming failures are not mislabeled as secure storage failures`() {
        val bootstrap = SecureBootstrap<Any> { error("unrelated") }
        assertThrows(IllegalStateException::class.java) { bootstrap.retry() }
        assertNull(bootstrap.current)
    }

    @Test
    fun `concurrent retries publish only one opened container`() {
        val attempts = AtomicInteger()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val value = Any()
        val bootstrap = SecureBootstrap {
            attempts.incrementAndGet()
            entered.countDown()
            check(release.await(2, TimeUnit.SECONDS))
            value
        }
        val pool = Executors.newFixedThreadPool(2)
        try {
            val first = pool.submit<Any?> { bootstrap.retry() }
            check(entered.await(2, TimeUnit.SECONDS))
            val second = pool.submit<Any?> { bootstrap.retry() }
            release.countDown()
            assertSame(value, first.get(2, TimeUnit.SECONDS))
            assertSame(value, second.get(2, TimeUnit.SECONDS))
            assertEquals(1, attempts.get())
        } finally {
            release.countDown()
            pool.shutdownNow()
        }
    }

    @Test
    fun `replacement observer sees a retry that completes after rotation`() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val value = Any()
        val bootstrap = SecureBootstrap {
            entered.countDown()
            check(release.await(2, TimeUnit.SECONDS))
            value
        }
        val worker = Executors.newSingleThreadExecutor()
        try {
            val retry = worker.submit<Any?> { bootstrap.retry() }
            check(entered.await(2, TimeUnit.SECONDS))
            val replacementObserver = bootstrap.state
            assertNull(replacementObserver.value)
            release.countDown()
            assertSame(value, retry.get(2, TimeUnit.SECONDS))
            assertSame(value, replacementObserver.value)
        } finally {
            release.countDown()
            worker.shutdownNow()
        }
    }
}
