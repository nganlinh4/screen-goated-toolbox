package dev.screengoated.toolbox.mobile.phonecontrol.provider

import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlOperationId
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo

internal fun testFileOwner() = PhoneControlEffectOwner(PhoneControlOperationId(1, 1, "file-transaction-test"))
internal fun testFileProvider(artifacts: PhoneControlArtifactStore) = AndroidFileProvider(artifacts, NioTestFileTransactionIo())
internal fun testFileProvider(find: (String) -> PhoneControlArtifact?) = AndroidFileProvider(find, NioTestFileTransactionIo())
