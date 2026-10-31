package com.hammerjones.budget

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.os.Build
import org.json.JSONObject
import java.io.File
import java.time.LocalDate

/** "Today's meals" notification. Tapping a meal opens its recipe inside Budget. */
object MealNotifier {
    private const val CHANNEL = "meal_plan"
    private const val ID = 2

    private data class Meal(val slot: String, val order: Int, val name: String, val recipe: Long)

    fun update(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java) ?: return
        val file = File(ctx.filesDir, "meals.json")
        if (!file.exists()) { nm.cancel(ID); return }
        val j = try { JSONObject(file.readText()) } catch (e: Exception) { nm.cancel(ID); return }
        if (j.optBoolean("hide", false)) { nm.cancel(ID); return }
        if (Build.VERSION.SDK_INT >= 33 &&
            ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return

        val today = LocalDate.now().toString()
        val meals = ArrayList<Meal>()
        val arr = j.optJSONArray("meals")
        if (arr != null) {
            for (i in 0 until arr.length()) {
                val m = arr.optJSONObject(i) ?: continue
                if (m.optString("date") != today || m.optBoolean("cooked", false)) continue
                meals.add(Meal(m.optString("slot"), m.optInt("order"), m.optString("name"), m.optLong("recipe", -1L)))
            }
        }
        meals.sortBy { it.order }
        if (meals.isEmpty()) { nm.cancel(ID); return }

        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Meal plan", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Today's planned meals"
            setShowBadge(false)
        })
        val title = if (meals.size == 1) "${meals[0].slot}: ${meals[0].name}" else "Today's meals"
        val text = meals.joinToString(" · ") { "${it.slot}: ${it.name}" }
        val style = Notification.InboxStyle()
        meals.forEach { style.addLine("${it.slot}: ${it.name}") }
        style.setSummaryText("Tap a meal for the recipe")
        val b = Notification.Builder(ctx, CHANNEL)
            .setSmallIcon(Icon.createWithResource(ctx, R.drawable.ic_meal))
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(style)
            .setColor(Daily.GREEN)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setContentIntent(openRecipe(ctx, meals[0].recipe, 0))
        meals.take(3).forEachIndexed { i, m ->
            val label = "${m.slot}: ${m.name}".let { if (it.length > 26) it.take(25) + "…" else it }
            b.addAction(Notification.Action.Builder(Icon.createWithResource(ctx, R.drawable.ic_meal), label, openRecipe(ctx, m.recipe, i + 1)).build())
        }
        nm.notify(ID, b.build())
    }

    private fun openRecipe(ctx: Context, recipe: Long, code: Int): PendingIntent {
        val intent = Intent(ctx, OpenRecipeActivity::class.java)
            .putExtra("recipe", recipe)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return PendingIntent.getActivity(ctx, 100 + code, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }
}
