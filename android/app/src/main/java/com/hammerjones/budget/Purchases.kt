package com.hammerjones.budget

import android.content.Context
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.util.Locale
import kotlin.math.abs

/** Turns payment notifications into rows in purchases.tsv (read by the app and the widget). */
object Purchases {
    const val FILE = "purchases.tsv"
    private const val HEADER = "id\tms\tdate\ttime\tsource\tamount\tmerchant\n"

    /** Apps whose notifications we read. Everything else is ignored. */
    val WATCHED = setOf(
        "com.google.android.apps.walletnfcrel", // Google Wallet
        "com.google.android.gms", // Google Pay tap-to-pay receipts on some phones
        "com.paypal.android.p2pmobile", // PayPal
        "com.konylabs.capitalone", // Capital One
        "com.venmo",
        "com.squareup.cash",
    )

    data class Parsed(val amount: Double, val merchant: String)

    private val MONEY = Regex("""\$\s?(\d{1,3}(?:,\d{3})+(?:\.\d{2})?|\d+(?:\.\d{2})?)""")
    private val PLACE = Regex("""\b(?:at|to)\s+([^\n$]{2,60}?)(?:\s+(?:with|using|on|for|via)\b|\.\s|[.\n]|$)""", RegexOption.IGNORE_CASE)

    /** Notifications about money that isn't a purchase you made. */
    private val SKIP = listOf(
        "declined", "not approved", "failed", "couldn't", "could not", "unsuccessful",
        "verify", "security", "sign in", "log in", "login", "password", "code is",
        "low balance", "available balance", "balance is", "credit limit", "statement", "due on", "is due", "reminder",
        "request", "offer", "reward", "cash back", "cashback", "points", "pay later", "pay in 4", "scheduled", "upcoming",
        "received", "sent you", "paid you", "direct deposit", "deposit", "added to your", "transferred to your",
    )
    private val REFUND = listOf("refund", "returned", "reversed")
    private val GENERIC = setOf("google wallet", "google pay", "paypal", "payment", "payment sent", "purchase", "transaction", "capital one", "venmo", "cash app")

    fun parse(pkg: String, title: String, text: String): Parsed? {
        val all = "$title\n$text".trim()
        val lower = all.lowercase(Locale.US)
        val refund = REFUND.any { lower.contains(it) }
        if (!refund && SKIP.any { lower.contains(it) }) return null
        val m = MONEY.find(all) ?: return null
        val amount = m.groupValues[1].replace(",", "").toDoubleOrNull() ?: return null
        if (amount <= 0.0 || amount > 50_000.0) return null
        return Parsed(if (refund) -amount else amount, merchant(pkg, title, text))
    }

    private fun merchant(pkg: String, title: String, text: String): String {
        for (src in listOf(text, title)) {
            val found = PLACE.find(src)?.groupValues?.get(1)?.trim()?.trimEnd('.', ',')
            if (!found.isNullOrEmpty() && !found.contains('$')) return found
        }
        val t = title.trim()
        if (t.isNotEmpty() && !t.contains('$') && t.lowercase(Locale.US) !in GENERIC) return t
        return when (pkg) {
            "com.paypal.android.p2pmobile" -> "PayPal payment"
            "com.konylabs.capitalone" -> "Capital One purchase"
            else -> "Card purchase"
        }
    }

    private fun clean(s: String) = s.replace('\t', ' ').replace('\n', ' ').replace('\r', ' ').trim().take(80)

    /** Append a purchase. Returns false if it's a repeat of one already logged. */
    @Synchronized
    fun record(ctx: Context, postTime: Long, pkg: String, p: Parsed): Boolean {
        val f = File(ctx.filesDir, FILE)
        val amountText = String.format(Locale.US, "%.2f", p.amount)
        if (f.exists()) {
            for (line in f.readLines()) {
                val c = line.split('\t')
                if (c.size < 7) continue
                val ms = c[1].toLongOrNull() ?: continue
                // The same charge often notifies twice (Wallet + Google Play services, or an update).
                if (c[5] == amountText && abs(ms - postTime) < 3 * 60_000L) return false
            }
        } else {
            f.writeText(HEADER)
        }
        val dt = Instant.ofEpochMilli(postTime).atZone(ZoneId.systemDefault())
        val row = listOf(
            "n$postTime",
            postTime.toString(),
            dt.toLocalDate().toString(),
            String.format(Locale.US, "%02d:%02d", dt.hour, dt.minute),
            pkg,
            amountText,
            clean(p.merchant),
        ).joinToString("\t")
        f.appendText(row + "\n")
        return true
    }
}
