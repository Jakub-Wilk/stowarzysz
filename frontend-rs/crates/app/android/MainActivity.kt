// The Android activity (Dioxus.toml `android_main_activity`) plus what Rust calls through JNI
// (`src/platform/android.rs`). dx compiles only this file, so every Kotlin class lives here.
package dev.dioxus.main

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.view.WindowCompat
import java.io.File

typealias BuildConfig = com.wilkjakub.stowarzysz.BuildConfig

class MainActivity : WryActivity()

private const val PREFS = "stowarzysz"
private const val INSTALL_ERROR = "install_error"

/** Static entry points for Rust. Every method may be called from a non-UI thread. */
object Bridge {
    @JvmStatic
    fun canInstallPackages(context: Context): Boolean =
        context.packageManager.canRequestPackageInstalls()

    /** The "install unknown apps" switch for this app. */
    @JvmStatic
    fun openInstallSettings(context: Context) {
        val intent = Intent(
            Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
            Uri.parse("package:" + context.packageName),
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)
    }

    /**
     * Hand a downloaded APK to the system installer (a PackageInstaller session; the system asks
     * the user to confirm, see InstallReceiver). Returns an error message, or null when started.
     */
    @JvmStatic
    fun installApk(context: Context, path: String): String? = try {
        val file = File(path)
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(context.packageName)
        if (Build.VERSION.SDK_INT >= 31) {
            params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            file.inputStream().use { input ->
                session.openWrite("update.apk", 0, file.length()).use { output ->
                    input.copyTo(output)
                    session.fsync(output)
                }
            }
            val intent = Intent(context, InstallReceiver::class.java)
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or
                (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
            val pending = PendingIntent.getBroadcast(context, id, intent, flags)
            session.commit(pending.intentSender)
        }
        null
    } catch (e: Exception) {
        e.message ?: e.toString()
    }

    /** The last failure InstallReceiver recorded, once. */
    @JvmStatic
    fun takeInstallError(context: Context): String? {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val error = prefs.getString(INSTALL_ERROR, null)
        if (error != null) prefs.edit().remove(INSTALL_ERROR).apply()
        return error
    }

    /** Colour the status and navigation bars like the page (`light`: dark icons). */
    @JvmStatic
    fun setSystemBars(activity: Activity, color: Int, light: Boolean) {
        activity.runOnUiThread {
            val window = activity.window
            @Suppress("DEPRECATION")
            window.statusBarColor = color
            @Suppress("DEPRECATION")
            window.navigationBarColor = color
            WindowCompat.getInsetsController(window, window.decorView).apply {
                isAppearanceLightStatusBars = light
                isAppearanceLightNavigationBars = light
            }
        }
    }
}

/** Results of an install session (declared in Dioxus.toml `[android.raw]`). */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm: Intent? = if (Build.VERSION.SDK_INT >= 33) {
                    intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(Intent.EXTRA_INTENT)
                }
                confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                if (confirm != null) context.startActivity(confirm)
            }
            PackageInstaller.STATUS_SUCCESS -> Unit // the new version replaces this process
            else -> {
                val detail = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
                context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                    .putString(INSTALL_ERROR, message(status, detail))
                    .apply()
            }
        }
    }

    private fun message(status: Int, detail: String?): String = when (status) {
        PackageInstaller.STATUS_FAILURE_ABORTED -> "Instalację anulowano."
        PackageInstaller.STATUS_FAILURE_STORAGE -> "Za mało miejsca na urządzeniu."
        PackageInstaller.STATUS_FAILURE_CONFLICT,
        PackageInstaller.STATUS_FAILURE_INCOMPATIBLE ->
            "Aktualizacja nie pasuje do zainstalowanej aplikacji (inny podpis?)."
        PackageInstaller.STATUS_FAILURE_BLOCKED -> "System zablokował instalację."
        else -> "Instalacja się nie udała" + (if (detail != null) " ($detail)." else ".")
    }
}
