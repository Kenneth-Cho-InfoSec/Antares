/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
package com.krystelligence.antares

import android.os.Bundle
import android.os.Build
import android.widget.TextView
import androidx.activity.ComponentActivity
import com.krystelligence.antares.engine.AntaresProtocol

class AntaresDiagnosticsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_antares_diagnostics)
        findViewById<TextView>(R.id.antares_status).text = getString(
            R.string.antares_diagnostics_status,
            BuildConfig.VERSION_NAME,
            AntaresProtocol.VERSION,
            Build.SUPPORTED_ABIS.joinToString(),
        )
    }
}
