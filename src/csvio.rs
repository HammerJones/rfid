//! CSV import and export.
//!
//! Export formats:
//! * Monthly summary: the same layout as the original "Monthly Expense / Rent Calculator" sheet.
//! * Expense list: one row per expense, re-importable (also works as a template).
//!
//! Import formats (detected automatically):
//! * The monthly summary sheet itself.
//! * An expense list with headers such as name, category, amount, frequency, start date.

use crate::model::{Budget, Expense, Frequency, IncomeMode, MonthSummary};
use chrono::NaiveDate;

pub const COLS: usize = 6;

/// "$1,109.00" style formatting, as in the spreadsheet.
pub fn money(v: f64) -> String {
    let neg = v < -0.004;
    let cents = (v.abs() * 100.0).round() as u64;
    let whole = cents / 100;
    let frac = cents % 100;
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${}.{:02}", if neg { "-" } else { "" }, grouped, frac)
}

/// Parse "$1,109.00", "1109.00", "(25.00)", "-25" into a number.
pub fn parse_money(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("n/a") {
        return None;
    }
    let neg = t.starts_with('-') || (t.starts_with('(') && t.ends_with(')'));
    let cleaned: String = t
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v: f64 = cleaned.parse().ok()?;
    Some(if neg { -v } else { v })
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    for fmt in ["%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y", "%Y/%m/%d", "%m-%d-%Y", "%b %d, %Y", "%B %d, %Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, fmt) {
            return Some(d);
        }
    }
    None
}

fn goal_label(goal: f64) -> String {
    if goal >= 1000.0 && (goal % 1000.0).abs() < 1e-9 {
        format!("Months to ${}k", (goal / 1000.0) as u64)
    } else {
        format!("Months to {}", money(goal))
    }
}

fn write_rows(rows: &[Vec<String>]) -> String {
    let mut w = csv::WriterBuilder::new().flexible(true).from_writer(vec![]);
    for r in rows {
        let mut r = r.clone();
        r.resize(COLS.max(r.len()), String::new());
        w.write_record(&r).expect("write to memory");
    }
    String::from_utf8(w.into_inner().expect("flush")).expect("utf8")
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|s| s.to_string()).collect()
}

/// Monthly summary in the spreadsheet's layout.
pub fn export_summary(budget: &Budget, s: &MonthSummary) -> String {
    let st = &budget.settings;
    let mut rows: Vec<Vec<String>> = Vec::new();
    rows.push(row(&[&format!("Monthly Costs - {}", s.month_start.format("%B %Y"))]));

    let header_tail = ["Monthly Cost", "Remaining Balance", "Remaining Payments"];
    for (ci, cat) in s.categories.iter().enumerate() {
        if ci == 0 {
            // The sheet puts the note and the column headings on the first category's row.
            rows.push(row(&[&st.note, &cat.name, "", header_tail[0], header_tail[1], header_tail[2]]));
        } else {
            rows.push(row(&["", &cat.name]));
        }
        for it in &cat.items {
            let bal = it.remaining_balance.map(money).unwrap_or_else(|| "n/a".into());
            let pay = it.remaining_payments.map(|p| p.to_string()).unwrap_or_else(|| "n/a".into());
            rows.push(row(&["", "", &it.name, &money(it.monthly_cost), &bal, &pay]));
        }
    }
    if s.categories.is_empty() {
        rows.push(row(&[&st.note, "", "", header_tail[0], header_tail[1], header_tail[2]]));
    }

    let months = s.months_to_goal.map(|m| m.to_string()).unwrap_or_else(|| "n/a".into());
    rows.push(row(&["", "", "Monthly Payment", &goal_label(st.savings_goal)]));
    rows.push(row(&["", "Emergency Fund/Savings", &money(s.savings), &months]));
    match st.income_mode {
        IncomeMode::Hourly => {
            rows.push(row(&["", "Current Hourly Income", &money(st.hourly_rate), "Monthly Income:"]));
            rows.push(row(&["", "Current Hours/week", &format!("{:.2}", st.hours_per_week), &money(s.income)]));
        }
        IncomeMode::Fixed => {
            rows.push(row(&["", "Monthly Income (after tax)", &money(s.income), "Monthly Income:"]));
            rows.push(row(&["", "", "", &money(s.income)]));
        }
    }
    // Totals: not in the original sheet, appended at the bottom so the layout above is unchanged.
    rows.push(row(&["", "Total Expenses", &money(s.total_expenses)]));
    rows.push(row(&["", "Left Over", &money(s.left_over)]));
    write_rows(&rows)
}

pub const LIST_HEADERS: [&str; 9] = [
    "name",
    "category",
    "amount",
    "frequency",
    "start_date",
    "remaining_balance",
    "remaining_payments",
    "is_remainder",
    "notes",
];

/// Every expense, one per row. Re-importable.
pub fn export_expense_list(budget: &Budget) -> String {
    let mut rows = vec![LIST_HEADERS.iter().map(|s| s.to_string()).collect::<Vec<_>>()];
    for e in &budget.expenses {
        rows.push(vec![
            e.name.clone(),
            e.category.clone(),
            format!("{:.2}", e.amount),
            e.frequency.label().to_string(),
            e.start.format("%Y-%m-%d").to_string(),
            e.remaining_balance.map(|b| format!("{b:.2}")).unwrap_or_default(),
            e.remaining_payments.map(|p| p.to_string()).unwrap_or_default(),
            if e.is_remainder { "yes".into() } else { String::new() },
            e.notes.clone(),
        ]);
    }
    let mut w = csv::Writer::from_writer(vec![]);
    for r in rows {
        w.write_record(&r).expect("write");
    }
    String::from_utf8(w.into_inner().expect("flush")).expect("utf8")
}

/// What an import produced.
#[derive(Debug, Default)]
pub struct ImportResult {
    pub expenses: Vec<Expense>,
    /// Settings found in the file (summary-sheet imports only).
    pub settings: Option<crate::model::Settings>,
    pub warnings: Vec<String>,
    pub kind: &'static str,
}

fn read_records(text: &str) -> Result<Vec<Vec<String>>, String> {
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

fn cell(r: &[String], i: usize) -> &str {
    r.get(i).map(|s| s.as_str()).unwrap_or("")
}

/// Import a CSV. `default_start` is used when the file has no dates (e.g. the summary sheet).
pub fn import(text: &str, base: &Budget, default_start: NaiveDate) -> Result<ImportResult, String> {
    let rows = read_records(text)?;
    if rows.is_empty() {
        return Err("The file is empty.".into());
    }
    let looks_like_sheet = cell(&rows[0], 0).to_ascii_lowercase().starts_with("monthly costs")
        || rows.iter().take(3).any(|r| cell(r, 5).eq_ignore_ascii_case("remaining payments"));
    if looks_like_sheet {
        import_sheet(&rows, base, default_start)
    } else {
        import_list(&rows, default_start)
    }
}

fn blank_expense(name: &str, category: &str, amount: f64, start: NaiveDate) -> Expense {
    Expense {
        id: 0,
        name: name.to_string(),
        category: category.to_string(),
        amount,
        frequency: Frequency::Monthly,
        start,
        remaining_payments: None,
        remaining_balance: None,
        is_remainder: false,
        notes: String::new(),
    }
}

fn import_sheet(rows: &[Vec<String>], base: &Budget, start: NaiveDate) -> Result<ImportResult, String> {
    let mut res = ImportResult { kind: "monthly summary sheet", ..Default::default() };
    let mut settings = base.settings.clone();
    let mut category = String::from("Uncategorized");
    let mut sheet_income: Option<f64> = None;
    let mut sheet_savings: Option<f64> = None;
    let mut hourly: Option<f64> = None;
    let mut hours: Option<f64> = None;
    let mut in_savings_block = false;

    for (i, r) in rows.iter().enumerate() {
        let (a, b, c, dcol) = (cell(r, 0), cell(r, 1), cell(r, 2), cell(r, 3));
        if i == 0 && a.to_ascii_lowercase().starts_with("monthly costs") {
            continue;
        }
        if !a.is_empty() && i <= 2 {
            settings.note = a.to_string();
        }
        let bl = b.to_ascii_lowercase();
        if c.eq_ignore_ascii_case("monthly payment") {
            in_savings_block = true;
            if let Some(goal) = parse_goal(dcol) {
                settings.savings_goal = goal;
            }
            continue;
        }
        if bl.starts_with("total expenses") || bl.starts_with("left over") {
            continue;
        }
        if in_savings_block || bl.contains("savings") || bl.contains("hourly") || bl.contains("hours") || bl.contains("income") {
            if bl.contains("savings") || bl.contains("emergency") {
                sheet_savings = parse_money(c);
            } else if bl.contains("hourly") {
                hourly = parse_money(c);
            } else if bl.contains("hours") {
                hours = parse_money(c);
                sheet_income = parse_money(dcol).or(sheet_income);
            } else if bl.contains("monthly income") {
                sheet_income = parse_money(c).or(sheet_income);
            }
            continue;
        }
        if !b.is_empty() && c.is_empty() {
            category = b.to_string();
            continue;
        }
        if !b.is_empty() && !c.is_empty() && dcol.eq_ignore_ascii_case("monthly cost") {
            // First category row also carries the column headings.
            category = b.to_string();
            continue;
        }
        if b.is_empty() && !c.is_empty() {
            let Some(amount) = parse_money(dcol) else {
                res.warnings.push(format!("Skipped \"{c}\": no monthly cost."));
                continue;
            };
            let mut e = blank_expense(c, &category, amount, start);
            e.remaining_balance = parse_money(cell(r, 4));
            e.remaining_payments = cell(r, 5).trim().parse::<u32>().ok();
            res.expenses.push(e);
        }
    }

    // Recover the income formula from the sheet.
    if let (Some(rate), Some(hrs)) = (hourly, hours) {
        settings.income_mode = IncomeMode::Hourly;
        settings.hourly_rate = rate;
        settings.hours_per_week = hrs;
        if let Some(inc) = sheet_income {
            let gross = rate * hrs * settings.weeks_per_month;
            if gross > 0.0 {
                settings.tax_rate = ((1.0 - inc / gross) * 10_000.0).round() / 10_000.0;
            }
        }
    } else if let Some(inc) = sheet_income {
        settings.income_mode = IncomeMode::Fixed;
        settings.fixed_monthly_income = inc;
    }
    let income = settings.monthly_income();
    if let Some(sav) = sheet_savings {
        if income > 0.0 {
            settings.savings_rate = ((sav / income) * 10_000.0).round() / 10_000.0;
        }
    }

    // The sheet's rent is "income - savings - everything else". When the sheet balances to the
    // cent, one line is computed that way: take the rent/housing line, or else the largest one.
    let total: f64 = res.expenses.iter().map(|e| e.amount).sum();
    let savings = settings.monthly_savings();
    let balances = (income - savings - total).abs() < 0.02;
    let pick = res
        .expenses
        .iter()
        .position(|e| e.name.to_ascii_lowercase().contains("rent"))
        .or_else(|| {
            res.expenses
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.amount.total_cmp(&b.1.amount))
                .map(|(i, _)| i)
        });
    if let (true, Some(idx)) = (balances, pick) {
        res.expenses[idx].is_remainder = true;
        res.warnings.push(format!(
            "\"{}\" equals income minus savings minus everything else, so it was set as the left-over line.",
            res.expenses[idx].name
        ));
    }
    if res.expenses.is_empty() {
        return Err("No expenses found in this sheet.".into());
    }
    res.settings = Some(settings);
    Ok(res)
}

fn parse_goal(s: &str) -> Option<f64> {
    let lower = s.to_ascii_lowercase();
    let idx = lower.find('$')?;
    let rest = &lower[idx + 1..];
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    let v: f64 = num.replace(',', "").parse().ok()?;
    let after = &rest[num.len()..];
    Some(if after.starts_with('k') { v * 1000.0 } else { v })
}

fn find_col(headers: &[String], names: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        let h = h.to_ascii_lowercase().replace(['_', '-'], " ");
        names.iter().any(|n| h == *n || h.starts_with(n))
    })
}

fn import_list(rows: &[Vec<String>], default_start: NaiveDate) -> Result<ImportResult, String> {
    let mut res = ImportResult { kind: "expense list", ..Default::default() };
    let headers = &rows[0];
    let name_c = find_col(headers, &["name", "description", "expense", "item", "payee"])
        .ok_or("Couldn't find a name column. Expected a header like \"name\" or \"description\".")?;
    let amount_c = find_col(headers, &["amount", "monthly cost", "cost", "price", "payment"])
        .ok_or("Couldn't find an amount column. Expected a header like \"amount\".")?;
    let cat_c = find_col(headers, &["category", "type", "group"]);
    let freq_c = find_col(headers, &["frequency", "repeat", "recurrence", "schedule"]);
    let date_c = find_col(headers, &["start date", "start", "due date", "date", "next due"]);
    let bal_c = find_col(headers, &["remaining balance", "balance"]);
    let pay_c = find_col(headers, &["remaining payments", "payments left", "payments"]);
    let rem_c = find_col(headers, &["is remainder", "remainder"]);
    let notes_c = find_col(headers, &["notes", "note", "memo"]);

    for (i, r) in rows.iter().enumerate().skip(1) {
        let name = cell(r, name_c);
        if name.is_empty() {
            continue;
        }
        let Some(amount) = parse_money(cell(r, amount_c)) else {
            res.warnings.push(format!("Row {}: skipped \"{name}\" (amount not a number).", i + 1));
            continue;
        };
        let category = cat_c.map(|c| cell(r, c)).filter(|s| !s.is_empty()).unwrap_or("Uncategorized");
        let start = match date_c.map(|c| cell(r, c)).filter(|s| !s.is_empty()) {
            Some(s) => parse_date(s).unwrap_or_else(|| {
                res.warnings.push(format!("Row {}: couldn't read date \"{s}\", used {default_start}.", i + 1));
                default_start
            }),
            None => default_start,
        };
        let mut e = blank_expense(name, category, amount.abs(), start);
        if let Some(c) = freq_c {
            let f = cell(r, c);
            e.frequency = Frequency::parse(f).unwrap_or_else(|| {
                res.warnings.push(format!("Row {}: unknown frequency \"{f}\", used Monthly.", i + 1));
                Frequency::Monthly
            });
        }
        e.remaining_balance = bal_c.and_then(|c| parse_money(cell(r, c)));
        e.remaining_payments = pay_c.and_then(|c| cell(r, c).parse::<u32>().ok());
        e.is_remainder = rem_c
            .map(|c| matches!(cell(r, c).to_ascii_lowercase().as_str(), "yes" | "true" | "1" | "y"))
            .unwrap_or(false);
        e.notes = notes_c.map(|c| cell(r, c).to_string()).unwrap_or_default();
        res.expenses.push(e);
    }
    if res.expenses.is_empty() {
        return Err("No expenses found in this file.".into());
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Budget;

    const SHEET: &str = include_str!("../tests/sample_sheet.csv");

    fn nov() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
    }

    #[test]
    fn money_format() {
        assert_eq!(money(1109.0), "$1,109.00");
        assert_eq!(money(3600.0), "$3,600.00");
        assert_eq!(money(30.0), "$30.00");
        assert_eq!(money(-25.5), "-$25.50");
        assert_eq!(parse_money("\"$3,600.00\"".trim_matches('"')), Some(3600.0));
    }

    #[test]
    fn imports_sample_sheet_and_round_trips() {
        let base = Budget::default();
        let res = import(SHEET, &base, nov()).unwrap();
        assert_eq!(res.kind, "monthly summary sheet");
        assert_eq!(res.expenses.len(), 10);
        let s = res.settings.clone().unwrap();
        assert!((s.monthly_income() - 2560.0).abs() < 1e-6);
        assert!((s.savings_rate - 0.10).abs() < 1e-9);
        assert_eq!(s.savings_goal, 5000.0);
        let rent = res.expenses.iter().find(|e| e.name == "Rent").unwrap();
        assert!(rent.is_remainder);

        let mut b = Budget { settings: s, ..Default::default() };
        for e in res.expenses {
            b.add_expense(e);
        }
        let out = export_summary(&b, &b.summarize(nov()));
        // Every data line of the original appears unchanged in the export.
        for line in SHEET.lines().skip(1) {
            assert!(out.contains(line.trim_end()), "missing line: {line}\n---\n{out}");
        }
    }

    #[test]
    fn imports_expense_list() {
        let csv = "Name,Category,Amount,Frequency,Start Date,Payments Left\nGym,Health,$40,monthly,11/15/2026,\nPaycheck tax,Other,25,weekly,2026-11-06,3\n";
        let res = import(csv, &Budget::default(), nov()).unwrap();
        assert_eq!(res.expenses.len(), 2);
        assert_eq!(res.expenses[1].frequency, Frequency::Weekly);
        assert_eq!(res.expenses[1].remaining_payments, Some(3));
        assert_eq!(res.expenses[0].start, NaiveDate::from_ymd_opt(2026, 11, 15).unwrap());
    }

    #[test]
    fn list_round_trip() {
        let res = import(SHEET, &Budget::default(), nov()).unwrap();
        let mut b = Budget::default();
        for e in res.expenses {
            b.add_expense(e);
        }
        let list = export_expense_list(&b);
        let back = import(&list, &Budget::default(), nov()).unwrap();
        assert_eq!(back.expenses.len(), b.expenses.len());
        assert!(back.expenses.iter().any(|e| e.is_remainder && e.name == "Rent"));
    }
}
