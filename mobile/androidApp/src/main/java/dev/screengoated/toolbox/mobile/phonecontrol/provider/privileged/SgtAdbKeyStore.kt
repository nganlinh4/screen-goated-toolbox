package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.cert.Certificate
import java.util.Date
import javax.security.auth.x500.X500Principal

internal data class SgtAdbKeyMaterial(
    val privateKey: PrivateKey,
    val certificate: Certificate,
)

internal object SgtAdbKeyStore {
    fun loadOrCreate(): SgtAdbKeyMaterial {
        val keyStore = load()
        keyStore.getEntry(KEY_ALIAS, null)
            ?.let { it as? KeyStore.PrivateKeyEntry }
            ?.let { return SgtAdbKeyMaterial(it.privateKey, it.certificate) }
        LEGACY_KEY_ALIASES.forEach { alias ->
            if (keyStore.containsAlias(alias)) keyStore.deleteEntry(alias)
        }

        val now = System.currentTimeMillis()
        val generator = KeyPairGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_RSA,
            ANDROID_KEY_STORE,
        )
        generator.initialize(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
            )
                .setKeySize(KEY_SIZE_BITS)
                .setDigests(
                    KeyProperties.DIGEST_NONE,
                    KeyProperties.DIGEST_SHA256,
                    KeyProperties.DIGEST_SHA512,
                )
                .setSignaturePaddings(KeyProperties.SIGNATURE_PADDING_RSA_PKCS1)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setCertificateSubject(X500Principal(CERTIFICATE_SUBJECT))
                .setCertificateSerialNumber(
                    BigInteger(SERIAL_BITS, SecureRandom()).abs().max(BigInteger.ONE),
                )
                .setCertificateNotBefore(Date(now - CLOCK_SKEW_MS))
                .setCertificateNotAfter(Date(now + CERTIFICATE_LIFETIME_MS))
                .setUserAuthenticationRequired(false)
                .build(),
        )
        generator.generateKeyPair()
        val entry = load().getEntry(KEY_ALIAS, null) as? KeyStore.PrivateKeyEntry
            ?: error("Android Keystore did not return the generated ADB key.")
        return SgtAdbKeyMaterial(entry.privateKey, entry.certificate)
    }

    fun exists(): Boolean = runCatching { load().containsAlias(KEY_ALIAS) }.getOrDefault(false)

    fun delete() {
        load().let { keyStore ->
            (listOf(KEY_ALIAS) + LEGACY_KEY_ALIASES).forEach { alias ->
                if (keyStore.containsAlias(alias)) keyStore.deleteEntry(alias)
            }
        }
    }

    private fun load(): KeyStore = KeyStore.getInstance(ANDROID_KEY_STORE).apply {
        load(null)
    }

    private const val ANDROID_KEY_STORE = "AndroidKeyStore"
    private const val KEY_ALIAS = "sgt_phone_control_adb_v3"
    private val LEGACY_KEY_ALIASES = listOf(
        "sgt_phone_control_adb_v1",
        "sgt_phone_control_adb_v2",
    )
    private const val KEY_SIZE_BITS = 2048
    private const val SERIAL_BITS = 128
    private const val CERTIFICATE_SUBJECT = "CN=Screen Goated Toolbox Phone Control"
    private const val CLOCK_SKEW_MS = 86_400_000L
    private const val CERTIFICATE_LIFETIME_MS = 20L * 365L * 86_400_000L
}
