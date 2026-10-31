//! Budget data model and calculations. No UI code lives here, so it is easy to test.

use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// How often an expense repeats.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Frequency {
    Once,
    Weekly,
    Biweekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl Frequency {
    pub const ALL: [Frequency; 6] = [
        Frequency::Once,
        Frequency::Weekly,
        Frequency::Biweekly,
        Frequency::Monthly,
        Frequency::Quarterly,
        Frequency::Yearly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Frequency::Once => "One-time",
            Frequency::Weekly => "Weekly",
            Frequency::Biweekly => "Every 2 weeks",
            Frequency::Monthly => "Monthly",
            Frequency::Quarterly => "Every 3 months",
            Frequency::Yearly => "Yearly",
        }
    }

    /// Parse loose user text such as "monthly", "bi-weekly", "once".
    pub fn parse(s: &str) -> Option<Frequency> {
        let s = s.trim().to_ascii_lowercase().replace(['-', ' ', '_'], "");
        Some(match s.as_str() {
            "once" | "onetime" | "single" | "oneoff" => Frequency::Once,
            "weekly" | "week" | "1w" => Frequency::Weekly,
            "biweekly" | "every2weeks" | "fortnightly" | "2w" => Frequency::Biweekly,
            "monthly" | "month" | "1m" | "" => Frequency::Monthly,
            "quarterly" | "every3months" | "3m" => Frequency::Quarterly,
            "yearly" | "annual" | "annually" | "year" | "1y" => Frequency::Yearly,
            _ => return None,
        })
    }

    /// Date of the `n`-th occurrence (0-based) for a schedule starting on `start`.
    pub fn nth(self, start: NaiveDate, n: u32) -> Option<NaiveDate> {
        match self {
            Frequency::Once => (n == 0).then_some(start),
            Frequency::Weekly => start.checked_add_signed(Duration::weeks(n as i64)),
            Frequency::Biweekly => start.checked_add_signed(Duration::weeks(2 * n as i64)),
            Frequency::Monthly => start.checked_add_months(Months::new(n)),
            Frequency::Quarterly => start.checked_add_months(Months::new(3 * n)),
            Frequency::Yearly => start.checked_add_months(Months::new(12 * n)),
        }
    }
}

/// A scheduled expense (a bill, subscription, loan payment, or spending allowance).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Expense {
    pub id: u64,
    pub name: String,
    pub category: String,
    /// Amount charged each time it comes due.
    pub amount: f64,
    pub frequency: Frequency,
    /// First due date.
    pub start: NaiveDate,
    /// Payments left, counted from `start` (including the payment on `start`).
    /// When set, the expense stops after this many payments.
    #[serde(default)]
    pub remaining_payments: Option<u32>,
    /// Balance owed as of `start` (before that day's payment). Shown counting down.
    #[serde(default)]
    pub remaining_balance: Option<f64>,
    /// "Whatever is left" line, like the max-rent estimate in the spreadsheet:
    /// its monthly cost is income minus savings minus every other expense.
    #[serde(default)]
    pub is_remainder: bool,
    #[serde(default)]
    pub notes: String,
}

impl Expense {
    /// Upper bound on how many payments this expense will ever have.
    pub fn max_payments(&self) -> Option<u32> {
        match (self.frequency, self.remaining_payments) {
            (Frequency::Once, _) => Some(1),
            (_, Some(n)) => Some(n),
            _ => None,
        }
    }

    /// All due dates within `from..=to`.
    pub fn due_dates_between(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let mut out = Vec::new();
        let max = self.max_payments().unwrap_or(u32::MAX);
        let mut n = 0u32;
        while n < max && n < 200_000 {
            let Some(d) = self.frequency.nth(self.start, n) else { break };
            if d > to {
                break;
            }
            if d >= from {
                out.push(d);
            }
            n += 1;
        }
        out
    }

    /// Number of payments due strictly before `date`.
    pub fn payments_before(&self, date: NaiveDate) -> u32 {
        if date <= self.start {
            return 0;
        }
        self.due_dates_between(self.start, date.pred_opt().unwrap_or(date)).len() as u32
    }

    /// Payments still owed at the start of the given day.
    pub fn remaining_payments_on(&self, date: NaiveDate) -> Option<u32> {
        self.remaining_payments
            .map(|n| n.saturating_sub(self.payments_before(date)))
    }

    /// Balance still owed at the start of the given day.
    pub fn remaining_balance_on(&self, date: NaiveDate) -> Option<f64> {
        self.remaining_balance
            .map(|b| (b - self.amount * self.payments_before(date) as f64).max(0.0))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum IncomeMode {
    /// Hourly rate x hours per week x weeks per month, minus tax.
    Hourly,
    /// A fixed after-tax monthly amount.
    Fixed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub income_mode: IncomeMode,
    pub hourly_rate: f64,
    pub hours_per_week: f64,
    /// 4 keeps the math simple. The true average is about 4.33.
    pub weeks_per_month: f64,
    /// Fraction withheld for taxes, e.g. 0.20 for 20%.
    pub tax_rate: f64,
    /// After-tax monthly income when `income_mode` is `Fixed`.
    pub fixed_monthly_income: f64,
    /// Fraction of after-tax income put into savings, e.g. 0.10.
    pub savings_rate: f64,
    pub savings_goal: f64,
    pub current_savings: f64,
    /// Free-text note shown at the top of the exported summary.
    pub note: String,
    /// Extra space at the top of the screen for the phone's status bar and camera cutout.
    #[serde(default = "default_safe_top")]
    pub safe_top: f32,
    /// Extra space at the bottom for the phone's gesture/navigation bar.
    #[serde(default = "default_safe_bottom")]
    pub safe_bottom: f32,
}

fn default_safe_top() -> f32 {
    if cfg!(target_os = "android") { 48.0 } else { 0.0 }
}

fn default_safe_bottom() -> f32 {
    if cfg!(target_os = "android") { 24.0 } else { 0.0 }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            income_mode: IncomeMode::Hourly,
            hourly_rate: 20.0,
            hours_per_week: 40.0,
            weeks_per_month: 4.0,
            tax_rate: 0.20,
            fixed_monthly_income: 3000.0,
            savings_rate: 0.10,
            savings_goal: 5000.0,
            current_savings: 0.0,
            note: "Sample budget with made-up numbers. Import your own spreadsheet from Import / Export to replace it.".into(),
            safe_top: default_safe_top(),
            safe_bottom: default_safe_bottom(),
        }
    }
}

impl Settings {
    pub fn monthly_income(&self) -> f64 {
        match self.income_mode {
            IncomeMode::Hourly => {
                self.hourly_rate * self.hours_per_week * self.weeks_per_month * (1.0 - self.tax_rate)
            }
            IncomeMode::Fixed => self.fixed_monthly_income,
        }
    }

    pub fn monthly_savings(&self) -> f64 {
        self.monthly_income() * self.savings_rate
    }

    /// Whole months until the savings goal is reached, or `None` if never.
    pub fn months_to_goal(&self) -> Option<u32> {
        let need = (self.savings_goal - self.current_savings).max(0.0);
        if need <= 0.0 {
            return Some(0);
        }
        let per = self.monthly_savings();
        if per <= 0.0 {
            return None;
        }
        Some((need / per - 1e-9).ceil() as u32)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Budget {
    pub settings: Settings,
    /// Category display order.
    pub categories: Vec<String>,
    pub expenses: Vec<Expense>,
    pub next_id: u64,
    /// Daily spending budget settings.
    #[serde(default)]
    pub daily: DailySettings,
}

/// Settings for the daily spending budget and auto-tracked purchases.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DailySettings {
    /// Monthly spending money to spread across the days. 0 = work it out from the budget.
    #[serde(default)]
    pub monthly_override: f64,
    /// Purchase ids that shouldn't count against the daily budget.
    #[serde(default)]
    pub ignored: Vec<String>,
    /// Merchants paid as bills (already in the budget), as merchant keys like "COLORADO STATION".
    #[serde(default)]
    pub bill_merchants: Vec<String>,
    /// Turn off the always-on daily budget notification (Android).
    #[serde(default)]
    pub hide_notification: bool,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            settings: Settings::default(),
            categories: vec![],
            expenses: vec![],
            next_id: 1,
            daily: DailySettings::default(),
        }
    }
}

/// One line of a monthly summary.
#[derive(Clone, Debug, PartialEq)]
pub struct LineItem {
    pub expense_id: u64,
    pub name: String,
    pub monthly_cost: f64,
    pub due_dates: Vec<NaiveDate>,
    pub remaining_balance: Option<f64>,
    pub remaining_payments: Option<u32>,
    pub is_remainder: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CategorySummary {
    pub name: String,
    pub items: Vec<LineItem>,
}

impl CategorySummary {
    pub fn total(&self) -> f64 {
        self.items.iter().map(|i| i.monthly_cost).sum()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonthSummary {
    pub month_start: NaiveDate,
    pub income: f64,
    pub savings: f64,
    pub months_to_goal: Option<u32>,
    pub categories: Vec<CategorySummary>,
    /// All expenses including any remainder line.
    pub total_expenses: f64,
    /// Income minus savings minus all expenses. Zero when a remainder line absorbs it.
    pub left_over: f64,
}

pub fn month_start(year: i32, month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, 1).expect("valid month")
}

pub fn month_end(start: NaiveDate) -> NaiveDate {
    start
        .checked_add_months(Months::new(1))
        .and_then(|d| d.pred_opt())
        .expect("valid month end")
}

pub fn add_months(start: NaiveDate, delta: i32) -> NaiveDate {
    if delta >= 0 {
        start.checked_add_months(Months::new(delta as u32)).unwrap_or(start)
    } else {
        start.checked_sub_months(Months::new((-delta) as u32)).unwrap_or(start)
    }
}

impl Budget {
    pub fn add_expense(&mut self, mut e: Expense) -> u64 {
        e.id = self.next_id;
        self.next_id += 1;
        self.ensure_category(&e.category);
        let id = e.id;
        self.expenses.push(e);
        id
    }

    pub fn ensure_category(&mut self, name: &str) {
        if !name.is_empty() && !self.categories.iter().any(|c| c == name) {
            self.categories.push(name.to_string());
        }
    }

    /// Categories in display order, including any used by expenses but missing from the list.
    pub fn ordered_categories(&self) -> Vec<String> {
        let mut cats = self.categories.clone();
        for e in &self.expenses {
            if !cats.contains(&e.category) {
                cats.push(e.category.clone());
            }
        }
        cats
    }

    /// Build the summary for the month that contains `any_day`.
    pub fn summarize(&self, any_day: NaiveDate) -> MonthSummary {
        let start = month_start(any_day.year(), any_day.month());
        let end = month_end(start);
        let income = self.settings.monthly_income();
        let savings = self.settings.monthly_savings();

        let mut categories = Vec::new();
        let mut fixed_total = 0.0;
        for cat in self.ordered_categories() {
            let mut items = Vec::new();
            for e in self.expenses.iter().filter(|e| e.category == cat) {
                let due = e.due_dates_between(start, end);
                if !e.is_remainder && due.is_empty() {
                    continue; // not active this month
                }
                let cost = if e.is_remainder { 0.0 } else { e.amount * due.len() as f64 };
                fixed_total += cost;
                items.push(LineItem {
                    expense_id: e.id,
                    name: e.name.clone(),
                    monthly_cost: cost,
                    due_dates: due,
                    remaining_balance: e.remaining_balance_on(start),
                    remaining_payments: e.remaining_payments_on(start),
                    is_remainder: e.is_remainder,
                });
            }
            if !items.is_empty() {
                categories.push(CategorySummary { name: cat, items });
            }
        }

        // Spread whatever is left over across remainder lines (normally just one, e.g. rent).
        let left = income - savings - fixed_total;
        let remainder_count = categories
            .iter()
            .flat_map(|c| &c.items)
            .filter(|i| i.is_remainder)
            .count();
        if remainder_count > 0 {
            let share = left / remainder_count as f64;
            for item in categories.iter_mut().flat_map(|c| c.items.iter_mut()) {
                if item.is_remainder {
                    item.monthly_cost = share;
                }
            }
        }
        let total_expenses: f64 = categories.iter().map(|c| c.total()).sum();

        MonthSummary {
            month_start: start,
            income,
            savings,
            months_to_goal: self.settings.months_to_goal(),
            categories,
            total_expenses,
            left_over: income - savings - total_expenses,
        }
    }

    /// Monthly bills for the daily budget: every expense due this month at its real amount.
    /// A left-over line (like rent) counts at the amount entered for it, if any.
    pub fn monthly_bills(&self, any_day: NaiveDate) -> f64 {
        let start = month_start(any_day.year(), any_day.month());
        let end = month_end(start);
        self.expenses
            .iter()
            .map(|e| {
                if e.is_remainder {
                    e.amount.max(0.0)
                } else {
                    e.amount * e.due_dates_between(start, end).len() as f64
                }
            })
            .sum()
    }

    /// Money for day-to-day spending this month: income minus savings and bills.
    pub fn spending_money(&self, any_day: NaiveDate) -> f64 {
        if self.daily.monthly_override > 0.0 {
            return self.daily.monthly_override;
        }
        (self.settings.monthly_income() - self.settings.monthly_savings() - self.monthly_bills(any_day)).max(0.0)
    }

    /// Every (date, expense) due in `from..=to`, sorted by date.
    pub fn schedule(&self, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, &Expense)> {
        let mut out: Vec<(NaiveDate, &Expense)> = self
            .expenses
            .iter()
            .flat_map(|e| e.due_dates_between(from, to).into_iter().map(move |d| (d, e)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.name.cmp(&b.1.name)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn exp(name: &str, amount: f64, freq: Frequency, start: NaiveDate) -> Expense {
        Expense {
            id: 0,
            name: name.into(),
            category: "Test".into(),
            amount,
            frequency: freq,
            start,
            remaining_payments: None,
            remaining_balance: None,
            is_remainder: false,
            notes: String::new(),
        }
    }

    #[test]
    fn income_matches_spreadsheet() {
        let s = Settings::default();
        assert!((s.monthly_income() - 2560.0).abs() < 1e-6);
        assert!((s.monthly_savings() - 256.0).abs() < 1e-6);
        assert_eq!(s.months_to_goal(), Some(20));
    }

    #[test]
    fn monthly_clamps_to_month_end() {
        let e = exp("x", 1.0, Frequency::Monthly, d(2026, 1, 31));
        let dates = e.due_dates_between(d(2026, 1, 1), d(2026, 4, 30));
        assert_eq!(dates, vec![d(2026, 1, 31), d(2026, 2, 28), d(2026, 3, 31), d(2026, 4, 30)]);
    }

    #[test]
    fn weekly_can_hit_five_times() {
        let e = exp("x", 10.0, Frequency::Weekly, d(2026, 10, 2));
        assert_eq!(e.due_dates_between(d(2026, 10, 1), d(2026, 10, 31)).len(), 5);
    }

    #[test]
    fn payments_count_down_and_stop() {
        let mut e = exp("Car", 325.0, Frequency::Monthly, d(2026, 11, 5));
        e.remaining_payments = Some(2);
        e.remaining_balance = Some(650.0);
        assert_eq!(e.remaining_payments_on(d(2026, 11, 1)), Some(2));
        assert_eq!(e.remaining_payments_on(d(2026, 12, 1)), Some(1));
        assert_eq!(e.remaining_balance_on(d(2026, 12, 1)), Some(325.0));
        assert_eq!(e.remaining_payments_on(d(2027, 1, 1)), Some(0));
        assert!(e.due_dates_between(d(2027, 1, 1), d(2027, 12, 31)).is_empty());
    }

    #[test]
    fn remainder_absorbs_leftover() {
        let mut b = Budget::default();
        b.add_expense(exp("Food", 1000.0, Frequency::Monthly, d(2026, 11, 1)));
        let mut rent = exp("Rent", 0.0, Frequency::Monthly, d(2026, 11, 1));
        rent.is_remainder = true;
        b.add_expense(rent);
        let s = b.summarize(d(2026, 11, 15));
        let rent_cost = s.categories[0].items.iter().find(|i| i.is_remainder).unwrap().monthly_cost;
        assert!((rent_cost - (2560.0 - 256.0 - 1000.0)).abs() < 1e-6);
        assert!(s.left_over.abs() < 1e-6);
    }
}
