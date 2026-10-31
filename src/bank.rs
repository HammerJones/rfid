//! Bank / card statement import, spending analysis and recurring-payment detection.
//!
//! Handles the common CSV shapes banks export:
//! * Date, Description, Amount            (negative = money out, or positive = charges on card exports)
//! * Date, Description, Debit, Credit     (e.g. Capital One)
//! * No header row: date, amount, ..., description   (e.g. some Wells Fargo exports)

use crate::csvio::{parse_date, parse_money};
use crate::model::Frequency;
use chrono::{Datelike, Duration, NaiveDate};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Txn {
    pub date: NaiveDate,
    pub description: String,
    /// Money out is positive, money in is negative.
    pub spent: f64,
    /// Money moving between your own accounts (bank top-ups, card payments, transfers).
    /// Not counted as income or spending.
    pub transfer: bool,
}

impl Txn {
    pub fn is_spending(&self) -> bool {
        self.spent > 0.0 && !self.transfer
    }
}

/// Rows that aren't real money movement: temporary card holds and their reversals.
fn is_hold(kind: &str) -> bool {
    let k = kind.to_ascii_lowercase();
    k.contains("account hold") || k.contains("authorization") || k.contains("reversal of general account hold")
}

/// Money moving between your own accounts. Counting these would double-count income and spending.
pub fn is_transfer(kind: &str, desc: &str) -> bool {
    let text = format!("{} {}", kind, desc).to_ascii_lowercase();
    // Paychecks: PayPal labels direct deposits "Electronic Funds Transfer Funding".
    if ["direct deposit", "direct dep", "payroll", "electronic funds transfer funding"].iter().any(|p| text.contains(p)) {
        return false;
    }
    [
        // PayPal: moving money to/from the bank, a card or PayPal savings
        "bank deposit to pp",
        "card deposit",
        "card withdrawal",
        "open wallet",
        "reversal of ach",
        "transfer to bank",
        // Banks and cards
        "transfer",
        "xfer",
        "card pymt",
        "crd pymt",
        "credit card payment",
        "card payment",
        "autopay",
        "payment thank you",
        "payment - thank you",
    ]
    .iter()
    .any(|p| text.contains(p))
}

/// Statuses that mean the row actually happened.
fn is_settled(status: &str) -> bool {
    let s = status.trim().to_ascii_lowercase();
    s.is_empty() || matches!(s.as_str(), "completed" | "complete" | "posted" | "cleared" | "settled" | "success" | "successful")
}

fn read_rows(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut r = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut out = Vec::new();
    for rec in r.records() {
        let rec = rec.map_err(|e| format!("CSV error: {e}"))?;
        out.push(rec.iter().map(|s| s.trim().to_string()).collect());
    }
    Ok(out)
}

fn col(headers: &[String], names: &[&str]) -> Option<usize> {
    // Exact matches first, then "contains", so "Posted Date" doesn't beat "Transaction Date" by accident.
    let lower: Vec<String> = headers.iter().map(|h| h.to_ascii_lowercase()).collect();
    for n in names {
        if let Some(i) = lower.iter().position(|h| h == n) {
            return Some(i);
        }
    }
    for n in names {
        if let Some(i) = lower.iter().position(|h| h.contains(n)) {
            return Some(i);
        }
    }
    None
}

fn get(r: &[String], i: usize) -> &str {
    r.get(i).map(|s| s.as_str()).unwrap_or("")
}

/// Parse one statement CSV into transactions.
pub fn parse_statement(text: &str) -> Result<Vec<Txn>, String> {
    let rows = read_rows(text)?;
    let rows: Vec<Vec<String>> = rows.into_iter().filter(|r| r.iter().any(|c| !c.is_empty())).collect();
    if rows.is_empty() {
        return Err("The file is empty.".into());
    }

    // Find the header row (some banks put account info lines above it).
    let header_idx = rows.iter().take(15).position(|r| {
        let joined = r.join(" ").to_ascii_lowercase();
        joined.contains("date") && (joined.contains("amount") || joined.contains("debit") || joined.contains("withdraw"))
    });

    let mut txns = Vec::new();
    match header_idx {
        Some(h) => {
            let headers = &rows[h];
            let date_c = col(headers, &["transaction date", "date", "trans date", "posted date", "posting date"])
                .ok_or("Couldn't find a date column.")?;
            let desc_c = col(headers, &["description", "merchant", "payee", "name", "memo", "details", "transaction"])
                .ok_or("Couldn't find a description column.")?;
            let debit_c = col(headers, &["debit", "withdrawal", "withdrawals", "money out", "charges"]);
            let credit_c = col(headers, &["credit", "deposit", "deposits", "money in", "payments"]);
            let amount_c = col(headers, &["amount", "transaction amount"]);
            if debit_c.is_none() && amount_c.is_none() {
                return Err("Couldn't find an amount column (or Debit/Credit columns).".into());
            }
            // Optional: transaction type (PayPal "Type", bank "Transaction Type") and status.
            let lower: Vec<String> = headers.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
            let type_c = lower.iter().position(|h| h == "type" || h == "transaction type");
            let status_c = lower.iter().position(|h| h == "status");
            let mut raw: Vec<(NaiveDate, String, f64, bool)> = Vec::new();
            for r in &rows[h + 1..] {
                let Some(date) = parse_date(get(r, date_c)) else { continue };
                let kind = type_c.map(|c| get(r, c)).unwrap_or("");
                // Skip pending rows and temporary holds; they reverse or post later.
                if status_c.is_some_and(|c| !is_settled(get(r, c))) || is_hold(kind) {
                    continue;
                }
                let mut desc = get(r, desc_c).trim().to_string();
                if desc.is_empty() || desc.eq_ignore_ascii_case("paypal") {
                    desc = if kind.is_empty() { desc } else { kind.to_string() };
                }
                let transfer = is_transfer(kind, &desc);
                let value = match (debit_c, credit_c) {
                    (Some(d), Some(c)) if amount_c.is_none() || d != c => {
                        let out = parse_money(get(r, d)).map(f64::abs).unwrap_or(0.0);
                        let inn = parse_money(get(r, c)).map(f64::abs).unwrap_or(0.0);
                        // Already in "spent" convention.
                        raw.push((date, desc, out - inn, transfer));
                        continue;
                    }
                    _ => parse_money(get(r, amount_c.unwrap())).unwrap_or(0.0),
                };
                raw.push((date, desc, value, transfer));
            }
            // Single amount column: decide which sign means spending.
            if debit_c.is_none() || credit_c.is_none() {
                let neg = raw.iter().filter(|t| t.2 < 0.0).count();
                let pos = raw.len() - neg;
                // Bank accounts: most rows are negative purchases. Card exports: most rows are positive charges.
                let flip = neg >= pos;
                for (d, s, v, transfer) in raw {
                    txns.push(Txn { date: d, description: s, spent: if flip { -v } else { v }, transfer });
                }
            } else {
                for (d, s, v, transfer) in raw {
                    txns.push(Txn { date: d, description: s, spent: v, transfer });
                }
            }
        }
        None => {
            // No header: find the date, amount and description columns by content.
            let first = &rows[0];
            let date_c = (0..first.len()).find(|&i| parse_date(get(first, i)).is_some()).ok_or("Couldn't find dates in this file.")?;
            let amount_c = (0..first.len())
                .find(|&i| i != date_c && parse_money(get(first, i)).is_some() && get(first, i).contains('.'))
                .ok_or("Couldn't find amounts in this file.")?;
            let desc_c = (0..first.len())
                .filter(|&i| i != date_c && i != amount_c)
                .max_by_key(|&i| get(first, i).len())
                .ok_or("Couldn't find descriptions in this file.")?;
            for r in &rows {
                let (Some(date), Some(v)) = (parse_date(get(r, date_c)), parse_money(get(r, amount_c))) else { continue };
                let desc = get(r, desc_c).to_string();
                let transfer = is_transfer("", &desc);
                txns.push(Txn { date, description: desc, spent: -v, transfer });
            }
        }
    }
    if txns.is_empty() {
        return Err("No transactions found.".into());
    }
    txns.sort_by_key(|t| t.date);
    Ok(txns)
}

/// Turn "SQ *BLUE BOTTLE #1234 DENVER CO 03/14" into "BLUE BOTTLE" so the same merchant groups together.
pub fn merchant_key(desc: &str) -> String {
    let mut s = desc.to_ascii_uppercase();
    for p in [
        "POS PURCHASE", "POS DEBIT", "DEBIT CARD PURCHASE", "PURCHASE AUTHORIZED ON", "RECURRING PAYMENT", "ACH DEBIT",
        "ACH WITHDRAWAL", "ONLINE PAYMENT", "CHECKCARD", "CHECK CARD", "VISA DDA PUR", "DBT CRD", "PREAUTHORIZED DEBIT",
    ] {
        s = s.replace(p, " ");
    }
    for p in ["SQ *", "TST*", "TST *", "SP *", "SP*", "PAYPAL *", "PP*", "DD *", "IN *"] {
        if let Some(rest) = s.trim_start().strip_prefix(p) {
            s = rest.to_string();
        }
    }
    let words: Vec<String> = s
        .split(|c: char| !c.is_ascii_alphabetic() && c != '&' && c != '\'')
        .filter(|w| w.len() >= 2)
        .filter(|w| !matches!(*w, "CO" | "CA" | "TX" | "NY" | "WWW" | "COM" | "HTTPS" | "INC" | "LLC" | "USA" | "US"))
        .map(|w| w.to_string())
        .collect();
    let key = words.into_iter().take(2).collect::<Vec<_>>().join(" ");
    if key.is_empty() { desc.trim().to_string() } else { key }
}

/// Best-guess category from the description, using the app's usual category names.
pub fn guess_category(desc: &str) -> &'static str {
    let d = desc.to_ascii_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| d.contains(w));
    if has(&["rent", "apartment", "property", "leasing", "xcel", "energy", "water", "utilit", "comcast", "xfinity", "internet", "centurylink", "lumen", "verizon", "t-mobile", "tmobile", "at&t", "mint mobile"]) {
        "Housing"
    } else if has(&["netflix", "spotify", "youtube", "hulu", "disney", "proton", "apple.com", "itunes", "google", "amazon prime", "prime video", "patreon", "openai", "anthropic", "adobe", "microsoft", "playstation", "xbox", "steam", "audible"]) {
        "Monthly Subscriptions"
    } else if has(&["shell", "conoco", "exxon", "chevron", "sinclair", "valero", "maverik", "kum & go", "loaf", "gas", "fuel", "geico", "progressive", "state farm", "allstate", "insurance", "autozone", "o'reilly", "napa", "parking", "toll", "e-470", "rtd", "uber", "lyft", "auto", "motors", "dmv"]) {
        "Transportation"
    } else if has(&["king soopers", "safeway", "whole foods", "trader joe", "sprouts", "walmart", "target", "costco", "aldi", "grocery", "market", "starbucks", "coffee", "dunkin", "mcdonald", "chipotle", "taco", "pizza", "doordash", "grubhub", "uber eats", "restaurant", "grill", "cafe", "bar ", "brew", "kitchen", "burger", "subway"]) {
        "Food"
    } else if has(&["pharmacy", "walgreens", "cvs", "rx", "medical", "health", "dental", "clinic"]) {
        "Health"
    } else {
        "Uncategorized"
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recurring {
    pub name: String,
    pub key: String,
    pub amount: f64,
    pub frequency: Frequency,
    pub count: usize,
    pub last: NaiveDate,
    pub next: NaiveDate,
    pub category: String,
    /// True when every charge was the same amount (subscriptions, loan payments).
    pub fixed_amount: bool,
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// Find payments that repeat on a regular schedule.
pub fn detect_recurring(txns: &[Txn]) -> Vec<Recurring> {
    let mut groups: BTreeMap<String, Vec<&Txn>> = BTreeMap::new();
    for t in txns.iter().filter(|t| t.is_spending()) {
        groups.entry(merchant_key(&t.description)).or_default().push(t);
    }

    let mut out = Vec::new();
    for (key, mut list) in groups {
        list.sort_by_key(|t| t.date);
        // Same-day duplicates (split charges) count once.
        list.dedup_by_key(|t| t.date);
        if list.len() < 2 {
            continue;
        }
        let mut gaps: Vec<f64> = list.windows(2).map(|w| (w[1].date - w[0].date).num_days() as f64).collect();
        let gap = median(&mut gaps.clone());
        let (freq, tol, min_count) = if (6.0..=8.0).contains(&gap) {
            (Frequency::Weekly, 2.0, 3)
        } else if (13.0..=16.0).contains(&gap) {
            (Frequency::Biweekly, 3.0, 3)
        } else if (26.0..=35.0).contains(&gap) {
            (Frequency::Monthly, 5.0, 2)
        } else if (84.0..=97.0).contains(&gap) {
            (Frequency::Quarterly, 10.0, 2)
        } else if (350.0..=380.0).contains(&gap) {
            (Frequency::Yearly, 15.0, 2)
        } else {
            continue;
        };
        if list.len() < min_count {
            continue;
        }
        // Most gaps should match the schedule (allow one missed or doubled charge).
        let regular = gaps.iter().filter(|g| (**g - gap).abs() <= tol).count();
        if (regular as f64) < (gaps.len() as f64 * 0.7).ceil() {
            continue;
        }
        let mut amounts: Vec<f64> = list.iter().map(|t| t.spent).collect();
        let med = median(&mut amounts);
        let (lo, hi) = amounts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), a| (lo.min(*a), hi.max(*a)));
        let fixed_amount = (hi - lo) < 0.01;
        // Grocery runs vary a lot; bills don't. Utilities can swing ~30%, so allow that
        // when there are 3+ charges, but demand near-identical amounts when there are only 2.
        let allowed = if list.len() >= 3 { 0.35 } else { 0.15 };
        if !fixed_amount && (hi - lo) > med.max(1.0) * allowed {
            continue;
        }
        gaps.clear();
        let last = list.last().unwrap().date;
        let next = freq.nth(last, 1).unwrap_or(last + Duration::days(30));
        let name = title_case(&key);
        out.push(Recurring {
            category: guess_category(&list.last().unwrap().description).to_string(),
            name,
            key,
            amount: (med * 100.0).round() / 100.0,
            frequency: freq,
            count: list.len(),
            last,
            next,
            fixed_amount,
        });
    }
    out.sort_by(|a, b| b.amount.total_cmp(&a.amount));
    out
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonthTotals {
    pub money_in: f64,
    pub money_out: f64,
    /// Moved between your own accounts; not counted in or out.
    pub transfers: f64,
}

/// Money in and out per calendar month (key = first of month).
pub fn monthly_totals(txns: &[Txn]) -> BTreeMap<NaiveDate, MonthTotals> {
    let mut m: BTreeMap<NaiveDate, MonthTotals> = BTreeMap::new();
    for t in txns {
        let k = NaiveDate::from_ymd_opt(t.date.year(), t.date.month(), 1).unwrap();
        let e = m.entry(k).or_default();
        if t.transfer {
            e.transfers += t.spent.abs();
        } else if t.spent > 0.0 {
            e.money_out += t.spent;
        } else {
            e.money_in += -t.spent;
        }
    }
    m
}

/// Spending per guessed category, largest first.
pub fn by_category(txns: &[Txn]) -> Vec<(String, f64)> {
    let mut m: BTreeMap<&str, f64> = BTreeMap::new();
    for t in txns.iter().filter(|t| t.is_spending()) {
        *m.entry(guess_category(&t.description)).or_default() += t.spent;
    }
    let mut v: Vec<(String, f64)> = m.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

/// Spending per merchant, largest first.
pub fn by_merchant(txns: &[Txn]) -> Vec<(String, f64, usize)> {
    let mut m: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for t in txns.iter().filter(|t| t.is_spending()) {
        let e = m.entry(merchant_key(&t.description)).or_default();
        e.0 += t.spent;
        e.1 += 1;
    }
    let mut v: Vec<(String, f64, usize)> = m.into_iter().map(|(k, (a, n))| (title_case(&k), a, n)).collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capital_one_style_debit_credit() {
        let csv = "Transaction Date,Posted Date,Card No.,Description,Category,Debit,Credit\n\
                   2026-09-03,2026-09-04,1840,SPOTIFY USA,Entertainment,11.99,\n\
                   2026-10-03,2026-10-04,1840,SPOTIFY USA,Entertainment,11.99,\n\
                   2026-10-10,2026-10-11,1840,CAPITAL ONE AUTOPAY,Payment,,250.00\n";
        let t = parse_statement(csv).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].spent, 11.99);
        assert_eq!(t[2].spent, -250.0);
        let r = detect_recurring(&t);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].frequency, Frequency::Monthly);
        assert_eq!(r[0].next, NaiveDate::from_ymd_opt(2026, 11, 3).unwrap());
        assert_eq!(r[0].category, "Monthly Subscriptions");
    }

    #[test]
    fn bank_style_negative_amounts() {
        let csv = "Date,Description,Amount\n\
                   08/01/2026,ACH DEBIT GEICO AUTO PMT 123,-98.40\n\
                   08/15/2026,PAYROLL DEPOSIT,1280.00\n\
                   08/20/2026,KING SOOPERS #12 DENVER CO,-84.17\n\
                   09/01/2026,ACH DEBIT GEICO AUTO PMT 456,-98.40\n\
                   09/22/2026,KING SOOPERS #31 DENVER CO,-131.02\n\
                   10/01/2026,ACH DEBIT GEICO AUTO PMT 789,-98.40\n";
        let t = parse_statement(csv).unwrap();
        assert!(t.iter().any(|x| x.spent < 0.0 && x.description.contains("PAYROLL")));
        let r = detect_recurring(&t);
        let geico = r.iter().find(|x| x.key == "GEICO AUTO").expect("geico found");
        assert_eq!(geico.amount, 98.40);
        assert_eq!(geico.category, "Transportation");
        assert_eq!(geico.count, 3);
        assert!(r.iter().all(|x| !x.key.starts_with("KING")), "irregular grocery trips aren't bills");
        let m = monthly_totals(&t);
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn no_header_rows() {
        let csv = "\"10/01/2026\",\"-15.00\",\"*\",\"\",\"PROTON AG GENEVA\"\n\"11/01/2026\",\"-15.00\",\"*\",\"\",\"PROTON AG GENEVA\"\n";
        let t = parse_statement(csv).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(detect_recurring(&t).len(), 1);
    }

    #[test]
    fn merchant_cleanup() {
        assert_eq!(merchant_key("SQ *BLUE BOTTLE #1234 DENVER CO"), "BLUE BOTTLE");
        assert_eq!(merchant_key("NETFLIX.COM 866-579-7172 CA"), "NETFLIX");
    }
}

#[cfg(test)]
mod sample {
    use super::*;
    #[test]
    fn sample_statement_finds_bills() {
        let t = super::parse_statement(include_str!("../tests/sample_statement.csv")).unwrap();
        let r = super::detect_recurring(&t);
        let names: Vec<&str> = r.iter().map(|x| x.key.as_str()).collect();
        for want in ["RENT PAYMENT", "SPOTIFY", "GEICO AUTO", "XCEL ENERGY", "PROTON AG"] {
            assert!(names.contains(&want), "missing {want}: {names:?}");
        }
        assert!(!names.iter().any(|n| n.starts_with("KING") || n.starts_with("CORVUS") || n.starts_with("CONOCO")), "{names:?}");
    }

    #[test]
    fn paypal_holds_and_transfers_are_not_income_or_spending() {
        let text = "\u{feff}\"Date\",\"Time\",\"TimeZone\",\"Name\",\"Type\",\"Status\",\"Currency\",\"Amount\",\"Fees\",\"Total\"
\"08/01/2026\",\"10:00:00\",\"MDT\",\"GROCERY MART\",\"Account Hold for Open Authorization\",\"Pending\",\"USD\",\"-50.00\",\"0.00\",\"-50.00\"
\"08/01/2026\",\"10:00:01\",\"MDT\",\"\",\"Electronic Funds Transfer Funding\",\"Completed\",\"USD\",\"50.00\",\"0.00\",\"50.00\"
\"08/02/2026\",\"09:00:00\",\"MDT\",\"GROCERY MART\",\"Reversal of General Account Hold\",\"Completed\",\"USD\",\"50.00\",\"0.00\",\"50.00\"
\"08/02/2026\",\"09:00:01\",\"MDT\",\"GROCERY MART\",\"General PayPal Debit Card Transaction\",\"Completed\",\"USD\",\"-50.00\",\"0.00\",\"-50.00\"
\"08/05/2026\",\"09:00:00\",\"MDT\",\"STREAMING CO\",\"PreApproved Payment Bill User Payment\",\"Completed\",\"USD\",\"-15.99\",\"0.00\",\"-15.99\"
\"08/06/2026\",\"09:00:00\",\"MDT\",\"\",\"General Card Withdrawal\",\"Completed\",\"USD\",\"-100.00\",\"0.00\",\"-100.00\"
\"08/07/2026\",\"09:00:00\",\"MDT\",\"A FRIEND\",\"Mobile Payment\",\"Completed\",\"USD\",\"20.00\",\"0.00\",\"20.00\"
";
        let t = parse_statement(text).unwrap();
        // The pending hold and its reversal are skipped entirely.
        assert_eq!(t.len(), 5);
        let m = monthly_totals(&t);
        let aug = m.values().next().unwrap();
        assert!((aug.money_out - 65.99).abs() < 0.001, "out {}", aug.money_out);
        // Direct deposit (EFT Funding) + a friend's payment.
        assert!((aug.money_in - 70.0).abs() < 0.001, "in {}", aug.money_in);
        assert!((aug.transfers - 100.0).abs() < 0.001, "transfers {}", aug.transfers);
    }

    #[test]
    fn card_payments_are_transfers() {
        assert!(is_transfer("", "CAPITAL ONE CARD PYMT"));
        assert!(is_transfer("", "ONLINE TRANSFER TO SAVINGS"));
        assert!(!is_transfer("", "KING SOOPERS #123"));
    }
}
