package com.hammerjones.budget

import android.app.Notification
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification

/** Runs in the background once "Notification access" is on. Logs purchases as they're notified. */
class PurchaseListener : NotificationListenerService() {
    override fun onListenerConnected() {
        Notifier.refreshAll(this)
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        try {
            if (sbn.packageName !in Purchases.WATCHED) return
            val n = sbn.notification ?: return
            if (n.flags and Notification.FLAG_GROUP_SUMMARY != 0) return
            val extras = n.extras
            val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString().orEmpty()
            val text = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT)
                ?: extras.getCharSequence(Notification.EXTRA_TEXT))?.toString().orEmpty()
            val parsed = Purchases.parse(sbn.packageName, title, text) ?: return
            if (Purchases.record(this, sbn.postTime, sbn.packageName, parsed)) {
                Notifier.refreshAll(this)
            }
        } catch (e: Exception) {
            // Never crash the listener over one odd notification.
        }
    }
}
