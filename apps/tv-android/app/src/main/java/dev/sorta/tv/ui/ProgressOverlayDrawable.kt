package dev.sorta.tv.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.text.TextPaint
import androidx.core.content.ContextCompat
import dev.sorta.tv.R

/**
 * Foreground drawable layered on top of a poster image. Renders one
 * of three things depending on its [state]:
 *
 *   - `None`        — nothing (transparent).
 *   - `InProgress`  — a thin circular progress ring in the bottom-
 *                     right corner. Track is faint, fill is accent.
 *   - `Watched`     — a translucent dark scrim across the whole
 *                     poster + a small "Watched" pill in the top-
 *                     left corner.
 *
 * Used as the `foreground` of the card's main image view. Backed by
 * a pure [ProgressOverlayState] so the state mapping is unit-tested
 * without instantiating Android graphics.
 */
class ProgressOverlayDrawable(context: Context) : Drawable() {

    private val density = context.resources.displayMetrics.density

    // Scrim covers the whole poster when watched.
    private val scrimPaint = Paint().apply {
        color = Color.argb(0x99, 0, 0, 0)
        isAntiAlias = false
    }

    // Ring track + fill.
    private val accent = ContextCompat.getColor(context, R.color.progress_overlay_accent)
    private val ringTrackPaint = Paint().apply {
        style = Paint.Style.STROKE
        strokeWidth = 3f * density
        color = Color.argb(0x99, 0, 0, 0)
        isAntiAlias = true
    }
    private val ringFillPaint = Paint().apply {
        style = Paint.Style.STROKE
        strokeWidth = 3f * density
        color = accent
        strokeCap = Paint.Cap.ROUND
        isAntiAlias = true
    }

    // "Watched" pill background + text.
    private val pillBgPaint = Paint().apply {
        color = ContextCompat.getColor(context, R.color.progress_overlay_pill_bg)
        isAntiAlias = true
    }
    private val pillTextPaint = TextPaint().apply {
        color = Color.WHITE
        textSize = 11f * density
        isAntiAlias = true
        isFakeBoldText = true
    }
    private val pillLabel: String = context.getString(R.string.card_watched_label)

    var state: ProgressOverlayState = ProgressOverlayState.None
        set(value) {
            if (field != value) {
                field = value
                invalidateSelf()
            }
        }

    override fun draw(canvas: Canvas) {
        val b = bounds
        when (val s = state) {
            ProgressOverlayState.None -> return
            ProgressOverlayState.Watched -> {
                canvas.drawRect(b, scrimPaint)
                drawWatchedPill(canvas)
            }
            is ProgressOverlayState.InProgress -> {
                drawRing(canvas, s.fraction)
            }
        }
    }

    private fun drawRing(canvas: Canvas, fraction: Float) {
        val ringSize = 20f * density
        val margin = 6f * density
        val left = bounds.right - ringSize - margin
        val top = bounds.bottom - ringSize - margin
        val oval = RectF(left, top, left + ringSize, top + ringSize)
        canvas.drawArc(oval, 0f, 360f, false, ringTrackPaint)
        canvas.drawArc(oval, -90f, 360f * fraction.coerceIn(0f, 1f), false, ringFillPaint)
    }

    private fun drawWatchedPill(canvas: Canvas) {
        val paddingX = 6f * density
        val paddingY = 3f * density
        val margin = 6f * density
        val textWidth = pillTextPaint.measureText(pillLabel)
        val textHeight = pillTextPaint.fontMetrics.run { descent - ascent }
        val left = bounds.left + margin
        val top = bounds.top + margin
        val right = left + textWidth + paddingX * 2
        val bottom = top + textHeight + paddingY * 2
        val pillRect = RectF(left, top, right, bottom)
        canvas.drawRoundRect(pillRect, 6f * density, 6f * density, pillBgPaint)
        val textX = left + paddingX
        val textY = top + paddingY - pillTextPaint.fontMetrics.ascent
        canvas.drawText(pillLabel, textX, textY, pillTextPaint)
    }

    override fun setAlpha(alpha: Int) {
        scrimPaint.alpha = (alpha * 0x99 / 255).coerceIn(0, 255)
    }

    override fun setColorFilter(colorFilter: ColorFilter?) { /* unused */ }

    @Deprecated("Deprecated in Java")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}
