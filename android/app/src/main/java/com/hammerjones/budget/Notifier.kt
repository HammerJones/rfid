package com.hammerjones.budget

import android.Manifest
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.os.Build
import java.time.LocalDate
import java.time.ZoneId

/** The always-on daily budget notification, plus the midnight refresh. */
object Notifier {
    private const val CHANNEL = "daily_budget"
    private const val ID = 1

    fun refreshAll(ctx: Context) {
        try { BudgetWidget.updateAll(ctx) } catch (e: Exception) {}
        try { update(ctx) } catch (e: Exception) {}
        try { MealNotifier.update(ctx) } catch (e: Exception) {}
        try { scheduleMidnight(ctx) } catch (e: Exception) {}
    }

    fun update(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java) ?: return
        val s = Daily.compute(ctx)
        if (!s.ready || !s.notify) {
            nm.cancel(ID)
            return
        }
        if (Build.VERSION.SDK_INT >= 33 &&
            ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return

        val channel = NotificationChannel(CHANNEL, "Daily budget", NotificationManager.IMPORTANCE_LOW).apply {
            description = "What you have left to spend today"
            setShowBadge(false)
        }
        nm.createNotificationChannel(channel)

        val color = Daily.colorFor(s)
        val title = if (s.over) "${Daily.money(-s.leftToday)} over today" else "${Daily.money(s.leftToday)} left today"
        val text = "of ${Daily.money(s.todayMax)} · ${Daily.money(s.spentToday)} spent · ${s.daysLeft} days left"
        val n = Notification.Builder(ctx, CHANNEL)
            .setSmallIcon(Icon.createWithBitmap(Dial.statusIcon(s.fuel, s.over)))
            .setLargeIcon(Dial.gauge(s.fuel, color, 220, 220, labels = false))
            .setContentTitle(title)
            .setContentText(text)
            .setColor(color)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setCategory(Notification.CATEGORY_STATUS)
            .setContentIntent(openApp(ctx))
            .build()
        nm.notify(ID, n)
    }

    fun openApp(ctx: Context): PendingIntent? {
        val intent = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName) ?: return null
        return PendingIntent.getActivity(ctx, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** A new day means a new daily amount: redraw just after midnight. */
    private fun scheduleMidnight(ctx: Context) {
        val am = ctx.getSystemService(AlarmManager::class.java) ?: return
        val at = LocalDate.now().plusDays(1).atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli() + 60_000L
        val intent = Intent(ctx, RefreshReceiver::class.java).setAction("${ctx.packageName}.MIDNIGHT")
        val pi = PendingIntent.getBroadcast(ctx, 1, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        am.set(AlarmManager.RTC, at, pi)
    }
}
