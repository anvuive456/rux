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

rootProject.name = "RuxCounter"

include(":app")

// Shared rux Kotlin runtime.
include(":rux-runtime")
project(":rux-runtime").projectDir = file("../../../platforms/android/rux-runtime")
