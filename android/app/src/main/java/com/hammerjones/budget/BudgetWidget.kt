package com.hammerjones.budget

import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews

/** Home screen fuel dial. The app sends REFRESH_WIDGET when anything changes. */
class BudgetWidget : AppWidgetProvider() {
    override fun onUpdate(ctx: Context, mgr: AppWidgetManager, ids: IntArray) {
        Notifier.refreshAll(ctx)
    }

    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action == "${ctx.packageName}.REFRESH_WIDGET") {
            Notifier.refreshAll(ctx)
            return
        }
        super.onReceive(ctx, intent)
    }

    companion object {
        fun updateAll(ctx: Context) {
            val mgr = AppWidgetManager.getInstance(ctx) ?: return
            val ids = mgr.getAppWidgetIds(ComponentName(ctx, BudgetWidget::class.java))
            if (ids.isEmpty()) return
            val views = build(ctx)
            for (id in ids) mgr.updateAppWidget(id, views)
        }

        private fun build(ctx: Context): RemoteViews {
            val v = RemoteViews(ctx.packageName, R.layout.budget_widget)
            val s = Daily.compute(ctx)
            if (!s.ready) {
                v.setImageViewBitmap(R.id.dial, Dial.gauge(0f, Daily.GREEN, 300, 200))
                v.setTextViewText(R.id.amount, "Budget")
                v.setTextColor(R.id.amount, Daily.GREEN)
                v.setTextViewText(R.id.detail, "Open the app to set up")
            } else {
                val color = Daily.colorFor(s)
                v.setImageViewBitmap(R.id.dial, Dial.gauge(s.fuel, color, 300, 200))
                v.setTextViewText(R.id.amount, if (s.over) Daily.money(-s.leftToday) else Daily.money(s.leftToday))
                v.setTextColor(R.id.amount, color)
                v.setTextViewText(R.id.caption, if (s.over) "over today" else "left today")
                v.setTextViewText(R.id.detail, "of ${Daily.money(s.todayMax)}\n${Daily.money(s.spentToday)} spent")
            }
            Notifier.openApp(ctx)?.let { v.setOnClickPendingIntent(R.id.root, it) }
            return v
        }
    }
}
