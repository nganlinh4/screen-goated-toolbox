package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileEntryAlreadyExists
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionLock
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

internal fun historyReceiptName(target: String) = ".history-$target.receipt"
internal fun historyStageName(target: String) = ".history-$target.stage"

internal data class HistoryDescriptorVersion(val identity: FileObjectId, val fingerprint: FileFingerprint)
internal fun TransactionFile.historyVersion(maximum: Int) = HistoryDescriptorVersion(identity, fingerprint(maximum))
internal fun historyDescriptorVersion(io: FileTransactionIo, route: HistoryDirectoryRoute, name: String, maximum: Int) =
    io.file(route.parent, name).use { it?.historyVersion(maximum) }

internal data class HistoryReceiptView(val target: HistoryDescriptorVersion?, val slot: HistoryDescriptorVersion?)

/** One fixed receipt and slot; unknown versions never grant reuse authority. */
internal class HistoryReceipt private constructor(
    private val io: FileTransactionIo,
    private val route: HistoryDirectoryRoute,
    private val targetName: String,
    private val maximum: Int,
    private val file: TransactionFile,
    private val lock: TransactionLock,
) : AutoCloseable {
    private val json = Json { encodeDefaults = true }
    private val name = historyReceiptName(targetName)
    private var proof = file.historyVersion(maximum)
    private lateinit var record: Record

    fun observe(durableRead: Boolean = false): HistoryReceiptView {
        validate()
        val actual = HistoryReceiptView(version(targetName), version(historyStageName(targetName)))
        val expected = HistoryReceiptView(record.target?.version(), record.slot?.version())
        val desired = record.desired?.fingerprint()
        val applied = record.phase == Phase.PREPARED && actual == HistoryReceiptView(expected.slot, expected.target)
        val known = when (record.phase) {
            Phase.SETTLED -> actual == expected
            Phase.CREATING -> actual.target == expected.target && actual.slot == null
            Phase.WRITING -> actual.target == expected.target &&
                (actual.slot == expected.slot || actual.slot == expected.slot?.copy(fingerprint = checkNotNull(desired)))
            Phase.PREPARED -> actual == expected || applied
        }
        if (!known) throw IOException("History ownership outcome is uncertain")
        if (durableRead && applied) {
            route.parent.sync()
            if (actual != HistoryReceiptView(version(targetName), version(historyStageName(targetName)))) {
                throw IOException("History recovery namespace changed")
            }
        }
        validate()
        return actual
    }

    fun creating(view: HistoryReceiptView, desired: FileFingerprint) = persist(Phase.CREATING, view, desired)
    fun writing(view: HistoryReceiptView, desired: FileFingerprint) = persist(Phase.WRITING, view, desired)
    fun prepared(view: HistoryReceiptView) = persist(Phase.PREPARED, view, checkNotNull(view.slot).fingerprint)
    fun settled(view: HistoryReceiptView) = persist(Phase.SETTLED, view, null)

    fun validate() {
        route.revalidate()
        file.requireSingleLink()
        if (file.historyVersion(maximum) != proof || version(name) != proof) {
            throw IOException("History ownership receipt changed")
        }
    }

    private fun version(name: String) = historyDescriptorVersion(io, route, name, maximum)

    private fun persist(phase: Phase, view: HistoryReceiptView, desired: FileFingerprint?) {
        validate()
        val next = Record(1, Id(file.identity.device, file.identity.inode), route.identities.map { Id(it.device, it.inode) },
            targetName, phase, view.target?.stored(), view.slot?.stored(), desired?.stored())
        checkRecord(next)
        val payload = json.encodeToString(next)
        val bytes = json.encodeToString(Envelope(payload, historyDigest(payload.toByteArray()))).toByteArray()
        if (bytes.size > maximum) throw IOException("History receipt exceeds its snapshot budget")
        file.writeSynced(bytes) { false }
        val written = HistoryDescriptorVersion(file.identity, FileFingerprint(bytes.size.toLong(), historyDigest(bytes)))
        if (file.historyVersion(maximum) != written || version(name) != written) {
            throw IOException("History ownership receipt changed")
        }
        route.revalidate()
        proof = written
        record = next
    }

    private fun load() {
        val bytes = file.readBounded(maximum)
        val text = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
        val envelope = json.decodeFromString<Envelope>(text)
        if (historyDigest(envelope.payload.toByteArray()) != envelope.sha256) throw IOException("History receipt is unreadable")
        val loaded = json.decodeFromString<Record>(envelope.payload)
        checkRecord(loaded)
        if (proof.fingerprint != FileFingerprint(bytes.size.toLong(), historyDigest(bytes))) {
            throw IOException("History ownership receipt changed")
        }
        record = loaded
        validate()
    }

    private fun checkRecord(value: Record) {
        if (value.version != 1 || value.self != Id(file.identity.device, file.identity.inode) ||
            value.ancestors != route.identities.map { Id(it.device, it.inode) } || value.targetName != targetName
        ) throw IOException("History ownership receipt does not match storage")
        for (fingerprint in listOfNotNull(value.target?.content, value.slot?.content, value.desired)) {
            if (fingerprint.bytes !in 0..maximum.toLong() || fingerprint.sha256.length != 64 ||
                fingerprint.sha256.any { it !in '0'..'9' && it !in 'a'..'f' }
            ) throw IOException("History receipt fingerprint is invalid")
        }
        if (value.target != null && value.slot != null && value.target.id == value.slot.id) {
            throw IOException("History target aliases its staging slot")
        }
        val valid = when (value.phase) {
            Phase.SETTLED -> value.desired == null
            Phase.CREATING -> value.slot == null && value.desired != null
            Phase.WRITING -> value.slot != null && value.desired != null
            Phase.PREPARED -> value.slot != null && value.desired == value.slot.content
        }
        if (!valid) throw IOException("History receipt phase is invalid")
    }

    override fun close() { try { lock.close() } finally { file.close() } }

    @Serializable private enum class Phase { SETTLED, CREATING, WRITING, PREPARED }
    @Serializable private data class Id(val device: Long, val inode: Long)
    @Serializable private data class Content(val bytes: Long, val sha256: String) {
        fun fingerprint() = FileFingerprint(bytes, sha256)
    }
    @Serializable private data class Owned(val id: Id, val content: Content) {
        fun version() = HistoryDescriptorVersion(FileObjectId(id.device, id.inode), content.fingerprint())
    }
    @Serializable private data class Record(val version: Int, val self: Id, val ancestors: List<Id>, val targetName: String,
        val phase: Phase, val target: Owned?, val slot: Owned?, val desired: Content?)
    @Serializable private data class Envelope(val payload: String, val sha256: String)

    private fun FileFingerprint.stored() = Content(bytes, sha256)
    private fun HistoryDescriptorVersion.stored() = Owned(Id(identity.device, identity.inode), fingerprint.stored())

    companion object {
        fun open(io: FileTransactionIo, route: HistoryDirectoryRoute, target: String, maximum: Int,
                 create: Boolean, expected: HistoryDescriptorVersion? = null): HistoryReceipt? {
            val name = historyReceiptName(target)
            var created = false
            var opened: TransactionFile? = if (create) io.writableFile(route.parent, name) else io.file(route.parent, name)
            if (opened == null && create) {
                if (io.file(route.parent, historyStageName(target)).use { it != null }) {
                    throw IOException("History staging ownership is unavailable")
                }
                try {
                    opened = io.file(route.parent, name, create = true)
                    created = true
                } catch (_: FileEntryAlreadyExists) { opened = io.writableFile(route.parent, name) }
            }
            val file = opened ?: return null
            var lease: TransactionLock? = null
            try {
                lease = file.lockExclusive()
                val receipt = HistoryReceipt(io, route, target, maximum, file, lease)
                if (created) {
                    receipt.settled(HistoryReceiptView(expected, null))
                    route.parent.sync()
                    receipt.validate()
                } else receipt.load()
                return receipt
            } catch (error: Throwable) {
                try { lease?.close() } catch (close: Throwable) { error.addSuppressed(close) }
                try { file.close() } catch (close: Throwable) { error.addSuppressed(close) }
                throw error
            }
        }
    }
}
