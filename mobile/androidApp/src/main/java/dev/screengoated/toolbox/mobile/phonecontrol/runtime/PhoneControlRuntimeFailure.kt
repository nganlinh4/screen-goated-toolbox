package dev.screengoated.toolbox.mobile.phonecontrol.runtime

internal class PhoneControlRuntimeFailure(
    val code: PhoneControlRuntimeCode,
    override val message: String,
    override val cause: Throwable? = null,
) : RuntimeException(message, cause)
