# utterance (Android)

Utterance presented as a native-feeling app: a single full-screen **WebView**, no
address bar, no tabs, a home-screen icon. It avoids browser chrome while showing
the UI exactly as designed (the system WebView is Chromium, so it renders like
Chrome).

The site is publicly resolvable but **behind a Nextcloud sign-in**. The WebView
keeps the session cookie, so it is a **one-time login**.

## The microphone

This is the only wrapper in the fleet that needs one, and it is the only thing here
that is not a copy of a sibling. Voice is what the app takes as input, so
`navigator.mediaDevices.getUserMedia` has to work inside the WebView — and a
WebView **denies it by default, silently**: the promise never settles, so the page
cannot tell a refusal from a slow microphone and shows neither audio nor an error.

There are two gates and both must open:

1. **Android's**, `RECORD_AUDIO`, declared in the manifest and requested at runtime
   the first time the page asks.
2. **The WebView's**, a `PermissionRequest` delivered to the chrome client, granted
   in `MainActivity` only once Android's has been granted.

An unanswered `PermissionRequest` is the failure mode worth knowing: it leaves
`getUserMedia` pending forever. Every path out of the handler therefore ends in an
explicit `grant` or `deny`, including the one where the user refuses.

The request is also checked against the page's own origin. The shell confines
*navigation*, but an iframe is not a navigation, so the origin is re-checked where
the microphone is actually handed out.

This lives in the app rather than in `org.xinutec:shell` because the shell's
contract puts permission prompts with the app. If a second voice app appears,
extract it then — the way the shell itself was extracted from eight copies rather
than designed up front.

## What else it does

Nothing: everything a wrapper does belongs to the shell (see
`~/Code/ui-harness/android`). Loads `https://utterance.xinutec.org/` — hardcoded,
this app is single-purpose — with `allowedHosts` naming the app **and
`dash.xinutec.org`**, without which the OAuth round-trip is ejected to the browser
and the app can never sign in. The page's console is mirrored to logcat under the
`utterance` tag, because the audio graph's own diagnostics are the only account of
what it did.

Runs on any Android 8+ (minSdk 26) device.

## Build & install

No toolchain lives in this repo — it borrows the recall project's `android` nix dev
shell (JDK 17 + Android SDK; the Gradle wrapper pins Gradle). `deploy.sh` does
both, and keys on the device *model* rather than an IP, because DHCP drifts and a
bare `adb install` can hit the wrong connected phone:

```sh
cd android
nix develop ~/Code/recall#android --command ./deploy.sh
```

It tries the VPN address (`10.100.0.12:5555`, the stable one) before the LAN lease.
To build without installing:

```sh
nix develop ~/Code/recall#android --command ./gradlew :app:assembleDebug
# → app/build/outputs/apk/debug/app-debug.apk
```

The APK is signed with the auto-generated debug key — fine for sideloading, the
only distribution path.

## Layout

```
android/
├── app/
│   ├── build.gradle.kts                                   # android app module, no Compose/AppCompat
│   └── src/main/
│       ├── AndroidManifest.xml                            # INTERNET + RECORD_AUDIO; one launcher activity
│       ├── kotlin/org/xinutec/utterance/MainActivity.kt   # url, login hop, and the microphone
│       └── res/                                           # launcher icon (the web mark), theme, strings
├── build.gradle.kts · settings.gradle.kts · gradle/        # project scaffolding
└── gradlew                                                # borrows ~/Code/recall#android for the SDK
```

The launcher icon is the web app's own `frontend/public/favicon.svg` — a pitch
contour — ported to a vector drawable; see the comment in
`res/drawable/ic_launcher_foreground.xml` for what changes in the port.
