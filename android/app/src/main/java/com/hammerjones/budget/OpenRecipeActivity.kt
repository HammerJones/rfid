package com.hammerjones.budget

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import java.io.File

/** Tapping a meal in the notification: leave the recipe id for the app, then bring the app up. */
class OpenRecipeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val id = intent.getLongExtra("recipe", -1L)
        if (id >= 0) {
            try { File(filesDir, "open_recipe.txt").writeText(id.toString()) } catch (e: Exception) {}
        }
        packageManager.getLaunchIntentForPackage(packageName)?.let {
            it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED)
            startActivity(it)
        }
        finish()
    }
}
