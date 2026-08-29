/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
package com.krystelligence.antares.engine

internal object AntaresProtocol {
    const val VERSION = 14
    const val CLIENT_PACKAGE = "com.krystelligence.solipsism"
    const val KEY_INITIAL_URL = "initial_url"
    const val KEY_EXPERIMENTAL = "experimental"
    const val KEY_USER_AGENT = "user_agent"
    const val KEY_THEME = "theme"
    const val THEME_LIGHT = 0
    const val THEME_DARK = 1
    const val KEY_SURFACE_PACKAGE = "surface_package"
    const val KEY_ERROR = "error"
    const val KEY_HOST_TOKEN = "host_token"
    const val KEY_INPUT_TRANSFER_TOKEN = "input_transfer_token"
    const val KEY_CAPABILITIES = "capabilities"
    const val KEY_MEDIA_PAGE_URL = "media_page_url"
    const val KEY_MEDIA_DIRECT_SOURCE = "media_direct_source"
    const val KEY_MEDIA_RENEWAL_REQUEST = "media_renewal_request"
    const val KEY_MEDIA_COOKIES = "media_cookies"
    const val KEY_MEDIA_TITLE = "media_title"

    val capabilities = arrayOf(
        "navigation",
        "history_navigation",
        "remote_surface",
        "forwarded_touch_input",
        "basic_web_platform",
        "custom_user_agent",
        "preferred_colour_scheme",
        "content_blocking",
        "android_media_bridge",
        "coordinate_probe",
        "host_input_method",
    )
}
