# Budget

A monthly budget planner written in Rust (egui). One codebase runs on Windows and Android, and also on Linux and macOS.

On first launch it loads made-up sample data in the "Monthly Costs" spreadsheet layout. Import your own spreadsheet (saved as CSV) from Import / Export to replace it.

## What it does

- **Overview:** income after tax, savings, total expenses and money left over for any month. Expenses are grouped by category, with balance and payments left for loans and contracts. A "left-over" line (like a rent budget) is calculated as income minus savings minus everything else.
- **Expenses:** add, edit or delete expenses. Each one can repeat one-time, weekly, every 2 weeks, monthly, every 3 months or yearly, starting on a first due date. Set **payments left** and the expense stops after the last one. Set **balance left** and it counts down with each payment.
- **Schedule:** a month calendar of what's due each day, plus a list of everything due in the next 30 days.
- **Bank:** import CSV statements from your bank or card. It shows money in and out by month, spending by category and top merchants. It also **finds recurring payments** (subscriptions, insurance, rent, utilities) and adds the ones you pick to your budget with their real amounts and next due dates. Statements are only analyzed in memory; they aren't saved.
- **Import / Export:**
  - Import a Monthly Costs spreadsheet (saved as CSV) or a simple expense list.
  - Export a **monthly summary CSV in the Monthly Costs spreadsheet layout**, with Total Expenses and Left Over added at the bottom.
  - Export all expenses as a CSV, or a blank import template.
- **Settings:**
  - Income: hourly rate, hours per week, weeks per month and tax rate, or a fixed monthly amount.
  - Savings rate, savings goal and amount saved so far.
  - Category order, renaming and deleting.

Everything is saved automatically:

- **Windows:** `%APPDATA%\HammerJones\Budget\data\budget.json`
- **Android:** the app's private storage

## Build on Windows

1. Install Rust from https://rustup.rs. Accept the defaults; it installs the Visual Studio C++ build tools if asked.
2. In this folder, run:

   ```
   cargo build --release
   ```

3. The app is `target\release\budget-app.exe`. Copy it anywhere. It's a single file with no installer.

## Build for Android

The Android app is a small Gradle project in `android/`. The Rust code runs as the app's main screen, and a few Kotlin files add the purchase tracker, the home screen widget and the daily budget notification.

You need Android Studio (for the SDK and NDK), Rust and cargo-ndk:

```
rustup target add aarch64-linux-android
cargo install cargo-ndk
cargo ndk -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build --release --lib
```

Then open the `android` folder in Android Studio and run it, or build from the command line with `gradle assembleRelease` inside `android/`. A release build needs a signing key at `android/budget.keystore` (store and key password `android`, alias `androiddebugkey`):

```
keytool -genkeypair -keystore android/budget.keystore -alias androiddebugkey -keyalg RSA -validity 10000 -storepass android -keypass android -dname "CN=Budget"
```

**Easier: let GitHub build both.** Push this folder to a GitHub repository. The workflow in `.github/workflows/build.yml` runs the tests and builds the Windows `.exe` and the Android `.apk`. Download them from the repo's Actions tab, under Artifacts. The workflow keeps the same signing key between builds, so new versions install over old ones.

### Android notes

- **Today tab and daily budget:** your monthly income minus savings and bills, minus what you've spent this month, spread over the days left. Spend less today and tomorrow's amount grows; spend more and it shrinks. If your left-over line (like rent) has no amount, enter what you really pay so it's set aside.
- **Auto-tracking:** turn on Notification access for Budget (the Today tab has a button). Purchases from Google Wallet, PayPal, Capital One, Venmo and Cash App notifications are logged as they arrive. Tap 🏠 on a purchase to treat that merchant as a bill already in your budget, or 🚫 to not count one purchase. On Android 13+, sideloaded apps may need App info > ⋮ > Allow restricted settings first.
- **Widget and notification:** long-press your home screen > Widgets > Budget for the fuel dial. The always-on notification shows the same thing, and its status-bar icon shows the needle. Turn it off on the Today tab.

- If the top bar sits under your phone's status bar or camera, or the bottom tabs sit under the gesture bar, adjust **Settings > Screen spacing**.

- **File access:** Android only shows CSVs saved by other apps (your browser, Drive) to apps with "All files access". The Files and Bank tabs show a **Grant file access** button that opens that switch. You can also find it at Settings > Apps > Special app access > All files access > Budget.
- **Importing:** tap "Choose a CSV file" (Files tab) or "Choose statement CSV" (Bank tab). A built-in browser lists CSV files in Downloads, Documents and other folders, newest first.
- **Exporting:** CSVs are saved to `Download/Budget/` on your phone, where any file manager, email or Drive app can find them.
- Amounts use drag/tap number fields. Text fields open the on-screen keyboard.

## Importing bank statements

Download a CSV from your bank's website, usually under "Download transactions" or "Export". Three or more months gives the best results. These formats are recognized:

- `Date, Description, Amount`: works for checking accounts, where purchases are negative, and for card exports, where charges are positive.
- `Transaction Date, ..., Description, ..., Debit, Credit`: Capital One and similar.
- Files with no header row (date, amount, ..., description), as some banks export.

PayPal activity exports (Activity > Statements > Download, CSV) also work. Pending rows and card authorization holds are skipped. Direct deposits count as income. Money moved between your own accounts doesn't count as money in or out: transfers to and from the bank or a card, credit card payments and autopay. The Bank tab shows that total separately.

A payment counts as recurring when the same merchant charges on a regular schedule (weekly, every 2 weeks, monthly, every 3 months or yearly) for a similar amount. That means at least 2 charges, or 3 for weekly and biweekly. Grocery and gas trips vary too much to count, so they show up only in the spending breakdown. Payments that look like something already in your budget start unchecked.

Try it with `tests/sample_statement.csv`.

## Expense list CSV format (for importing)

```
name,category,amount,frequency,start_date,remaining_balance,remaining_payments,is_remainder,notes
Car Payment,Transportation,300.00,Monthly,2026-11-05,3600.00,12,,
Rent,Housing,0,Monthly,2026-11-01,,,yes,Max rent = what's left
Gym,Health,40.00,Monthly,2026-11-15,,,,
```

- Headers are matched loosely, so "Description", "Cost" and "Due Date" also work.
- Dates can be `2026-11-05` or `11/05/2026`.
- Frequency can be once, weekly, biweekly, monthly, quarterly or yearly.

## Project layout

- `src/model.rs`: budget math and scheduling, with unit tests
- `src/csvio.rs`: spreadsheet and expense-list import and export, with tests that check the export reproduces the sample spreadsheet line for line
- `src/bank.rs`: statement parsing, recurring-payment detection and spending analysis, with tests
- `src/app.rs`: the interface
- `src/storage.rs`: where data is saved
- `src/lib.rs`, `src/main.rs`: desktop and Android entry points

Run the tests with `cargo test`.
