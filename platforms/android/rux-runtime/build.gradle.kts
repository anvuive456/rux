plugins {
    id("com.android.library")
}

android {
    namespace = "dev.rux.runtime"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // Material 3: the base theme (`Theme.Rux`) and the views RuxHost creates. `api` so apps
    // can use and extend the theme in their manifest.
    api("com.google.android.material:material:1.14.0")
}
