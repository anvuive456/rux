plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.rux.examples.counter"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.rux.examples.counter"
        minSdk = 24
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

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

kotlin {
    jvmToolchain(17)
}

dependencies {
    implementation(project(":rux-runtime"))
}
