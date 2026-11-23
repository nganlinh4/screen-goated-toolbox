package dev.screengoated.toolbox.mobile.storage

import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.StringReader
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.attribute.BasicFileAttributes
import javax.xml.parsers.SAXParserFactory
import org.xml.sax.Attributes
import org.xml.sax.InputSource
import org.xml.sax.SAXException
import org.xml.sax.helpers.DefaultHandler

internal const val SECURE_SETTINGS_PREFS_NAME = "sgt_mobile_secure"
private const val MAX_STORE_BYTES = 16 * 1024 * 1024
private val KEYSETS = setOf(
    "__androidx_security_crypto_encrypted_prefs_key_keyset__",
    "__androidx_security_crypto_encrypted_prefs_value_keyset__",
)

internal fun secureStoreFileExists(path: Path): Boolean = try {
    val attributes = Files.readAttributes(path, BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
    check(attributes.isRegularFile) { "Secure-store path is not a regular file" }
    true
} catch (_: NoSuchFileException) {
    false
}

/** Check the backup independently: Android would otherwise restore it on open. */
internal fun existingSecureStore(probe: (String) -> Boolean): Boolean {
    val main = probe("$SECURE_SETTINGS_PREFS_NAME.xml")
    val backup = probe("$SECURE_SETTINGS_PREFS_NAME.xml.bak")
    check(!backup) { "Secure-store backup requires recovery" }
    return main
}

internal fun requireExistingKeyIfStoreExists(storeExists: Boolean, keyExists: () -> Boolean) {
    check(!storeExists || keyExists()) { "Existing secure-store key is unavailable" }
}

/** Inspect without SharedPreferences, which can restore or rewrite a backup. */
internal fun inspectSecureStoreXml(input: InputStream) {
    val bytes = ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val count = input.read(buffer, 0, minOf(buffer.size, MAX_STORE_BYTES + 1 - bytes.size()))
        if (count < 0) break
        bytes.write(buffer, 0, count)
        check(bytes.size() <= MAX_STORE_BYTES) { "Secure store is too large" }
    }
    val xml = Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes.toByteArray())).toString()
    // Android-generated preference XML never requires declarations, comments, or CDATA.
    // Reject them before parsing so no DTD or entity can cause external access.
    check(!xml.contains("<!") && !xml.contains('\u0000')) { "Unsupported secure-store XML" }
    val names = mutableSetOf<String>()
    val found = mutableSetOf<String>()
    var depth = 0
    var currentKeyset: String? = null
    val value = StringBuilder()
    val handler = object : DefaultHandler() {
        override fun resolveEntity(publicId: String?, systemId: String?): InputSource =
            throw SAXException("External entity rejected")

        override fun startElement(uri: String?, localName: String?, qName: String, attrs: Attributes) {
            when (depth) {
                0 -> check(qName == "map" && attrs.length == 0)
                1 -> {
                    check(qName == "string" && attrs.length == 1 && attrs.getQName(0) == "name")
                    val name = attrs.getValue(0)
                    check(names.add(name)) { "Duplicate secure-store entry" }
                    currentKeyset = name.takeIf { it in KEYSETS }
                    value.setLength(0)
                }
                else -> error("Nested secure-store entry")
            }
            depth++
        }

        override fun characters(ch: CharArray, start: Int, length: Int) {
            if (depth == 1) check(ch.sliceArray(start until start + length).all { it.isWhitespace() })
            if (depth == 2 && currentKeyset != null) value.append(ch, start, length)
        }

        override fun endElement(uri: String?, localName: String?, qName: String?) {
            if (depth == 2 && currentKeyset != null) {
                check(value.isNotEmpty() && value.length % 2 == 0 && value.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' })
                found.add(checkNotNull(currentKeyset))
                currentKeyset = null
            }
            depth--
        }
    }
    SAXParserFactory.newInstance().newSAXParser().parse(InputSource(StringReader(xml)), handler)
    check(depth == 0 && found == KEYSETS) { "Existing secure-store keysets are unavailable" }
}
