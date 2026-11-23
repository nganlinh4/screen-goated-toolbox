package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PublicResearchRelevanceTest {
    private val fixture = generateSequence(File(System.getProperty("user.dir") ?: error("working directory unavailable"))) { it.parentFile }
        .map { File(it, "parity-fixtures/phone-control/public-research-relevance.json") }
        .first(File::isFile)
        .readText()
        .let { Json.parseToJsonElement(it).jsonObject }

    @Test fun sharedTermsCountUnicodeScalars() {
        for (case in fixture.getValue("token_cases").jsonArray.map { it.jsonObject }) {
            val expected = case.getValue("terms").jsonArray.map { it.jsonPrimitive.content }.toSet()
            assertEquals(case.getValue("name").jsonPrimitive.content, expected, queryTerms(case.getValue("query").jsonPrimitive.content))
        }
    }

    @Test fun shortTermsKeepRelevantPassagesWithinBudget() {
        for (case in fixture.getValue("excerpt_cases").jsonArray.map { it.jsonObject }) {
            val line = case.getValue("line").jsonPrimitive.content
            val body = "neutral passage context\n".repeat(case.getValue("filler_before").jsonPrimitive.int) +
                line + "\n" + "neutral passage context\n".repeat(case.getValue("filler_after").jsonPrimitive.int)
            val limit = case.getValue("max_chars").jsonPrimitive.int
            val excerpt = relevantExcerpt(body, case.getValue("query").jsonPrimitive.content, limit)
            assertTrue(case.getValue("name").jsonPrimitive.content, excerpt.contains(line))
            assertTrue(excerpt.length <= limit)
        }
    }

    @Test fun emptyAndUnmatchedTermsStayWithinBudget() {
        val body = "neutral passage context\n".repeat(100)
        for (limit in listOf(0, 1, 12, 80)) {
            assertTrue(relevantExcerpt(body, "x", limit).length <= limit)
            assertTrue(relevantExcerpt(body, "unmatched", limit).length <= limit)
        }
    }

    @Test fun sharedShortTermsMatchAtLexicalStarts() {
        for (case in fixture.getValue("match_cases").jsonArray.map { it.jsonObject }) {
            val actual = researchMatchOffset(case.getValue("term").jsonPrimitive.content, case.getValue("text").jsonPrimitive.content.lowercase()) != null
            assertEquals(case.toString(), case.getValue("matches").jsonPrimitive.content.toBooleanStrict(), actual)
        }
    }

    @Test fun scalarBudgetsAndLongLinesPreserveEvidence() {
        for (case in fixture.getValue("scalar_budget_cases").jsonArray.map { it.jsonObject }) {
            val actual = relevantExcerpt(case.getValue("text").jsonPrimitive.content, case.getValue("query").jsonPrimitive.content, case.getValue("max_chars").jsonPrimitive.int)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("expected").jsonPrimitive.content, actual)
        }
        for (case in fixture.getValue("long_line_cases").jsonArray.map { it.jsonObject }) {
            val expected = case.getValue("match").jsonPrimitive.content
            val line = case.getValue("prefix").jsonPrimitive.content.repeat(case.getValue("prefix_repetitions").jsonPrimitive.int) + expected + case.getValue("suffix").jsonPrimitive.content
            val limit = case.getValue("max_chars").jsonPrimitive.int
            val actual = relevantExcerpt(line, case.getValue("query").jsonPrimitive.content, limit)
            assertTrue(actual, actual.contains(expected))
            assertTrue(actual.codePointCount(0, actual.length) <= limit)
        }
        for (case in fixture.getValue("ranking_cases").jsonArray.map { it.jsonObject }) {
            val distractor = case.getValue("distractor").jsonPrimitive.content + "\n"
            val expected = case.getValue("match").jsonPrimitive.content
            val body = distractor.repeat(case.getValue("distractor_repetitions").jsonPrimitive.int) + expected + "\n" + distractor.repeat(40)
            val actual = relevantExcerpt(body, case.getValue("query").jsonPrimitive.content, case.getValue("max_chars").jsonPrimitive.int)
            assertTrue(actual, actual.contains(expected))
        }
    }
}
