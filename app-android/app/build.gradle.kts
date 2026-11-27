plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "dev.drawkit.android"
    compileSdk = 37
    // Compose 1.13 requires API 37.1 or later, not just 37.
    compileSdkMinor = 1
    defaultConfig {
        applicationId = "dev.drawkit.android"
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        // A release stamps the tag through -PdrawkitVersion; a local build keeps the placeholder.
        versionName = (project.findProperty("drawkitVersion") as String?) ?: "0.0.1"
        ndk {
            // Only the two ABIs the Rust library is built for; the rest would install and then fail to
            // load the native library.
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    // GitHub renders a missing secret as an empty string, so blank has to mean absent here.
    val alphaStore = System.getenv("DRAWKIT_KEYSTORE")?.takeIf { it.isNotBlank() }

    signingConfigs {
        create("alpha") {
            val store = alphaStore ?: return@create
            storeFile = file(store)
            storePassword = System.getenv("DRAWKIT_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("DRAWKIT_KEY_ALIAS")
            keyPassword = System.getenv("DRAWKIT_KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // A release build is not debuggable, which is what an alpha wants to hand out. Sign it
            // with a real key when the environment carries one; otherwise the debug key, which
            // installs but cannot be uploaded to a store.
            signingConfig = if (alphaStore != null) {
                signingConfigs.getByName("alpha")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
    buildFeatures {
        compose = true
        aidl = false
        buildConfig = false
        shaders = false
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

kotlin {
    // The build runs on whatever JDK Gradle uses (CI pins 21, a developer's Android Studio may be
    // newer); only the bytecode target is fixed, so no toolchain has to be downloaded.
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21
    }
}

dependencies {
    val composeBom = platform(libs.androidx.compose.bom)
    implementation(composeBom)
    androidTestImplementation(composeBom)

    // Core Android dependencies
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.core.splashscreen)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.activity.compose)

    // Arch Components
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)

    // Compose
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material.icons.extended)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    // Tooling
    debugImplementation(libs.androidx.compose.ui.tooling)
    // Instrumented tests
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    debugImplementation(libs.androidx.compose.ui.test.manifest)

    // Local tests: jUnit, coroutines, Android runner
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)

    // Instrumented tests: jUnit rules and runners
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.espresso.core)
}
