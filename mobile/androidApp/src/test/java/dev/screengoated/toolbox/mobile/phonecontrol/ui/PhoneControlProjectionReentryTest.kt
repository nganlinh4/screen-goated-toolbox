package dev.screengoated.toolbox.mobile.phonecontrol.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlProjectionReentryTest {
    @Test
    fun returnToAppIntentDoesNotStartActivation() {
        for (mode in PhoneControlCoordinatorMode.entries) {
            assertEquals(mode, phoneControlCoordinatorMode(mode.wireName))
        }
        assertEquals(PhoneControlCoordinatorMode.ACTIVATE, phoneControlCoordinatorMode(null))
    }

    @Test
    fun savedProjectionResultIsRetiredWhenItsProcessOwnerIsGone() {
        val prior = PhoneControlUserStepState()
        prior.projectionRequest = PhoneControlProjectionRequest.Activation("generation-a")
        val savedInFlight = prior.hasProjectionOwnership
        assertTrue(savedInFlight)
        assertFalse(retireUnownedRestoredProjection(savedInFlight, prior.hasProjectionOwnership))

        val restarted = PhoneControlUserStepState()
        assertTrue(prior.projectionLauncherKey != restarted.projectionLauncherKey)
        assertTrue(retireUnownedRestoredProjection(savedInFlight, restarted.hasProjectionOwnership))
        assertFalse(retireUnownedRestoredProjection(false, restarted.hasProjectionOwnership))

        prior.projectionRequest = PhoneControlProjectionRequest.Protected("opaque-owner", "generation-a")
        assertTrue(prior.hasProjectionOwnership)
        assertTrue(retireUnownedRestoredProjection(prior.hasProjectionOwnership, false))
    }

    @Test
    fun inFlightActivationKeepsItsRequestWhenResumeArrives() {
        val state = PhoneControlUserStepState()
        state.projectionRequest = PhoneControlProjectionRequest.Activation("generation-a")
        assertEquals(
            ProjectionReentryDisposition.DEFER,
            projectionReentryDisposition(
                state.projectionRequest, state.projectionSettling, ProjectionReentryKind.RESUME, false,
            ),
        )
        assertEquals(PhoneControlProjectionRequest.Activation("generation-a"), state.projectionRequest)
        assertEquals(
            ProjectionReentryDisposition.COALESCE,
            projectionReentryDisposition(
                state.projectionRequest, false, ProjectionReentryKind.ACTIVATE, true,
            ),
        )
        assertEquals(
            ProjectionReentryDisposition.DEFER,
            projectionReentryDisposition(
                state.projectionRequest, false, ProjectionReentryKind.ACTIVATE, false,
            ),
        )
    }

    @Test
    fun protectedConsentKeepsOwnerAndDefersDifferentRequests() {
        val state = PhoneControlUserStepState()
        val request = PhoneControlProjectionRequest.Protected("owner-a", "generation-a")
        state.projectionRequest = request
        assertEquals(
            ProjectionReentryDisposition.DEFER,
            projectionReentryDisposition(request, false, ProjectionReentryKind.ACTIVATE, true),
        )
        assertEquals(
            ProjectionReentryDisposition.COALESCE,
            projectionReentryDisposition(request, false, ProjectionReentryKind.RESUME, true),
        )
        // A resume for a replacement checkpoint cannot coalesce with the old dialog.
        assertEquals(
            ProjectionReentryDisposition.DEFER,
            projectionReentryDisposition(request, false, ProjectionReentryKind.RESUME, false),
        )
        assertEquals(request, state.projectionRequest)
    }

    @Test
    fun cancelPreemptsTheDialogAndSettlingDefersNewWork() {
        assertEquals(
            ProjectionReentryDisposition.CANCEL,
            projectionReentryDisposition(
                PhoneControlProjectionRequest.Activation("generation-a"), false, ProjectionReentryKind.CANCEL, false,
            ),
        )
        assertEquals(
            ProjectionReentryDisposition.DEFER,
            projectionReentryDisposition(null, true, ProjectionReentryKind.OTHER, false),
        )
        assertEquals(
            ProjectionReentryDisposition.PROCEED,
            projectionReentryDisposition(null, false, ProjectionReentryKind.OTHER, false),
        )
    }
}
