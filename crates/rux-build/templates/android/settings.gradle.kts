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

// Shared rux Kotlin runtime, from the `rux` crate cargo resolved for the app: its location
// is written to rux.properties (gitignored) by `rux build android`.
val ruxRuntimeDir = java.util.Properties().run {
    val file = file("rux.properties")
    if (!file.exists()) {
        throw GradleException("${file.path} not found: run `rux build android` first.")
    }
    file.inputStream().use { load(it) }
    getProperty("rux.runtimeDir")
        ?: throw GradleException("rux.runtimeDir missing in ${file.path}: run `rux build android`.")
}
include(":rux-runtime")
project(":rux-runtime").projectDir = file(ruxRuntimeDir)
