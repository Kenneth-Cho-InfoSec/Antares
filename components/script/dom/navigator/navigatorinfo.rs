/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::dom::bindings::str::DOMString;

#[expect(non_snake_case)]
pub(crate) fn Product() -> DOMString {
    DOMString::from("Gecko")
}

#[expect(non_snake_case)]
pub(crate) fn ProductSub(user_agent: &str) -> DOMString {
    DOMString::from(if is_chromium_identity(user_agent) {
        "20030107"
    } else {
        "20100101"
    })
}

#[expect(non_snake_case)]
pub(crate) fn Vendor(user_agent: &str) -> DOMString {
    DOMString::from(if is_chromium_identity(user_agent) {
        "Google Inc."
    } else {
        ""
    })
}

#[expect(non_snake_case)]
pub(crate) fn VendorSub() -> DOMString {
    DOMString::from("")
}

#[expect(non_snake_case)]
pub(crate) fn TaintEnabled() -> bool {
    false
}

#[expect(non_snake_case)]
pub(crate) fn AppName() -> DOMString {
    DOMString::from("Netscape") // Like Gecko/Webkit
}

#[expect(non_snake_case)]
pub(crate) fn AppCodeName() -> DOMString {
    DOMString::from("Mozilla")
}

#[expect(non_snake_case)]
#[cfg(target_os = "windows")]
pub(crate) fn Platform(_user_agent: &str) -> DOMString {
    DOMString::from("Win32")
}

#[expect(non_snake_case)]
#[cfg(any(target_os = "android", target_os = "linux", target_os = "freebsd"))]
pub(crate) fn Platform(user_agent: &str) -> DOMString {
    DOMString::from(
        if cfg!(target_os = "android") && is_chromium_identity(user_agent) {
            "Linux armv81"
        } else {
            "Linux"
        },
    )
}

#[expect(non_snake_case)]
#[cfg(target_os = "macos")]
pub(crate) fn Platform(_user_agent: &str) -> DOMString {
    DOMString::from("Mac")
}

#[expect(non_snake_case)]
#[cfg(target_os = "ios")]
pub(crate) fn Platform(_user_agent: &str) -> DOMString {
    DOMString::from("iOS")
}

#[expect(non_snake_case)]
pub(crate) fn UserAgent(user_agent: &str) -> DOMString {
    DOMString::from(user_agent)
}

#[expect(non_snake_case)]
pub(crate) fn AppVersion(user_agent: &str) -> DOMString {
    DOMString::from(
        user_agent
            .strip_prefix("Mozilla/")
            .filter(|_| is_chromium_identity(user_agent))
            .unwrap_or("4.0"),
    )
}

fn is_chromium_identity(user_agent: &str) -> bool {
    user_agent.contains("AppleWebKit/")
        && user_agent.contains("Chrome/")
        && user_agent.contains("Safari/")
}

#[expect(non_snake_case)]
pub(crate) fn Language() -> DOMString {
    DOMString::from(net_traits::get_current_locale().0.clone())
}
