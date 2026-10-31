plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// CI passes -PversionCode=<run number> so each build installs over the last one.
val buildNumber = (project.findProperty("versionCode") as String?)?.toIntOrNull() ?: 1

android {
    namespace = "com.hammerjones.budget"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.hammerjones.budget"
        minSdk = 26
        targetSdk = 34
        versionCode = buildNumber
        versionName = "0.2.$buildNumber"
        ndk { abiFilters += "arm64-v8a" }
    }

    signingConfigs {
        create("release") {
            storeFile = file(System.getenv("BUDGET_KEYSTORE") ?: "budget.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    // The Rust library (libbudget_app.so) is copied into src/main/jniLibs by CI.
    packaging { jniLibs { useLegacyPackaging = true } }

    lint {
        checkReleaseBuilds = false
        abortOnError = false
    }
}
