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

## Dependency attributions

The Android shell includes the generated third-party attribution page at
`resources/resource_protocol/license.html`. It is produced from the complete resolved Cargo
graph, including transitive and target-specific crates, rather than from a hand-maintained
shortlist. The page covers the permissive licences used by the build, including MPL-2.0,
Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, zlib, BSL-1.0, Unicode-3.0,
CDLA-Permissive-2.0, OFL-1.1 and the Ubuntu Font Licence.

Regenerate it after changing `Cargo.lock` with:

```sh
cargo about generate etc/about.hbs > resources/resource_protocol/license.html
```

The generated page records each crate name, version, source repository and full applicable licence
text. Do not remove individual notices when updating or redistributing Antares.
