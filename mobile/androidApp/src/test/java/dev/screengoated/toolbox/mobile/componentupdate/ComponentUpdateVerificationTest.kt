package dev.screengoated.toolbox.mobile.componentupdate

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ComponentUpdateVerificationTest {
    @Test
    fun `raw p256 signatures bind exact catalog bytes`() {
        val publicKey = decode(
            "0456b573c6eb8cd3996eb3ca132be504746107b782e511a6b5884d5e3e4d1ca804" +
                "68b8764005a2957f8f9e1f2798ea703630ee2aaf01051d0cba3d9e63ebca20b0",
        )
        val signature = decode(
            "5003f14e2f0fb17a4cf134b4d40d2367ac891787345d7bf7614d7c5062770596" +
                "a10720f99c936ba5a8d22e0dfb351055418138fc80517b061168a292d51907c6",
        )
        val payload = "signed catalog fixture".encodeToByteArray()
        assertTrue(verifyP256Signature(publicKey, payload, signature))
        assertFalse(
            verifyP256Signature(
                publicKey,
                "signed catalog fixturf".encodeToByteArray(),
                signature,
            ),
        )
        signature[17] = (signature[17].toInt() xor 1).toByte()
        assertFalse(verifyP256Signature(publicKey, payload, signature))
    }

    private fun decode(value: String): ByteArray = ByteArray(value.length / 2) { index ->
        value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
}
