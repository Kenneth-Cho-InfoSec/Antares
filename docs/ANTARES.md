# Antares Engine

Antares is the experimental Servo-based secondary browser core for Solipsism Browser. It is a
continuation fork of Servo's Android shell and view integration, packaged as a separate F-Droid
application so people who select Android WebView do not pay the storage cost of the native engine.

The browser UI, tab list, privacy policy, and feature controls remain in Solipsism. Antares runs in
its own process and exposes a small, versioned AIDL API. Web content is embedded back into
Solipsism with Android's `SurfaceControlViewHost` API.

Internal `org.servo.servoview` names and `libservoshell.so` are intentionally retained at the JNI
boundary. They are implementation details required for upstream compatibility and do not change
the Antares product or Android package identity.

Antares remains Mozilla Public License 2.0 software. Servo copyright and licence notices are
retained throughout the fork.
