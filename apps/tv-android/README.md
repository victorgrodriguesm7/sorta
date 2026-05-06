# Sorta TV (Android)

Read-only companion app to the Sorta desktop. Runs on a TV box that the
user has plugged the catalogued hard drive into; reads the on-disk
catalog (`sorta.db`, `manifest.json`, the `Movies/` and `Series/`
folders) and launches an external player on click.

## Target hardware

- Amlogic-class TV box, 32-bit Android 7.1.1 (API 25), Mali-450, 2 GB
  RAM, ~500 MB free at runtime.
- Native Kotlin + AndroidX Leanback. No Compose, no Flutter.
- `compileSdk = 35`, `targetSdk = 25`, `minSdk = 21`, `abiFilter =
  armeabi-v7a`.

## Build

From the repo root:

```bash
cd apps/tv-android
./gradlew test                  # JVM unit tests (TDD inner loop)
./gradlew :app:assembleDebug    # produces app/build/outputs/apk/debug/app-debug.apk
./gradlew :app:installDebug     # builds + sideloads to the connected adb device
```

Make sure your TV box is reachable first:

```bash
adb connect <box-ip>:5555
adb devices                     # should list the box as 'device'
```

If nothing else is connected, `installDebug` lands the APK directly on
the box. Otherwise pass `-PadbTarget=<serial>`.

## Running tests

JVM-only unit tests live under `app/src/test/java/`. They don't require
Gradle's Android instrumentation runner — `./gradlew test` runs them
in seconds.

Instrumented tests would live under `app/src/androidTest/java/`; we
don't have any yet.

## Source of truth

The on-disk format the app reads is documented in
[`docs/disk-format.md`](../../docs/disk-format.md) at the repo root.
That doc is the contract; this app's source code follows it but does
not redefine it. If you change behaviour around folder naming /
schema / manifest, update the doc.
