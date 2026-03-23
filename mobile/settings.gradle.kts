pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "sgt-mobile"

includeBuild("../../youtubedl-android") {
    dependencySubstitution {
        substitute(module("io.github.junkfood02.youtubedl-android:library")).using(project(":library"))
        substitute(module("io.github.junkfood02.youtubedl-android:ffmpeg")).using(project(":ffmpeg"))
        substitute(module("io.github.junkfood02.youtubedl-android:common")).using(project(":common"))
    }
}

include(":androidApp")
include(":shared")

