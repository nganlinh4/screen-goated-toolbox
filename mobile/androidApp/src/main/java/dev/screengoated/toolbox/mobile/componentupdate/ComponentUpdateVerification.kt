package dev.screengoated.toolbox.mobile.componentupdate

import android.content.Context
import java.math.BigInteger
import java.security.AlgorithmParameters
import java.security.KeyFactory
import java.security.MessageDigest
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.security.spec.ECParameterSpec
import java.security.spec.ECPoint
import java.security.spec.ECPublicKeySpec
import org.json.JSONObject

internal data class VerifiedComponentCatalog(
    val sequence: Long,
    val root: JSONObject,
)

internal fun verifyComponentCatalog(
    context: Context,
    catalog: ByteArray,
    rawSignature: ByteArray,
    hostVersion: String,
): VerifiedComponentCatalog {
    require(catalog.isNotEmpty() && catalog.size <= MAXIMUM_CATALOG_BYTES)
    require(rawSignature.size == 64) { "Component catalog signature shape is invalid" }
    val publicPoint = context.assets.open(PUBLIC_KEY_ASSET).bufferedReader().use { reader ->
        decodeHex(reader.readText().trim())
    }
    require(publicPoint.size == 65 && publicPoint[0] == 4.toByte()) {
        "Component catalog public key is invalid"
    }
    require(verifyP256Signature(publicPoint, catalog, rawSignature)) {
        "Component catalog signature is invalid"
    }

    val root = JSONObject(catalog.toString(Charsets.UTF_8))
    require(root.getInt("schemaVersion") == 1 && root.getString("channel") == "stable")
    val sequence = root.getLong("sequence")
    require(sequence > 0L)
    require(compareVersions(hostVersion, root.getString("minHostVersion")) >= 0)
    require(compareVersions(hostVersion, root.getString("maxHostVersionExclusive")) < 0)
    validateContracts(root)
    validatePolicies(root)
    return VerifiedComponentCatalog(sequence, root)
}

internal fun verifyP256Signature(
    publicPoint: ByteArray,
    payload: ByteArray,
    rawSignature: ByteArray,
): Boolean {
    val coordinates = when {
        publicPoint.size == 64 -> publicPoint
        publicPoint.size == 65 && publicPoint[0] == 4.toByte() -> publicPoint.copyOfRange(1, 65)
        else -> return false
    }
    if (rawSignature.size != 64) return false
    val parameters = AlgorithmParameters.getInstance("EC").apply {
        init(ECGenParameterSpec("secp256r1"))
    }.getParameterSpec(ECParameterSpec::class.java)
    val point = ECPoint(
        BigInteger(1, coordinates.copyOfRange(0, 32)),
        BigInteger(1, coordinates.copyOfRange(32, 64)),
    )
    val publicKey = KeyFactory.getInstance("EC").generatePublic(ECPublicKeySpec(point, parameters))
    val verified = Signature.getInstance("SHA256withECDSA").run {
        initVerify(publicKey)
        update(payload)
        verify(rawEcdsaToDer(rawSignature))
    }
    return verified
}

internal fun sha256(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

private fun validateContracts(root: JSONObject) {
    val contracts = root.getJSONArray("contracts")
    require(contracts.length() in 1..64)
    val names = mutableSetOf<String>()
    for (index in 0 until contracts.length()) {
        val contract = contracts.getJSONObject(index)
        val name = contract.getString("name")
        val platform = contract.getString("platform")
        require(validToken(name, 96) && validToken(platform, 32) && names.add(name))
        require(contract.get("delivery") is JSONObject)
    }
}

private fun validatePolicies(root: JSONObject) {
    val policies = root.getJSONArray("policies")
    require(policies.length() <= 128)
    val ids = mutableSetOf<String>()
    for (index in 0 until policies.length()) {
        val policy = policies.getJSONObject(index)
        val id = policy.getString("id")
        require(validToken(id, 96) && ids.add(id))
        require(validToken(policy.getString("mode"), 32))
        require(validToken(policy.getString("group"), 64))
        require(policy.getLong("checkHours") in 1..(24L * 365L))
    }
}

private fun compareVersions(left: String, right: String): Int {
    val leftParts = parseVersion(left)
    val rightParts = parseVersion(right)
    for (index in 0..2) {
        leftParts[index].compareTo(rightParts[index]).takeIf { it != 0 }?.let { return it }
    }
    return 0
}

private fun parseVersion(value: String): List<Int> {
    val match = Regex("^(\\d+)\\.(\\d+)\\.(\\d+)(?:[-+].*)?$").matchEntire(value)
        ?: error("Component catalog host version is invalid")
    return match.groupValues.drop(1).map(String::toInt)
}

private fun validToken(value: String, maximum: Int): Boolean =
    value.isNotEmpty() && value.length <= maximum &&
        value.all { it.isLetterOrDigit() || it in "-_." }

private fun rawEcdsaToDer(raw: ByteArray): ByteArray {
    val r = unsignedInteger(raw.copyOfRange(0, 32))
    val s = unsignedInteger(raw.copyOfRange(32, 64))
    val body = byteArrayOf(0x02, r.size.toByte()) + r + byteArrayOf(0x02, s.size.toByte()) + s
    return byteArrayOf(0x30, body.size.toByte()) + body
}

private fun unsignedInteger(value: ByteArray): ByteArray {
    var first = 0
    while (first < value.size && value[first] == 0.toByte()) first += 1
    val stripped = if (first == value.size) byteArrayOf(0) else value.copyOfRange(first, value.size)
    return if (stripped[0].toInt() and 0x80 != 0) byteArrayOf(0) + stripped else stripped
}

private fun decodeHex(value: String): ByteArray {
    require(value.length % 2 == 0 && value.all(Char::isDigitOrHexLetter))
    return ByteArray(value.length / 2) { index ->
        value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
}

private fun Char.isDigitOrHexLetter(): Boolean = isDigit() || lowercaseChar() in 'a'..'f'

private const val PUBLIC_KEY_ASSET = "component-update/public-key.hex"
internal const val MAXIMUM_CATALOG_BYTES = 2 * 1024 * 1024
