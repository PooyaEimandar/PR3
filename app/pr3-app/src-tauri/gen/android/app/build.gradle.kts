import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
// A CI build's version (script/version.py, .github/workflows/app.yml): the name, such
// as 2026.10.27.30184-dev+fd9a2875dc06, and the code, which grows with every commit so
// each build installs over the last. A local build takes Tauri's, from Cargo.toml.
val ciVersionCode = System.getenv("PR3_ANDROID_VERSION_CODE")?.let { value ->
    value.toIntOrNull()?.takeIf { it in 1..2_100_000_000 }
        ?: throw GradleException("PR3_ANDROID_VERSION_CODE must be a Play version code")
}
val ciVersionName = System.getenv("PR3_ANDROID_VERSION_NAME")?.also { value ->
    if (!Regex("[0-9A-Za-z.+-]{1,64}").matches(value)) {
        throw GradleException("PR3_ANDROID_VERSION_NAME must be a plain version name")
    }
}

android {
    compileSdk = 36
    namespace = "ai.pooya.pr3"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "ai.pooya.pr3"
        minSdk = 24
        targetSdk = 36
        versionCode = ciVersionCode
            ?: tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = ciVersionName
            ?: tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // BiometricPrompt for approving actions with the Keystore key (VaultPlugin.kt).
    implementation("androidx.biometric:biometric:1.1.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")