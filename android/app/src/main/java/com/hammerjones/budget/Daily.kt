package com.hammerjones.budget

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.time.LocalDate
import java.time.YearMonth
import java.util.Locale
import kotlin.math.abs

/** Today's numbers. Mirrors daily::status in src/daily.rs so the app, widget and notification agree. */
data class DayStatus(
    val ready: Boolean,
    val monthMoney: Double = 0.0,
    val spentBefore: Double = 0.0,
    val spentToday: Double = 0.0,
    val daysLeft: Int = 0,
    val todayMax: Double = 0.0,
    val leftToday: Double = 0.0,
    val notify: Boolean = true,
) {
    val fuel: Float
        get() = if (todayMax <= 0.0) 0f else (leftToday / todayMax).coerceIn(0.0, 1.0).toFloat()
    val over: Boolean
        get() = leftToday < -0.005
}

object Daily {
    const val GREEN = 0xFF4ADE80.toInt()
    const val AMBER = 0xFFFBBF24.toInt()
    const val RED = 0xFFF87171.toInt()

    fun compute(ctx: Context, today: LocalDate = LocalDate.now()): DayStatus {
        val dir = ctx.filesDir
        val wf = File(dir, "widget.json")
        if (!wf.exists()) return DayStatus(ready = false)
        val j = try {
            JSONObject(wf.readText())
        } catch (e: Exception) {
            return DayStatus(ready = false)
        }
        val monthMoney = j.optDouble("month_money", 0.0)
        val cutoff = j.optDouble("cutoff", 0.0)
        val planJson = j.optJSONObject("plan")
        val evenly = monthMoney / YearMonth.from(today).lengthOfMonth()
        fun planFor(d: LocalDate): Double = planJson?.optDouble(d.toString(), evenly)?.takeIf { !it.isNaN() } ?: evenly
        val notify = !j.optBoolean("hide_notification", false)
        val ignored = HashSet<String>()
        j.optJSONArray("ignored")?.let { a -> for (i in 0 until a.length()) ignored.add(a.optString(i)) }
        val bills = ArrayList<List<String>>()
        j.optJSONArray("bill_merchants")?.let { a ->
            for (i in 0 until a.length()) {
                val words = a.optString(i).trim().split(" ").filter { it.isNotEmpty() }
                if (words.isNotEmpty()) bills.add(words)
            }
        }

        // Carry-over counts from the first logged purchase (same as src/daily.rs).
        var firstLogged: LocalDate = today
        if (pf0(dir).exists()) {
            for (line in pf0(dir).readLines()) {
                val f = line.split('\t')
                if (f.size < 7 || f[0] == "id") continue
                val dd = try { LocalDate.parse(f[2]) } catch (e: Exception) { continue }
                if (dd.isBefore(firstLogged)) firstLogged = dd
            }
        }
        val monthFirst = today.withDayOfMonth(1)
        val start = if (firstLogged.isAfter(monthFirst)) firstLogged else monthFirst
        var before = 0.0
        var now = 0.0
        val seen = HashSet<String>()
        val pf = File(dir, Purchases.FILE)
        if (pf.exists()) {
            for (line in pf.readLines()) {
                val f = line.split('\t')
                if (f.size < 7 || f[0] == "id" || !seen.add(f[0])) continue
                val date = try {
                    LocalDate.parse(f[2])
                } catch (e: Exception) {
                    continue
                }
                val amount = f[5].toDoubleOrNull() ?: continue
                if (date.isBefore(start) || date.isAfter(today)) continue
                if (f[0] in ignored) continue
                if (cutoff > 0.0 && abs(amount) >= cutoff) continue
                val m = f[6].uppercase(Locale.US)
                if (bills.any { words -> words.all { m.contains(it) } }) continue
                if (date == today) now += amount else before += amount
            }
        }
        val end = YearMonth.from(today).atEndOfMonth()
        val daysLeft = end.dayOfMonth - today.dayOfMonth + 1
        // Same as src/daily.rs: carry-over is shared by each remaining day's plan.
        var plannedBefore = 0.0
        var d = start
        while (d.isBefore(today)) { plannedBefore += planFor(d); d = d.plusDays(1) }
        var plannedRest = 0.0
        d = today
        while (!d.isAfter(end)) { plannedRest += planFor(d); d = d.plusDays(1) }
        val max = if (plannedRest > 0.0) (planFor(today) * (plannedRest - (before - plannedBefore)) / plannedRest).coerceAtLeast(0.0) else 0.0
        return DayStatus(true, monthMoney, before, now, daysLeft, max, max - now, notify)
    }

    private fun pf0(dir: File) = File(dir, Purchases.FILE)

    fun money(v: Double): String {
        val s = String.format(Locale.US, "%,.2f", abs(v))
        return if (v < -0.004) "-$$s" else "$$s"
    }

    /** Green when there's plenty left, amber under half, red under a quarter or over. */
    fun colorFor(s: DayStatus): Int = when {
        s.over || s.fuel < 0.25f -> RED
        s.fuel < 0.5f -> AMBER
        else -> GREEN
    }
}
