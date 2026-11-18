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

rootProject.name = "{{project_name}}"

include(":app")

// Shared rux Kotlin runtime.
include(":rux-runtime")
project(":rux-runtime").projectDir = file("{{android_runtime}}")
