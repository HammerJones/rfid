package com.hammerjones.budget

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Midnight alarm and app updates: redraw the widget and notification. */
class RefreshReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        Notifier.refreshAll(ctx)
    }
}
