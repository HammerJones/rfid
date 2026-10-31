//! Daily spending budget and auto-tracked purchases.
//!
//! On Android, a small listener (android/…/PurchaseListener.kt) reads purchase notifications from
//! Google Wallet, PayPal and similar apps and appends them to `purchases.tsv` in the app's data
//! folder. Purchases added by hand go to the same file. The home screen widget reads the same file
//! plus `widget.json`, which this module writes, so the app and widget always agree.

use crate::bank::merchant_key;
use crate::model::{month_end, month_start, Budget};
use chrono::{Datelike, NaiveDate};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const PURCHASES_FILE: &str = "purchases.tsv";
pub const WIDGET_FILE: &str = "widget.json";

#[derive(Clone, Debug, PartialEq)]
pub struct Purchase {
    pub id: String,
    pub date: NaiveDate,
    /// "HH:MM", local time.
    pub time: String,
    /// App it came from: "wallet", "paypal", "manual", or a package name.
    pub source: String,
    /// Positive = spent, negative = refund or money received.
    pub amount: f64,
    pub merchant: String,
}

impl Purchase {
    pub fn source_label(&self) -> &str {
        match self.source.as_str() {
            "com.google.android.apps.walletnfcrel" | "com.google.android.gms" | "wallet" => "Google Wallet",
            "com.paypal.android.p2pmobile" | "paypal" => "PayPal",
            "manual" => "Added by hand",
            s if s.contains("capitalone") => "Capital One",
            s => s,
        }
    }
}

pub fn purchases_path(dir: &Path) -> PathBuf {
    dir.join(PURCHASES_FILE)
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ").trim().to_string()
}

/// Parse purchases.tsv: id, epoch ms, local date, local time, source, amount, merchant.
pub fn parse_purchases(text: &str) -> Vec<Purchase> {
    let mut out: Vec<Purchase> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 || f[0] == "id" {
            continue;
        }
        let (Ok(date), Ok(amount)) = (NaiveDate::parse_from_str(f[2], "%Y-%m-%d"), f[5].parse::<f64>()) else {
            continue;
        };
        if out.iter().any(|p| p.id == f[0]) {
            continue;
        }
        out.push(Purchase {
            id: f[0].to_string(),
            date,
            time: f[3].to_string(),
            source: f[4].to_string(),
            amount,
            merchant: f[6].to_string(),
        });
    }
    out.sort_by(|a, b| (b.date, &b.time).cmp(&(a.date, &a.time)));
    out
}

pub fn load_purchases(dir: &Path) -> Vec<Purchase> {
    std::fs::read_to_string(purchases_path(dir)).map(|t| parse_purchases(&t)).unwrap_or_default()
}

/// Add a purchase by hand.
pub fn add_manual(dir: &Path, date: NaiveDate, time: &str, amount: f64, merchant: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let stamp = chrono::Local::now().timestamp_millis();
    let line = format!(
        "manual-{stamp}\t{stamp}\t{}\t{}\tmanual\t{:.2}\t{}\n",
        date.format("%Y-%m-%d"),
        clean(time),
        amount,
        clean(merchant)
    );
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(purchases_path(dir))?;
    f.write_all(line.as_bytes())
}

/// Delete a purchase that was added by hand.
pub fn remove_manual(dir: &Path, id: &str) -> std::io::Result<()> {
    if !id.starts_with("manual-") {
        return Ok(());
    }
    let path = purchases_path(dir);
    let text = std::fs::read_to_string(&path)?;
    let kept: String = text
        .lines()
        .filter(|l| l.split('\t').next() != Some(id))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(path, kept)
}

/// Does this purchase count against the daily budget?
pub fn counts(b: &Budget, p: &Purchase) -> bool {
    if b.daily.ignored.iter().any(|i| i == &p.id) {
        return false;
    }
    !is_big(b, p.amount) && !is_bill(b, &p.merchant)
}

/// Big purchases (groceries, gas) are left out of the daily budget.
pub fn is_big(b: &Budget, amount: f64) -> bool {
    b.daily.small_cutoff > 0.0 && amount.abs() >= b.daily.small_cutoff
}

/// The planned daily budget for one day, before any carry-over.
pub fn plan_for(b: &Budget, d: NaiveDate) -> f64 {
    let s = &b.daily;
    if s.uses_schedule() {
        if s.is_work_day(d) { s.work_day_budget } else { s.off_day_budget }
    } else {
        let start = month_start(d.year(), d.month());
        let days = (month_end(start) - start).num_days() as f64 + 1.0;
        b.spending_money(d) / days
    }
}

/// Total of the daily plan for d's month.
pub fn month_plan(b: &Budget, d: NaiveDate) -> f64 {
    let start = month_start(d.year(), d.month());
    let end = month_end(start);
    start.iter_days().take_while(|x| *x <= end).map(|x| plan_for(b, x)).sum()
}

/// Average small-purchase spending per work day and per off day in a bank statement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayAverages {
    pub work_avg: f64,
    pub off_avg: f64,
    pub work_days: u32,
    pub off_days: u32,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

pub fn day_averages(b: &Budget, txns: &[crate::bank::Txn]) -> DayAverages {
    let spend: Vec<&crate::bank::Txn> = txns.iter().filter(|t| t.is_spending()).collect();
    let (Some(from), Some(to)) = (spend.iter().map(|t| t.date).min(), spend.iter().map(|t| t.date).max()) else {
        return DayAverages::default();
    };
    // Regular bills (subscriptions) aren't day-to-day spending.
    let recurring: Vec<String> = crate::bank::detect_recurring(txns).into_iter().map(|r| r.key).collect();
    let mut a = DayAverages { from: Some(from), to: Some(to), ..Default::default() };
    for d in from.iter_days().take_while(|d| *d <= to) {
        if b.daily.is_work_day(d) { a.work_days += 1 } else { a.off_days += 1 }
    }
    let (mut w, mut o) = (0.0, 0.0);
    for t in spend {
        if is_big(b, t.spent) || is_bill(b, &t.description) || recurring.contains(&merchant_key(&t.description)) {
            continue;
        }
        if b.daily.is_work_day(t.date) { w += t.spent } else { o += t.spent }
    }
    a.work_avg = if a.work_days > 0 { w / a.work_days as f64 } else { 0.0 };
    a.off_avg = if a.off_days > 0 { o / a.off_days as f64 } else { 0.0 };
    a
}

/// Is this merchant paid as a bill that's already in the budget?
pub fn is_bill(b: &Budget, merchant: &str) -> bool {
    let m = merchant.to_ascii_uppercase();
    b.daily
        .bill_merchants
        .iter()
        .any(|k| !k.is_empty() && k.split(' ').all(|w| m.contains(w)))
}

pub fn bill_key(merchant: &str) -> String {
    merchant_key(merchant).to_ascii_uppercase()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayStatus {
    /// Spending money for the whole month.
    pub month_money: f64,
    pub spent_before_today: f64,
    pub spent_today: f64,
    pub days_left: u32,
    /// What you can spend today: (month money − spent before today) ÷ days left, including today.
    pub today_max: f64,
    pub left_today: f64,
    /// Today's planned amount before carry-over.
    pub today_plan: f64,
    pub work_day: bool,
}

impl DayStatus {
    /// Matches the widget: red when over or under a quarter left, amber under half, else green.
    pub fn level(&self) -> u8 {
        let f = self.fuel();
        if self.left_today < -0.005 || f < 0.25 {
            2
        } else if f < 0.5 {
            1
        } else {
            0
        }
    }

    /// 0..=1 for the gauge.
    pub fn fuel(&self) -> f32 {
        if self.today_max <= 0.0 {
            0.0
        } else {
            (self.left_today / self.today_max).clamp(0.0, 1.0) as f32
        }
    }
}

pub fn status(b: &Budget, purchases: &[Purchase], today: NaiveDate) -> DayStatus {
    let month_first = month_start(today.year(), today.month());
    let end = month_end(month_first);
    let month_money = month_plan(b, today);
    // Carry-over only counts from when tracking started (the first logged purchase), so starting
    // mid-month doesn't hand you the whole month's unspent money at once.
    let tracked_from = purchases.iter().map(|p| p.date).min().unwrap_or(today).min(today);
    let start = month_first.max(tracked_from);
    let mut before = 0.0;
    let mut now = 0.0;
    for p in purchases.iter().filter(|p| p.date >= start && p.date <= today && counts(b, p)) {
        if p.date == today {
            now += p.amount;
        } else {
            before += p.amount;
        }
    }
    let days_left = (end - today).num_days() as u32 + 1;
    // Over- or under-spending so far this month is shared across the rest of the month,
    // in proportion to each day's plan (work days get more than off days).
    let planned_before: f64 = start.iter_days().take_while(|d| *d < today).map(|d| plan_for(b, d)).sum();
    let planned_rest: f64 = today.iter_days().take_while(|d| *d <= end).map(|d| plan_for(b, d)).sum();
    let today_plan = plan_for(b, today);
    let today_max = if planned_rest > 0.0 {
        (today_plan * (planned_rest - (before - planned_before)) / planned_rest).max(0.0)
    } else {
        0.0
    };
    DayStatus {
        month_money,
        spent_before_today: before,
        spent_today: now,
        days_left,
        today_max,
        left_today: today_max - now,
        today_plan,
        work_day: b.daily.is_work_day(today),
    }
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// What the widget needs to work out today's numbers on its own.
pub fn widget_json(b: &Budget, today: NaiveDate) -> String {
    let ignored: Vec<String> = b.daily.ignored.iter().map(|s| json_str(s)).collect();
    let bills: Vec<String> = b.daily.bill_merchants.iter().map(|s| json_str(s)).collect();
    // The plan for this month and next, so the widget keeps working into next month.
    let start = month_start(today.year(), today.month());
    let end = month_end(crate::model::add_months(start, 1));
    let plan: Vec<String> = start
        .iter_days()
        .take_while(|d| *d <= end)
        .map(|d| format!("\"{}\":{:.2}", d.format("%Y-%m-%d"), plan_for(b, d)))
        .collect();
    format!(
        "{{\"month\":\"{}\",\"month_money\":{:.2},\"hide_notification\":{},\"cutoff\":{:.2},\"ignored\":[{}],\"bill_merchants\":[{}],\"plan\":{{{}}}}}\n",
        today.format("%Y-%m"),
        month_plan(b, today),
        b.daily.hide_notification,
        b.daily.small_cutoff,
        ignored.join(","),
        bills.join(","),
        plan.join(",")
    )
}

pub fn write_widget_file(dir: &Path, b: &Budget, today: NaiveDate) {
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join("widget.json.tmp");
    if std::fs::write(&tmp, widget_json(b, today)).is_ok() {
        let _ = std::fs::rename(tmp, dir.join(WIDGET_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Expense, Frequency, IncomeMode};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn budget() -> Budget {
        let mut b = Budget::default();
        b.settings.income_mode = IncomeMode::Fixed;
        b.settings.fixed_monthly_income = 3000.0;
        b.settings.savings_rate = 0.0;
        b.add_expense(Expense {
            id: 0,
            name: "Rent".into(),
            category: "Housing".into(),
            amount: 1500.0,
            frequency: Frequency::Monthly,
            start: d(2026, 1, 1),
            remaining_payments: None,
            remaining_balance: None,
            is_remainder: false,
            notes: String::new(),
        });
        b
    }

    const LOG: &str = "id\tms\tdate\ttime\tsource\tamount\tmerchant
a\t0\t2026-11-01\t09:00\tcom.google.android.apps.walletnfcrel\t12.00\tKing Soopers
b\t0\t2026-11-01\t12:00\tcom.paypal.android.p2pmobile\t500.00\tYSI*COLORADO STATION
a\t0\t2026-11-01\t09:00\tcom.google.android.apps.walletnfcrel\t12.00\tKing Soopers
c\t0\t2026-11-02\t08:00\tmanual\t10.00\tCoffee
bad line
";

    #[test]
    fn parses_and_dedups() {
        let p = parse_purchases(LOG);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].id, "c"); // newest first
    }

    #[test]
    fn daily_math() {
        let mut b = budget();
        b.daily.bill_merchants.push(bill_key("YSI*COLORADO STATION"));
        let p = parse_purchases(LOG);
        // November: 1500 spending money, 30 days.
        let s = status(&b, &p, d(2026, 11, 2));
        assert_eq!(s.month_money, 1500.0);
        assert_eq!(s.spent_before_today, 12.0); // rent counted as a bill, not spending
        assert_eq!(s.days_left, 29);
        assert!((s.today_max - 1488.0 / 29.0).abs() < 1e-9);
        assert!((s.left_today - (1488.0 / 29.0 - 10.0)).abs() < 1e-9);

        b.daily.ignored.push("c".into());
        assert_eq!(status(&b, &p, d(2026, 11, 2)).spent_today, 0.0);
    }

    #[test]
    fn big_purchases_and_schedule() {
        let mut b = budget();
        b.daily.bill_merchants.push(bill_key("YSI*COLORADO STATION"));
        // Nov 2026: 1st is a Sunday. Work Fri/Sat/Sun at $40, off days $10.
        b.daily.work_day_budget = 40.0;
        b.daily.off_day_budget = 10.0;
        let log = "x\t0\t2026-11-01\t09:00\tmanual\t85.00\tCostco\ny\t0\t2026-11-01\t10:00\tmanual\t50.00\tLunch?\nz\t0\t2026-11-01\t11:00\tmanual\t25.00\tCoffee and food\n";
        let p = parse_purchases(log);
        // Costco ($85) and $50 are big purchases; only the $25 counts.
        let sun = status(&b, &p, d(2026, 11, 1));
        assert!(sun.work_day);
        assert_eq!(sun.spent_today, 25.0);
        assert!((sun.today_max - 40.0).abs() < 1e-9);
        // Monday: spent 15 less than planned on Sunday, so the extra is shared out by plan weight.
        let mon = status(&b, &p, d(2026, 11, 2));
        assert!(!mon.work_day);
        let rest: f64 = d(2026, 11, 2).iter_days().take_while(|x| *x <= d(2026, 11, 30)).map(|x| plan_for(&b, x)).sum();
        assert!((mon.today_max - 10.0 * (rest + 15.0) / rest).abs() < 1e-9);
        assert_eq!(month_plan(&b, d(2026, 11, 2)), 13.0 * 40.0 + 17.0 * 10.0);
    }

    #[test]
    fn widget_file_is_json() {
        let mut b = budget();
        b.daily.bill_merchants.push("COLORADO \"STATION".into());
        let v: serde_json::Value = serde_json::from_str(&widget_json(&b, d(2026, 11, 2))).unwrap();
        assert_eq!(v["month"], "2026-11");
        assert_eq!(v["month_money"], 1500.0);
        assert_eq!(v["hide_notification"], false);
        assert_eq!(v["cutoff"], 30.0);
        assert_eq!(v["plan"]["2026-11-02"], 50.0);
    }

    #[test]
    fn manual_add_and_remove() {
        let dir = std::env::temp_dir().join(format!("budget-daily-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        add_manual(&dir, d(2026, 11, 2), "10:00", 4.5, "Tea\tshop").unwrap();
        let p = load_purchases(&dir);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].merchant, "Tea shop");
        remove_manual(&dir, &p[0].id).unwrap();
        assert!(load_purchases(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
