package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationStagingSealTest {
    @Test
    fun deliveryRejectsBytesThatChangedAfterValidation() {
        val validated = "a".repeat(64)
        assertTrue(creationStagingDigestUnchanged(validated, validated.uppercase()))
        assertFalse(creationStagingDigestUnchanged(validated, "b".repeat(64)))
        assertTrue(creationStagingDigestUnchanged(null, "b".repeat(64)))
    }

    @Test
    fun prepareHashesBeforeValidatingAndBeginComparesBeforeRecording() {
        val prepare = source("CreationJobCompletion.kt")
            .substringAfter("fun prepare(")
            .substringBefore("\n    fun completePublished(")
        val hashed = prepare.indexOf("files.sha256(staging.absolutePath)")
        assertTrue(hashed >= 0)
        assertTrue(hashed < prepare.indexOf("validateGlb(staging)"))
        assertTrue(hashed < prepare.indexOf("validateSvg(staging)"))
        assertTrue(prepare.contains("validatedSha256 = validatedSha256"))

        val begin = source("CreationDeliveryTransaction.kt")
            .substringAfter("fun begin(")
            .substringBefore("\n    fun publish(")
        val compared = begin.indexOf("creationStagingDigestUnchanged(prepared.validatedSha256, digest)")
        assertTrue(compared >= 0)
        assertTrue(compared < begin.indexOf("write(records)"))
    }

    private fun source(name: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/$name",
        ).readText()
    }
}
