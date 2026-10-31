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
    !is_bill(b, &p.merchant)
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
    let start = month_start(today.year(), today.month());
    let end = month_end(start);
    let month_money = b.spending_money(today);
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
    let today_max = ((month_money - before) / days_left as f64).max(0.0);
    DayStatus {
        month_money,
        spent_before_today: before,
        spent_today: now,
        days_left,
        today_max,
        left_today: today_max - now,
    }
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// What the widget needs to work out today's numbers on its own.
pub fn widget_json(b: &Budget, today: NaiveDate) -> String {
    let ignored: Vec<String> = b.daily.ignored.iter().map(|s| json_str(s)).collect();
    let bills: Vec<String> = b.daily.bill_merchants.iter().map(|s| json_str(s)).collect();
    format!(
        "{{\"month\":\"{}\",\"month_money\":{:.2},\"hide_notification\":{},\"ignored\":[{}],\"bill_merchants\":[{}]}}\n",
        today.format("%Y-%m"),
        b.spending_money(today),
        b.daily.hide_notification,
        ignored.join(","),
        bills.join(",")
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
a\t0\t2026-11-01\t09:00\tcom.google.android.apps.walletnfcrel\t30.00\tKing Soopers
b\t0\t2026-11-01\t12:00\tcom.paypal.android.p2pmobile\t500.00\tYSI*COLORADO STATION
a\t0\t2026-11-01\t09:00\tcom.google.android.apps.walletnfcrel\t30.00\tKing Soopers
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
        assert_eq!(s.spent_before_today, 30.0); // rent counted as a bill, not spending
        assert_eq!(s.days_left, 29);
        assert!((s.today_max - 1470.0 / 29.0).abs() < 1e-9);
        assert!((s.left_today - (1470.0 / 29.0 - 10.0)).abs() < 1e-9);

        b.daily.ignored.push("c".into());
        assert_eq!(status(&b, &p, d(2026, 11, 2)).spent_today, 0.0);
    }

    #[test]
    fn widget_file_is_json() {
        let mut b = budget();
        b.daily.bill_merchants.push("COLORADO \"STATION".into());
        let v: serde_json::Value = serde_json::from_str(&widget_json(&b, d(2026, 11, 2))).unwrap();
        assert_eq!(v["month"], "2026-11");
        assert_eq!(v["month_money"], 1500.0);
        assert_eq!(v["hide_notification"], false);
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
