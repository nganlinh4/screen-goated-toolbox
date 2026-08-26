package dev.screengoated.toolbox.mobile.baselineprofile

import androidx.benchmark.macro.BaselineProfileMode
import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.FrameTimingMetric
import androidx.benchmark.macro.MemoryUsageMetric
import androidx.benchmark.macro.StartupMode
import androidx.benchmark.macro.StartupTimingMetric
import androidx.benchmark.macro.ExperimentalMetricApi
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@OptIn(ExperimentalMetricApi::class)
class StartupBenchmark {
    @get:Rule
    val rule = MacrobenchmarkRule()

    @Test
    fun coldStartupWithoutCompilation() = measure(CompilationMode.None())

    @Test
    fun coldStartupWithBaselineProfile() = measure(
        CompilationMode.Partial(
            baselineProfileMode = BaselineProfileMode.Require,
        ),
    )

    private fun measure(compilationMode: CompilationMode) {
        rule.measureRepeated(
            packageName = TARGET_PACKAGE,
            metrics = listOf(
                StartupTimingMetric(),
                FrameTimingMetric(),
                MemoryUsageMetric(MemoryUsageMetric.Mode.Max),
            ),
            compilationMode = compilationMode,
            startupMode = StartupMode.COLD,
            iterations = 5,
            setupBlock = { pressHome() },
        ) {
            runCriticalUserJourney()
        }
    }
}
