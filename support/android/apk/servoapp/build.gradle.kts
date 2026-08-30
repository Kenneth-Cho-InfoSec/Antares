import java.util.regex.Pattern

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose)
}

android {
    compileSdk = 37
    buildToolsVersion = "36.0.0"

    namespace = "com.krystelligence.antares"

    defaultConfig {
        applicationId = "com.krystelligence.antares"
        // Antares embeds its renderer through SurfaceControlViewHost and is intentionally offered
        // by Solipsism only on Android 13 and newer.
        minSdk = 33
        targetSdk = 34
        versionCode = 4
        versionName = "0.2.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    buildFeatures {
        aidl = true
        buildConfig = true
    }

    // Release artifacts are distributed for ARM64 phones only. Debug variants
    // for emulator and legacy ABIs remain available for development.
    packaging {
        jniLibs.useLegacyPackaging = true
        resources.excludes += setOf("**/*.prof", "**/*.profraw", "**/*.so.dbg", "**/symbols/**")
    }

    val signingKeyInfo = getSigningKeyInfo()

    if (signingKeyInfo != null) {
        signingConfigs {
            register("release") {
                storeFile = signingKeyInfo["storeFile"] as File
                storePassword = signingKeyInfo["storePassword"] as String
                keyAlias = signingKeyInfo["keyAlias"] as String
                keyPassword = signingKeyInfo["keyPassword"] as String
            }
        }
    }

    buildTypes {
        debug {
        }

        release {
            signingConfig =
                signingConfigs.getByName(if (signingKeyInfo != null) "release" else "debug")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }

        // Custom build types

        val debug = getByName("debug")
        val release = getByName("release")


        register("armv7Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("armv7"))
            }
        }
        register("armv7Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("armv7"))
            }
        }
        register("arm64Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("arm64"))
            }
        }
        register("arm64Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("arm64"))
            }
        }
        register("x86Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("x86"))
            }
        }
        register("x86Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("x86"))
            }
        }
        register("x64Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("x64"))
            }
        }
        register("x64Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("x64"))
            }
        }
    }
}

// Ignore default "debug" and "release" build types
androidComponents {
    beforeVariants {
        if (it.buildType == "release" || it.buildType == "debug") {
            it.enable = false
        }
        if (it.buildType?.endsWith("Release") == true && it.name != "arm64Release") {
            it.enable = false
        }
    }
}

project.afterEvaluate {
    android.applicationVariants.forEach { variant ->
        val pattern = Pattern.compile("^([\\w\\d]+)(Debug|Release)")
        val matcher = pattern.matcher(variant.name)
        if (!matcher.find()) {
            throw GradleException("Invalid variant name for output: " + variant.name)
        }
        val arch = matcher.group(1)
        val debug = variant.name.contains("Debug")
        val finalFolder = getTargetDir(debug, arch)
        val finalFile = File(finalFolder, "servoapp.apk")
        variant.outputs.forEach { output ->
            val copyAndRenameAPKTask =
                project.task<Copy>("copyAndRename${variant.name.capitalize()}APK") {
                    from(output.outputFile.parent)
                    into(finalFolder)
                    include(output.outputFile.name)
                    rename(output.outputFile.name, finalFile.name)
                }
            variant.assembleProvider.get().finalizedBy(copyAndRenameAPKTask)
        }
    }
}

dependencies {
    if (findProject(":servoview-local") != null) {
        implementation(project(":servoview-local"))
    } else {
        implementation(project(":servoview"))
    }
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.material3.compose)
    implementation(libs.androidx.material3.compose.adaptive)
    implementation(libs.androidx.preference)
}
