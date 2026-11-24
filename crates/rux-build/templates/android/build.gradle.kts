plugins {
    // AGP 9 compiles Kotlin itself (built-in Kotlin), so no Kotlin Gradle plugin is needed.
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
}
