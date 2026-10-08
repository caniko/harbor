# Android APK packages

Android SDK, APK, and Maven-cache helpers live in
[harbor-android](https://github.com/caniko/harbor-android).

`harbor-rs.lib` re-exports `mkAndroidSdk`, `mkAndroidDevShell`, `mkAndroidApk`,
`mkAndroidApkDevBuilder`, `mkAndroidFlavorTable`, and `findLocalMavenCache` for
one migration release. New consumers should take `harbor-android` directly.

Keep `mkToolchain` here and pass `rustToolchain` into the Android helpers.
`mkGradlePackage` and `fetchMavenCache` also stay in harbor-rs.
