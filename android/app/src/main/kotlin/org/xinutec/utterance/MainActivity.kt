package org.xinutec.utterance

import android.Manifest
import android.content.pm.PackageManager
import android.webkit.PermissionRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import org.xinutec.shell.ShellConfig
import org.xinutec.shell.WebShellActivity

/**
 * Utterance — voice into music, an Angular app served at [UTTERANCE_URL], in the
 * fleet's shared [WebShellActivity]. Publicly resolvable but behind a Nextcloud
 * sign-in; the WebView keeps the session cookie, so it is a one-time login.
 *
 * The one thing no other wrapper needs: **a microphone**. `getUserMedia` inside a
 * WebView is denied by default and silently — the page's `navigator.mediaDevices`
 * call simply never resolves — so the grant has to be plumbed through twice, once
 * for the app and once for the page. This lives here rather than in the shell
 * because the shell's contract puts permission prompts with the app; if a second
 * voice app appears, extract it then, the way the shell itself was extracted from
 * eight copies rather than designed up front.
 */
class MainActivity : WebShellActivity() {
    override val shell =
        ShellConfig(
            url = UTTERANCE_URL,
            // The app plus the Nextcloud login hop. Without the second the OAuth
            // round-trip is ejected to the browser and the app can never sign in.
            allowedHosts = setOf("utterance.xinutec.org", NC_HOST),
            // The page's own diagnostics are the only account of what the audio
            // graph did; without a tag they never leave the WebView.
            consoleTag = "utterance",
        )

    /**
     * The page's pending microphone request, held while Android asks the user.
     *
     * There can only be one: the WebView will not issue a second while the first
     * is unanswered. Cleared on every path out, because a request left here would
     * be granted by the *next* answer — to a different question.
     */
    private var pendingMic: PermissionRequest? = null

    private val askAndroid =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            val request = pendingMic ?: return@registerForActivityResult
            pendingMic = null
            // Deny explicitly rather than dropping it. An unanswered PermissionRequest
            // leaves getUserMedia pending forever, which the page cannot tell apart
            // from a microphone that is simply slow, so it shows no error and no audio.
            if (granted) {
                request.grant(arrayOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE))
            } else {
                request.deny()
            }
        }

    override fun createWebChromeClient() = UtteranceWebChromeClient()

    inner class UtteranceWebChromeClient : ShellWebChromeClient() {
        override fun onPermissionRequest(request: PermissionRequest) {
            // Only the microphone, and only from our own page. A PermissionRequest
            // carries the origin that made it; the shell confines navigation, but
            // an iframe is not a navigation, so the check is worth making here too.
            val wantsMic = PermissionRequest.RESOURCE_AUDIO_CAPTURE in request.resources
            if (!wantsMic || request.origin?.host != UTTERANCE_HOST) {
                request.deny()
                return
            }
            if (hasMicPermission()) {
                request.grant(arrayOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE))
                return
            }
            // Drop any older pending request first: it can no longer be answered,
            // and leaving it would let this answer grant that one.
            pendingMic?.deny()
            pendingMic = request
            askAndroid.launch(Manifest.permission.RECORD_AUDIO)
        }

        /** The page navigated away or reloaded — the request is void. */
        override fun onPermissionRequestCanceled(request: PermissionRequest) {
            if (pendingMic == request) pendingMic = null
        }
    }

    private fun hasMicPermission(): Boolean =
        ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED

    private companion object {
        const val UTTERANCE_HOST = "utterance.xinutec.org"

        // Voice into music (HTTPS — getUserMedia needs a secure context — behind a login).
        const val UTTERANCE_URL = "https://$UTTERANCE_HOST/"

        // The Nextcloud identity provider the login bounces through.
        const val NC_HOST = "dash.xinutec.org"
    }
}
