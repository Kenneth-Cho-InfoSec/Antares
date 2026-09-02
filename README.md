# Antares Engine

<p align="center">
  <img src="resources/antares-icon.png" alt="Antares Engine logo" width="220">
</p>

<p align="center">
  <strong>An experimental, privacy-minded Android browser engine for Solipsism Browser.</strong>
</p>

<p align="center">
  <a href="https://github.com/Kenneth-Cho-InfoSec/Antares/releases/latest"><img src="https://img.shields.io/github/v/release/Kenneth-Cho-InfoSec/Antares?label=latest%20release" alt="Latest release"></a>
  <a href="https://github.com/Kenneth-Cho-InfoSec/Antares/blob/main/LICENSE"><img src="https://img.shields.io/github/license/Kenneth-Cho-InfoSec/Antares" alt="License"></a>
  <a href="https://github.com/Kenneth-Cho-InfoSec/Solipsism"><img src="https://img.shields.io/badge/Solipsism-companion-6f42c1" alt="Solipsism companion"></a>
</p>

Antares 0.2.0 is the experimental in-house Android browser core used by Solipsism Browser. It provides an alternative to the system Android WebView through a native rendering path, with a focus on privacy, control, and a small host interface that can be embedded in an Android application.

Antares is derived from the open-source [Servo](https://github.com/servo/servo) project and is heavily customised for this use case. Servo is mentioned here as the upstream foundation; Antares development, packaging, Android integration, and release decisions are maintained in this repository.

## Current status

Antares is experimental software and is developed alongside the Solipsism host application. It is not a drop-in replacement for Android WebView. Compatibility, media playback, text input, accessibility, and complex web applications can vary by Android version and device.

The stable browsing fallback is Android WebView through [Solipsism Browser](https://github.com/Kenneth-Cho-InfoSec/Solipsism). Keep both packages installed when selecting Antares from Solipsism; the host application owns the browser interface and can switch back to WebView at any time.

## Install the companion core

Antares is published as a separate signed Android companion package so its native engine can be tested and updated independently, while the Solipsism source tree contains the host protocol and integration code.

1. Install Solipsism Browser from its [release page](https://github.com/Kenneth-Cho-InfoSec/Solipsism/releases).
2. Download the matching **Antares Engine** APK from the [Antares releases](https://github.com/Kenneth-Cho-InfoSec/Antares/releases).
3. Install both packages on the same Android device. The current release targets ARM64 devices running Android 13 or newer.
4. Open Solipsism and choose **Antares** in the browser-core chooser or Debug Settings.
5. To return to the stable core, select **Android WebView** in Solipsism.

Solipsism verifies that the companion package is present and trusted before binding to it. Do not install companion APKs from unofficial sources.

## What Antares provides

- Native page layout and rendering through the Antares engine.
- A small Android service that can be hosted by Solipsism.
- Cross-process text input with Unicode composition, deletion, Enter, and keyboard dismissal.
- Generic HTML text and search input sizing that honours the default character width and the `size` attribute.
- Android 17-safe surface ownership so the renderer does not steal the host keyboard connection.
- A focused Android packaging path for ARM64 release builds.

The host application remains responsible for browser chrome, tabs, permissions, downloads, bookmarks, settings, and user-facing core selection. Antares does not replace those Solipsism features.

## Android feature profile

The Antares Android package is built without WebGPU and WebXR. Their native
runtimes, graphics backends, and XR device integrations are therefore not
included in the companion APK, which keeps the package focused on ordinary
2D browsing and avoids advertising APIs that are not available in this
Android integration. WebGL remains available where supported by the renderer.

The upstream Servo feature switches are retained in the Cargo manifests for
desktop experiments and source compatibility, but they are not part of
Antares's default Android feature set. WebXR is also disabled in the packaged
preferences. Applications embedding Antares should treat WebGPU and WebXR as
unsupported and provide a normal WebGL or non-XR fallback.

## Known limitations

- Web compatibility is incomplete compared with mature Android browser engines.
- Some JavaScript-heavy applications may render partially or fail to respond to interaction.
- Media playback depends on the engine, Android media components, codecs, and the page implementation.
- CAPTCHA, authentication, DRM, WebRTC, downloads, and advanced storage behaviour may differ from WebView.
- Site isolation and permission enforcement depend on the host integration and Android platform facilities.
- The Android build currently supports ARM64 release packaging. Other ABIs require their own native build and validation.

When a page is incompatible, switch back to Android WebView from Solipsism. Please report a reproducible, generic test case rather than a private browsing history or personal data.

## Build from source

### Prerequisites

- Linux, macOS, or Windows development environment supported by the upstream build system.
- Rust toolchain managed by [rustup](https://rustup.rs/).
- [uv](https://docs.astral.sh/uv/) for the repository's Python tooling.
- Android SDK and NDK for Android builds.

The Android release configuration currently uses:

- Android platform 37
- Build tools 36.0.0
- NDK 28.2.13676358
- Minimum Android version 13

Set `ANDROID_SDK_ROOT` and `ANDROID_NDK_ROOT` before building Android targets. The NDK path should point to the installed `28.2.13676358` directory.

### Desktop development build

```shell
./mach bootstrap
./mach build
```

The desktop build is useful for engine development and layout tests. It is not the Android companion package.

### Android ARM64 release build

```shell
./mach build --android --profile production-stripped --no-package
./mach package --android --profile production-stripped
```

The Gradle companion application can also be assembled directly:

```shell
cd support/android/apk
./gradlew :servoapp:assembleArm64Release
```

The resulting APK is written under `support/android/apk/servoapp/build/outputs/apk/arm64Release/`.

## Testing

Run the repository checks before publishing a change:

```shell
./mach fmt --check
./mach test-tidy
./mach test-unit
./mach test-wpt
```

For a focused layout regression test:

```shell
./mach test-wpt tests/wpt/tests/html/rendering/widgets/input-auto-width-size.html
```

For Android validation, install the ARM64 debug or release package on a test device, exercise text and search fields, switch between Solipsism cores, and verify that keyboard ownership remains with the host application.

## Repository layout

- `components/` contains engine subsystems, including networking, script, layout, and media.
- `ports/servoshell/` contains platform entry points and Android engine plumbing.
- `support/android/apk/` contains the Antares companion Android package.
- `tests/wpt/tests/` contains Web Platform Test coverage.
- `docs/` contains Antares-specific development notes.
- `fdroid/` contains the F-Droid metadata for the companion package.

## Reporting problems

Open an issue with:

- Antares version and APK source.
- Android version, device model, and CPU architecture.
- Steps to reproduce using a minimal public test page where possible.
- Expected and actual behaviour.
- Relevant logcat output with personal data removed.

Do not include passwords, private URLs, tokens, or identifying browsing history.

## Licensing

Antares is distributed under the [Mozilla Public License 2.0](LICENSE). Components inherited from or adapted from upstream projects retain their applicable copyright notices and licence requirements. Review `LICENSE`, `LICENSE_WHATWG_SPECS`, and individual component notices before redistributing modified builds.

## Related project

[Solipsism Browser](https://github.com/Kenneth-Cho-InfoSec/Solipsism) is the Android browser application that hosts and selects the Antares core. Install both projects when testing the integrated experience.
