package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbKeyStore
import java.security.PrivateKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SgtAdbKeyStoreDeviceTest {
    @Test
    fun nonExportableKeyCanSignAnAdbChallenge() {
        val material = SgtAdbKeyStore.loadOrCreate()
        val challenge = ByteArray(ADB_CHALLENGE_BYTES) { index -> index.toByte() }

        val androidPubkey = Class.forName(ANDROID_PUBKEY_CLASS)
        val signer = androidPubkey.getDeclaredMethod(
            "adbAuthSign",
            PrivateKey::class.java,
            ByteArray::class.java,
        ).apply { isAccessible = true }
        val signature = signer.invoke(null, material.privateKey, challenge) as ByteArray

        assertEquals(ADB_RSA_SIGNATURE_BYTES, signature.size)
        assertFalse(material.privateKey.encoded?.isNotEmpty() == true)
    }

    private companion object {
        const val ADB_CHALLENGE_BYTES = 20
        const val ADB_RSA_SIGNATURE_BYTES = 256
        const val ANDROID_PUBKEY_CLASS = "io.github.muntashirakon.adb.AndroidPubkey"
    }
}
