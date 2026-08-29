package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Test

class CreationRefinementContractTest {
    @Test
    fun `refinement variants share stable public capabilities`() {
        assertEquals("separate_parts", CreationContract.refinementCapability("separate_simple"))
        assertEquals("optimize_quad", CreationContract.refinementCapability("optimize_quad"))
        assertEquals("add_materials", CreationContract.refinementCapability("materials"))
        assertEquals("generate_pbr", CreationContract.refinementCapability("pbr"))
        assertEquals("animate", CreationContract.refinementCapability("animate_walk"))
    }
}
