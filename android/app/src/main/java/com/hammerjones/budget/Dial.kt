package com.hammerjones.budget

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/** Draws the classic fuel dial: E on the left, F on the right, needle pointing at what's left today. */
object Dial {
    private const val TRACK = 0xFF0A0D16.toInt()
    private const val NEEDLE = 0xFFE6EBF5.toInt()

    /** Full-color dial for the widget and the notification. */
    fun gauge(fuel: Float, color: Int, w: Int, h: Int, labels: Boolean = true): Bitmap {
        val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val stroke = min(w, h) * 0.11f
        val cx = w / 2f
        val r = min(w / 2f - stroke, h * 0.78f - stroke)
        val cy = h * 0.5f + r * 0.42f
        val oval = RectF(cx - r, cy - r, cx + r, cy + r)
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = stroke
            strokeCap = Paint.Cap.BUTT
        }
        // Track, then faint red / amber / green zones, then the bright fill up to the needle.
        p.color = TRACK
        c.drawArc(oval, 180f, 180f, false, p)
        p.color = withAlpha(Daily.RED, 70); c.drawArc(oval, 180f, 45f, false, p)
        p.color = withAlpha(Daily.AMBER, 70); c.drawArc(oval, 225f, 45f, false, p)
        p.color = withAlpha(Daily.GREEN, 70); c.drawArc(oval, 270f, 90f, false, p)
        if (fuel > 0.005f) {
            p.color = color
            c.drawArc(oval, 180f, 180f * fuel.coerceIn(0f, 1f), false, p)
        }
        // Quarter ticks.
        val tick = Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = Color.argb(150, 255, 255, 255); strokeWidth = stroke * 0.16f }
        for (i in 1..3) {
            val a = Math.toRadians(180.0 + 45.0 * i)
            val r1 = r - stroke * 0.45f
            val r2 = r + stroke * 0.45f
            c.drawLine(cx + (cos(a) * r1).toFloat(), cy + (sin(a) * r1).toFloat(), cx + (cos(a) * r2).toFloat(), cy + (sin(a) * r2).toFloat(), tick)
        }
        drawNeedle(c, cx, cy, r * 0.82f, fuel, NEEDLE, stroke * 0.42f)
        if (labels) {
            val t = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                textSize = stroke * 1.15f
                isFakeBoldText = true
                textAlign = Paint.Align.CENTER
            }
            t.color = Daily.RED
            c.drawText("E", cx - r, cy + stroke * 1.55f, t)
            t.color = Daily.GREEN
            c.drawText("F", cx + r, cy + stroke * 1.55f, t)
        }
        return bmp
    }

    /** White-on-clear dial for the status bar (Android only shows these in one color). */
    fun statusIcon(fuel: Float, over: Boolean, size: Int = 96): Bitmap {
        val bmp = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val s = size.toFloat()
        val stroke = s * 0.1f
        val r = s * 0.38f
        val cx = s / 2f
        val cy = s * 0.72f
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = stroke
            strokeCap = Paint.Cap.ROUND
            color = Color.WHITE
        }
        c.drawArc(RectF(cx - r, cy - r, cx + r, cy + r), 180f, 180f, false, p)
        drawNeedle(c, cx, cy, r * 0.9f, if (over) 0f else fuel, Color.WHITE, stroke)
        if (over) {
            val t = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = Color.WHITE
                textSize = s * 0.42f
                isFakeBoldText = true
                textAlign = Paint.Align.CENTER
            }
            c.drawText("!", s * 0.86f, s * 0.36f, t)
        }
        return bmp
    }

    private fun drawNeedle(c: Canvas, cx: Float, cy: Float, len: Float, fuel: Float, color: Int, width: Float) {
        val a = Math.toRadians(180.0 + 180.0 * fuel.coerceIn(0f, 1f))
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            this.color = color
            strokeWidth = width
            strokeCap = Paint.Cap.ROUND
        }
        c.drawLine(cx, cy, cx + (cos(a) * len).toFloat(), cy + (sin(a) * len).toFloat(), p)
        p.style = Paint.Style.FILL
        c.drawCircle(cx, cy, width * 1.25f, p)
    }

    private fun withAlpha(color: Int, alpha: Int): Int = Color.argb(alpha, Color.red(color), Color.green(color), Color.blue(color))
}
