import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

android {
    namespace = "dev.sorta.tv"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.sorta.tv"
        minSdk = 21
        // Targeting API 25 (Android 7.1.1, the deployment box) means
        // we DON'T have to deal with Scoped Storage opt-in. Lint will
        // complain that this is below the Play Store minimum — fine,
        // we don't ship to Play.
        @Suppress("OldTargetApi")
        targetSdk = 25
        versionCode = 1
        versionName = "0.1.0"

        // Single ABI: the Amlogic / RK box runs a 32-bit Android image
        // even though the SoC is 64-bit-capable.
        ndk {
            abiFilters += "armeabi-v7a"
        }

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }


    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    lint {
        // The targetSdk-too-low lint comes from publishing rules we
        // don't care about for an unpublished sideload-only app.
        disable += setOf("OldTargetApi", "ExpiredTargetSdkVersion")
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.appcompat)
    implementation(libs.androidx.leanback)
    implementation(libs.androidx.recyclerview)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.documentfile)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.glide)

    testImplementation(libs.junit)
    testImplementation(libs.org.json)

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
