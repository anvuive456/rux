import java.util.Properties

plugins {
    id("com.android.application")
}

// Application id (`app-id` in [package.metadata.rux]) and version name / code
// (`version = "x.y.z+n"`) come from the app's Cargo.toml; `rux build android` writes
// them to ../rux.properties.
val ruxProperties = Properties().apply {
    val file = rootProject.file("rux.properties")
    if (!file.exists()) {
        throw GradleException("${file.path} not found: run `rux build android` first.")
    }
    file.inputStream().use { load(it) }
}

android {
    // Kotlin/R package of the app's code — independent of applicationId.
    namespace = "dev.rux.examples.counter"
    compileSdk = 35

    defaultConfig {
        applicationId = ruxProperties.getProperty("rux.applicationId")
        minSdk = 24
        targetSdk = 35
        versionCode = ruxProperties.getProperty("rux.versionCode").toInt()
        versionName = ruxProperties.getProperty("rux.versionName")

        ndk {
            // Must match the ABIs built with `rux build android --abi ...`.
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    // `rux build android` copies libcounter.so into src/main/jniLibs/<abi>/.

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}


dependencies {
    implementation(project(":rux-runtime"))
}
