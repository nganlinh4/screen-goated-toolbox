package dev.screengoated.toolbox.mobile.phonecontrol.result

/** Structural evidence from an input-injection boundary. */
internal data class InputInjectionEvidence(
    val requested: Long,
    val inserted: Long,
    val fullyInserted: Boolean,
) {
    init {
        require(requested >= 0) { "requested input count must be non-negative" }
        require(inserted >= 0) { "inserted input count must be non-negative" }
        require(inserted <= requested) { "inserted input count cannot exceed requested count" }
    }
}

/**
 * Effect certainty is independent from a tool's ordinary success flag.
 * Stronger evidence wins in this order: verified, may-have-occurred,
 * proven-no-effect, then unknown.
 */
internal enum class EffectCertainty(val wireName: String) {
    VERIFIED("verified"),
    MAY_HAVE_OCCURRED("may_have_occurred"),
    PROVEN_NO_EFFECT("proven_no_effect"),
    UNKNOWN("unknown"),
    ;

    val effectMayHaveOccurred: Boolean?
        get() = when (this) {
            VERIFIED, MAY_HAVE_OCCURRED -> true
            PROVEN_NO_EFFECT -> false
            UNKNOWN -> null
        }

    val effectVerified: Boolean
        get() = this == VERIFIED

    val executed: Boolean?
        get() = when (this) {
            VERIFIED -> true
            PROVEN_NO_EFFECT -> false
            MAY_HAVE_OCCURRED, UNKNOWN -> null
        }

    fun afterDispatch(mutating: Boolean): EffectCertainty =
        if (this == UNKNOWN && mutating) MAY_HAVE_OCCURRED else this

    companion object {
        fun fromSignals(
            effectVerified: Boolean = false,
            effectMayHaveOccurred: Boolean? = null,
            dispatchOk: Boolean? = null,
            executed: Boolean? = null,
            inputInjection: InputInjectionEvidence? = null,
        ): EffectCertainty {
            if (effectVerified) return VERIFIED

            val injectionMayHaveOccurred = inputInjection?.let {
                it.fullyInserted || it.inserted > 0
            } == true
            val mayHaveOccurred = effectMayHaveOccurred == true ||
                dispatchOk == true ||
                executed == true ||
                injectionMayHaveOccurred
            if (mayHaveOccurred) return MAY_HAVE_OCCURRED

            val injectionProvesNoEffect = inputInjection?.let {
                it.requested > 0 && !it.fullyInserted && it.inserted == 0L
            } == true
            val provenNoEffect = effectMayHaveOccurred == false ||
                dispatchOk == false ||
                executed == false ||
                injectionProvesNoEffect
            return if (provenNoEffect) PROVEN_NO_EFFECT else UNKNOWN
        }
    }
}
