plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.dualaux"
    compileSdk = 36
    defaultConfig {
        applicationId = "app.dualaux"
        minSdk = 29  // AudioPlaybackCapture
        targetSdk = 34
        versionCode = 3
        versionName = "0.2.0"
    }
    // Release signing reads a local keystore from ~/.gradle/gradle.properties; without it the release APK is unsigned.
    val ks = findProperty("DUALAUX_KEYSTORE") as String?
    signingConfigs {
        if (ks != null) create("release") {
            storeFile = file(ks)
            storePassword = findProperty("DUALAUX_STORE_PASS") as String
            keyAlias = "dualaux"
            keyPassword = findProperty("DUALAUX_STORE_PASS") as String
        }
    }
    buildTypes {
        release {
            if (ks != null) signingConfig = signingConfigs.getByName("release")
        }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
}
