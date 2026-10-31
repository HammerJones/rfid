//! The user interface (egui). Runs the same on Windows, Linux, macOS and Android.

use crate::bank::{self, Recurring, Txn};
use crate::csvio::{self, money, ImportResult};
use crate::model::{self, Budget, Expense, Frequency, IncomeMode};
use crate::storage;
use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{self, Color32, RichText};

const SAMPLE_SHEET: &str = include_str!("../tests/sample_sheet.csv");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Expenses,
    Calendar,
    Bank,
    Settings,
    ImportExport,
}

/// Form state for adding or editing an expense.
#[derive(Clone)]
struct Editor {
    id: Option<u64>,
    name: String,
    category: String,
    new_category: String,
    amount: f64,
    frequency: Frequency,
    start_text: String,
    has_payments: bool,
    payments: u32,
    has_balance: bool,
    balance: f64,
    is_remainder: bool,
    notes: String,
    error: Option<String>,
}

impl Editor {
    fn new(default_category: &str, start: NaiveDate) -> Self {
        Editor {
            id: None,
            name: String::new(),
            category: default_category.to_string(),
            new_category: String::new(),
            amount: 0.0,
            frequency: Frequency::Monthly,
            start_text: start.format("%Y-%m-%d").to_string(),
            has_payments: false,
            payments: 12,
            has_balance: false,
            balance: 0.0,
            is_remainder: false,
            notes: String::new(),
            error: None,
        }
    }

    fn from_expense(e: &Expense) -> Self {
        Editor {
            id: Some(e.id),
            name: e.name.clone(),
            category: e.category.clone(),
            new_category: String::new(),
            amount: e.amount,
            frequency: e.frequency,
            start_text: e.start.format("%Y-%m-%d").to_string(),
            has_payments: e.remaining_payments.is_some(),
            payments: e.remaining_payments.unwrap_or(12),
            has_balance: e.remaining_balance.is_some(),
            balance: e.remaining_balance.unwrap_or(0.0),
            is_remainder: e.is_remainder,
            notes: e.notes.clone(),
            error: None,
        }
    }

    fn build(&self) -> Result<Expense, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Give the expense a name.".into());
        }
        let category = if !self.new_category.trim().is_empty() {
            self.new_category.trim().to_string()
        } else if !self.category.trim().is_empty() {
            self.category.trim().to_string()
        } else {
            "Uncategorized".to_string()
        };
        let start = csvio::parse_date(&self.start_text)
            .ok_or("Start date should look like 2026-11-01 or 11/01/2026.")?;
        Ok(Expense {
            id: self.id.unwrap_or(0),
            name: name.to_string(),
            category,
            amount: self.amount.max(0.0),
            frequency: self.frequency,
            start,
            remaining_payments: self.has_payments.then_some(self.payments),
            remaining_balance: self.has_balance.then_some(self.balance),
            is_remainder: self.is_remainder,
            notes: self.notes.trim().to_string(),
        })
    }
}

enum Status {
    Info(String),
    Error(String),
}

pub struct BudgetApp {
    budget: Budget,
    tab: Tab,
    view_month: NaiveDate,
    selected_day: Option<NaiveDate>,
    editor: Option<Editor>,
    confirm_delete: Option<u64>,
    status: Option<Status>,
    filter: String,
    import_text: String,
    pending_import: Option<ImportResult>,
    import_replace: bool,
    import_settings: bool,
    new_category: String,
    renaming: Option<(usize, String)>,
    dirty: bool,
    bank_txns: Vec<Txn>,
    bank_found: Vec<(Recurring, bool)>,
    bank_files: Vec<String>,
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn first_of(d: NaiveDate) -> NaiveDate {
    model::month_start(d.year(), d.month())
}

/// Starting data on first launch: a made-up sample in the spreadsheet layout.
fn sample_budget() -> Budget {
    let mut b = Budget::default();
    if let Ok(res) = csvio::import(SAMPLE_SHEET, &b, first_of(today())) {
        if let Some(s) = res.settings {
            b.settings = s;
        }
        for e in res.expenses {
            b.add_expense(e);
        }
    }
    b
}

impl BudgetApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Bigger text and touch targets: easier on phones, still fine on desktop.
        cc.egui_ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(10.0, 5.0);
            s.spacing.interact_size.y = 26.0;
        });
        let (budget, status) = match storage::load() {
            Some(b) => (b, None),
            None => (
                sample_budget(),
                Some(Status::Info(
                    "Loaded sample data. Import your own spreadsheet from Import / Export, or edit anything here.".into(),
                )),
            ),
        };
        BudgetApp {
            budget,
            tab: Tab::Overview,
            view_month: first_of(today()),
            selected_day: Some(today()),
            editor: None,
            confirm_delete: None,
            status,
            filter: String::new(),
            import_text: String::new(),
            pending_import: None,
            import_replace: true,
            import_settings: true,
            new_category: String::new(),
            renaming: None,
            dirty: true,
            bank_txns: Vec::new(),
            bank_found: Vec::new(),
            bank_files: Vec::new(),
        }
    }

    fn info(&mut self, s: impl Into<String>) {
        self.status = Some(Status::Info(s.into()));
    }
    fn error(&mut self, s: impl Into<String>) {
        self.status = Some(Status::Error(s.into()));
    }

    fn month_nav(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("< Prev").clicked() {
                self.view_month = model::add_months(self.view_month, -1);
            }
            ui.label(RichText::new(self.view_month.format("%B %Y").to_string()).strong().size(18.0));
            if ui.button("Next >").clicked() {
                self.view_month = model::add_months(self.view_month, 1);
            }
            if ui.button("This month").clicked() {
                self.view_month = first_of(today());
            }
        });
    }

    // ---------------------------------------------------------------- Overview
    fn overview(&mut self, ui: &mut egui::Ui) {
        self.month_nav(ui);
        let s = self.budget.summarize(self.view_month);
        let st = self.budget.settings.clone();
        ui.add_space(4.0);

        let color = if s.left_over < -0.004 { Color32::from_rgb(220, 70, 70) } else { Color32::from_rgb(60, 170, 90) };
        let cards = [
            ("Income (after tax)".to_string(), money(s.income), None),
            (format!("Savings ({:.0}%)", st.savings_rate * 100.0), money(s.savings), None),
            ("Expenses".to_string(), money(s.total_expenses), None),
            ("Left over".to_string(), money(s.left_over), Some(color)),
        ];
        // Four across on wide screens, two by two on phones.
        let per_row = if ui.available_width() > 620.0 { 4 } else { 2 };
        for chunk in cards.chunks(per_row) {
            ui.columns(per_row, |cols| {
                for (col, (title, value, color)) in cols.iter_mut().zip(chunk) {
                    stat_card(col, title, value, *color);
                }
            });
        }

        // Savings goal progress.
        let goal = st.savings_goal.max(0.01);
        let frac = (st.current_savings / goal).clamp(0.0, 1.0) as f32;
        let months = match s.months_to_goal {
            Some(0) => "Goal reached".to_string(),
            Some(m) => format!("{m} months to go at {}/mo", money(s.savings)),
            None => "Set a savings rate to reach this".to_string(),
        };
        ui.add_space(4.0);
        ui.label(format!(
            "Emergency fund: {} of {} ({})",
            money(st.current_savings),
            money(st.savings_goal),
            months
        ));
        ui.add(egui::ProgressBar::new(frac).desired_height(10.0));

        if let Some(rem) = s.categories.iter().flat_map(|c| &c.items).find(|i| i.is_remainder) {
            let msg = if rem.monthly_cost < 0.0 {
                format!("Over budget: \"{}\" would need to be {} to balance.", rem.name, money(rem.monthly_cost))
            } else {
                format!("Max \"{}\" you can afford this month: {}", rem.name, money(rem.monthly_cost))
            };
            ui.label(RichText::new(msg).strong());
        }
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            if ui.button("Export monthly summary (CSV)").clicked() {
                self.export_summary(ui.ctx());
            }
            if ui.button("+ Add expense").clicked() {
                let cat = self.budget.ordered_categories().first().cloned().unwrap_or_default();
                self.editor = Some(Editor::new(&cat, self.view_month));
            }
        });
        ui.add_space(4.0);

        let mut edit: Option<u64> = None;
        for cat in &s.categories {
            let share = if s.income > 0.0 { (cat.total() / s.income).clamp(0.0, 1.0) as f32 } else { 0.0 };
            egui::CollapsingHeader::new(RichText::new(format!("{}  ({})", cat.name, money(cat.total()))).strong())
                .id_salt(("cat", &cat.name))
                .default_open(true)
                .show(ui, |ui| {
                    ui.add(egui::ProgressBar::new(share).text(format!("{:.0}% of income", share * 100.0)));
                    let narrow = ui.available_width() < 480.0;
                    egui::Grid::new(("grid", &cat.name)).striped(true).show(ui, |ui| {
                        ui.label(RichText::new("Item").weak());
                        ui.label(RichText::new("This month").weak());
                        ui.label(RichText::new("Due").weak());
                        if narrow {
                            ui.label(RichText::new("Left").weak());
                        } else {
                            ui.label(RichText::new("Balance left").weak());
                            ui.label(RichText::new("Payments left").weak());
                        }
                        ui.end_row();
                        for it in &cat.items {
                            let name = if it.is_remainder { format!("{} (left-over)", it.name) } else { it.name.clone() };
                            if ui.link(name).on_hover_text("Edit").clicked() {
                                edit = Some(it.expense_id);
                            }
                            ui.label(money(it.monthly_cost));
                            let due = it.due_dates.iter().map(|d| ordinal(d.day())).collect::<Vec<_>>().join(", ");
                            ui.label(if due.is_empty() { "-".into() } else { due });
                            let bal = it.remaining_balance.map(money);
                            let pay = it.remaining_payments.map(|p| format!("{p} pmts"));
                            if narrow {
                                let both = [bal, pay].into_iter().flatten().collect::<Vec<_>>().join(", ");
                                ui.label(if both.is_empty() { "-".into() } else { both });
                            } else {
                                ui.label(it.remaining_balance.map(money).unwrap_or_else(|| "n/a".into()));
                                ui.label(it.remaining_payments.map(|p| p.to_string()).unwrap_or_else(|| "n/a".into()));
                            }
                            ui.end_row();
                        }
                    });
                });
        }
        if s.categories.is_empty() {
            ui.label("No expenses are due this month. Add one, or import a CSV.");
        }
        if let Some(id) = edit {
            self.open_editor(id);
        }
    }

    // ---------------------------------------------------------------- Expenses
    fn expenses(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Add expense").clicked() {
                let cat = self.budget.ordered_categories().first().cloned().unwrap_or_default();
                self.editor = Some(Editor::new(&cat, first_of(today())));
            }
            ui.label("Search:");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(160.0));
        });
        ui.separator();
        let t = today();
        let filter = self.filter.to_lowercase();
        let mut edit = None;
        let mut delete = None;
        for cat in self.budget.ordered_categories() {
            let items: Vec<&Expense> = self
                .budget
                .expenses
                .iter()
                .filter(|e| e.category == cat)
                .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter) || cat.to_lowercase().contains(&filter))
                .collect();
            if items.is_empty() {
                continue;
            }
            ui.label(RichText::new(&cat).strong().size(16.0));
            egui::Grid::new(("exp", &cat)).striped(true).num_columns(5).show(ui, |ui| {
                for e in items {
                    ui.label(&e.name);
                    ui.label(if e.is_remainder { "left-over".to_string() } else { money(e.amount) });
                    ui.label(e.frequency.label());
                    let next = e.due_dates_between(t, t + Duration::days(800)).first().copied();
                    ui.label(match next {
                        Some(d) => format!("next {}", d.format("%b %-d")),
                        None => "finished".into(),
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Edit").clicked() {
                            edit = Some(e.id);
                        }
                        if ui.small_button("Delete").clicked() {
                            delete = Some(e.id);
                        }
                    });
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
        }
        if self.budget.expenses.is_empty() {
            ui.label("No expenses yet.");
        }
        if let Some(id) = edit {
            self.open_editor(id);
        }
        if delete.is_some() {
            self.confirm_delete = delete;
        }
    }

    // ---------------------------------------------------------------- Calendar
    fn calendar(&mut self, ui: &mut egui::Ui) {
        self.month_nav(ui);
        let start = self.view_month;
        let end = model::month_end(start);
        let due = self.budget.schedule(start, end);
        let s = self.budget.summarize(start);
        let rem_cost: f64 = s.categories.iter().flat_map(|c| &c.items).filter(|i| i.is_remainder).map(|i| i.monthly_cost).sum();

        let amount_of = |e: &Expense| if e.is_remainder { rem_cost } else { e.amount };
        let cell_w = ((ui.available_width() - 6.0 * 4.0) / 7.0).max(40.0);
        let compact = cell_w < 90.0;
        let cell_h = if compact { 56.0 } else { 86.0 };

        egui::Grid::new("cal").spacing(egui::vec2(4.0, 4.0)).show(ui, |ui| {
            for w in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                ui.allocate_ui(egui::vec2(cell_w, 18.0), |ui| ui.label(RichText::new(w).weak()));
            }
            ui.end_row();
            let lead = start.weekday().num_days_from_sunday();
            for _ in 0..lead {
                ui.allocate_space(egui::vec2(cell_w, cell_h));
            }
            let mut col = lead;
            let mut day = start;
            while day <= end {
                let todays: Vec<&(NaiveDate, &Expense)> = due.iter().filter(|(d, _)| *d == day).collect();
                let selected = self.selected_day == Some(day);
                let is_today = day == today();
                let mut frame = egui::Frame::group(ui.style()).inner_margin(4.0);
                if selected {
                    frame = frame.stroke(egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill));
                } else if is_today {
                    frame = frame.stroke(egui::Stroke::new(1.5_f32, Color32::from_rgb(60, 170, 90)));
                }
                let resp = frame
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(cell_w - 10.0, cell_h - 10.0));
                        ui.set_max_width(cell_w - 10.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(day.day().to_string()).strong());
                            if compact {
                                if !todays.is_empty() {
                                    let total: f64 = todays.iter().map(|(_, e)| amount_of(e)).sum();
                                    ui.label(RichText::new(format!("{} due", todays.len())).small());
                                    ui.label(RichText::new(money(total)).small());
                                }
                            } else {
                                for (_, e) in todays.iter().take(3) {
                                    ui.add(egui::Label::new(RichText::new(format!("{} {}", e.name, money(amount_of(e)))).small()).truncate());
                                }
                                if todays.len() > 3 {
                                    ui.label(RichText::new(format!("+{} more", todays.len() - 3)).small().weak());
                                }
                            }
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if resp.clicked() {
                    self.selected_day = Some(day);
                }
                col += 1;
                if col % 7 == 0 {
                    ui.end_row();
                }
                day = day.succ_opt().expect("next day");
            }
        });

        ui.separator();
        let month_total: f64 = due.iter().map(|(_, e)| amount_of(e)).sum();
        ui.label(format!("{} payments due this month, {} total.", due.len(), money(month_total)));

        if let Some(sel) = self.selected_day {
            ui.label(RichText::new(sel.format("%A, %B %-d").to_string()).strong());
            let list: Vec<(String, f64, u64)> = self
                .budget
                .schedule(sel, sel)
                .into_iter()
                .map(|(_, e)| (e.name.clone(), amount_of(e), e.id))
                .collect();
            if list.is_empty() {
                ui.label("Nothing due.");
            }
            let mut edit = None;
            for (name, amt, id) in list {
                ui.horizontal(|ui| {
                    if ui.link(&name).clicked() {
                        edit = Some(id);
                    }
                    ui.label(money(amt));
                });
            }
            if let Some(id) = edit {
                self.open_editor(id);
            }
        }

        ui.separator();
        ui.label(RichText::new("Next 30 days").strong());
        let t = today();
        let upcoming: Vec<(NaiveDate, String, f64)> = self
            .budget
            .schedule(t, t + Duration::days(30))
            .into_iter()
            .map(|(d, e)| (d, e.name.clone(), amount_of(e)))
            .collect();
        egui::Grid::new("upcoming").striped(true).show(ui, |ui| {
            for (d, name, amt) in &upcoming {
                ui.label(d.format("%a %b %-d").to_string());
                ui.label(name);
                ui.label(money(*amt));
                ui.end_row();
            }
        });
        if upcoming.is_empty() {
            ui.label("Nothing due in the next 30 days.");
        }
    }


    // ---------------------------------------------------------------- Bank statements
    fn bank_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Bank & card statements").strong().size(16.0));
        ui.label(
            "Import CSV statements from your bank or card (download them from the bank's website). \
             The app finds payments that repeat on a schedule and shows where your money goes. \
             Add several months for the best results.",
        );
        ui.label(RichText::new("Statements are only analyzed in memory; they aren't saved.").weak());
        ui.horizontal_wrapped(|ui| {
            #[cfg(not(target_os = "android"))]
            if ui.button("Add statement CSV(s)...").clicked() {
                if let Some(paths) = rfd::FileDialog::new().add_filter("CSV", &["csv", "txt"]).pick_files() {
                    for path in paths {
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        match std::fs::read_to_string(&path) {
                            Ok(text) => self.add_statement(&name, &text),
                            Err(e) => self.error(format!("Couldn't read {name}: {e}")),
                        }
                    }
                }
            }
            #[cfg(target_os = "android")]
            {
                let p = storage::export_dir().join("statement.csv");
                if ui.button("Import statement.csv").on_hover_text(p.display().to_string()).clicked() {
                    match std::fs::read_to_string(&p) {
                        Ok(text) => self.add_statement("statement.csv", &text),
                        Err(e) => self.error(format!("Copy your statement to {} first ({e}).", p.display())),
                    }
                }
            }
            if !self.bank_txns.is_empty() && ui.button("Clear").clicked() {
                self.bank_txns.clear();
                self.bank_found.clear();
                self.bank_files.clear();
            }
        });
        ui.collapsing("Or paste statement CSV text", |ui| {
            egui::ScrollArea::vertical().id_salt("paste_bank").max_height(140.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.import_text).desired_rows(5).desired_width(f32::INFINITY).code_editor());
            });
            if ui.button("Analyze pasted text").clicked() {
                let text = std::mem::take(&mut self.import_text);
                self.add_statement("pasted text", &text);
            }
        });
        if self.bank_txns.is_empty() {
            return;
        }
        let first = self.bank_txns.first().map(|t| t.date).unwrap();
        let last = self.bank_txns.last().map(|t| t.date).unwrap();
        ui.label(format!(
            "{} transactions from {} to {} ({}).",
            self.bank_txns.len(),
            first.format("%b %-d, %Y"),
            last.format("%b %-d, %Y"),
            self.bank_files.join(", ")
        ));
        ui.separator();

        // Recurring payments.
        ui.label(RichText::new("Recurring payments found").strong().size(16.0));
        if self.bank_found.is_empty() {
            ui.label("None found yet. Recurring payments need at least 2 charges (3 for weekly), so add more months.");
        } else {
            let existing: Vec<String> = self.budget.expenses.iter().map(|e| e.name.to_lowercase()).collect();
            let cats = self.budget.ordered_categories();
            egui::Grid::new("recurring").striped(true).show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new("Name").weak());
                ui.label(RichText::new("Amount").weak());
                ui.label(RichText::new("Repeats").weak());
                ui.label(RichText::new("Next").weak());
                ui.label(RichText::new("Category").weak());
                ui.end_row();
                for (i, (r, sel)) in self.bank_found.iter_mut().enumerate() {
                    ui.checkbox(sel, "");
                    ui.add_sized([150.0, 22.0], egui::TextEdit::singleline(&mut r.name));
                    let mut amt = money(r.amount);
                    if !r.fixed_amount {
                        amt.push_str(" (varies)");
                    }
                    ui.label(amt).on_hover_text(format!("{} charges seen, last on {}", r.count, r.last));
                    ui.label(r.frequency.label());
                    ui.label(r.next.format("%b %-d").to_string());
                    egui::ComboBox::from_id_salt(("rcat", i)).selected_text(r.category.as_str()).show_ui(ui, |ui| {
                        for c in &cats {
                            ui.selectable_value(&mut r.category, c.clone(), c);
                        }
                        for c in ["Health", "Uncategorized"] {
                            if !cats.iter().any(|x| x == c) {
                                ui.selectable_value(&mut r.category, c.to_string(), c);
                            }
                        }
                    });
                    if already_budgeted(&existing, &r.name) {
                        ui.label(RichText::new("may already be in budget").weak().small());
                    }
                    ui.end_row();
                }
            });
            let picked = self.bank_found.iter().filter(|(_, s)| *s).count();
            let total: f64 = self
                .bank_found
                .iter()
                .filter(|(_, s)| *s)
                .map(|(r, _)| r.amount * monthly_factor(r.frequency))
                .sum();
            ui.label(format!("{picked} selected, about {} per month.", money(total)));
            if ui.add_enabled(picked > 0, egui::Button::new("Add selected to my budget")).clicked() {
                let chosen: Vec<Recurring> = self.bank_found.iter().filter(|(_, s)| *s).map(|(r, _)| r.clone()).collect();
                for r in &chosen {
                    self.budget.add_expense(Expense {
                        id: 0,
                        name: r.name.clone(),
                        category: r.category.clone(),
                        amount: r.amount,
                        frequency: r.frequency,
                        start: r.next,
                        remaining_payments: None,
                        remaining_balance: None,
                        is_remainder: false,
                        notes: format!("Found in bank statement ({} charges, last {}).", r.count, r.last),
                    });
                }
                self.bank_found.retain(|(_, s)| !*s);
                self.dirty = true;
                self.info(format!("Added {} recurring payments.", chosen.len()));
            }
        }
        ui.separator();

        // Monthly totals.
        ui.label(RichText::new("Money in and out by month").strong().size(16.0));
        let months = bank::monthly_totals(&self.bank_txns);
        egui::Grid::new("bank_months").striped(true).show(ui, |ui| {
            ui.label(RichText::new("Month").weak());
            ui.label(RichText::new("In").weak());
            ui.label(RichText::new("Out").weak());
            ui.label(RichText::new("Net").weak());
            ui.end_row();
            for (m, t) in &months {
                ui.label(m.format("%b %Y").to_string());
                ui.label(money(t.money_in));
                ui.label(money(t.money_out));
                let net = t.money_in - t.money_out;
                let c = if net < 0.0 { Color32::from_rgb(220, 70, 70) } else { Color32::from_rgb(60, 170, 90) };
                ui.colored_label(c, money(net));
                ui.end_row();
            }
        });
        if months.len() > 1 {
            let avg_out: f64 = months.values().map(|t| t.money_out).sum::<f64>() / months.len() as f64;
            ui.label(format!("Average spending: {} per month. Partial first/last months pull this down.", money(avg_out)));
        }
        ui.separator();

        // Spending breakdowns.
        ui.label(RichText::new("Spending by category (best guess)").strong().size(16.0));
        let cats = bank::by_category(&self.bank_txns);
        let total: f64 = cats.iter().map(|c| c.1).sum::<f64>().max(0.01);
        for (name, amt) in &cats {
            ui.add(egui::ProgressBar::new((amt / total) as f32).text(format!("{name}: {} ({:.0}%)", money(*amt), amt / total * 100.0)));
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Top merchants").strong().size(16.0));
        egui::Grid::new("merchants").striped(true).show(ui, |ui| {
            for (name, amt, n) in bank::by_merchant(&self.bank_txns).into_iter().take(15) {
                ui.label(name);
                ui.label(money(amt));
                ui.label(RichText::new(format!("{n}x")).weak());
                ui.end_row();
            }
        });
    }

    fn add_statement(&mut self, name: &str, text: &str) {
        match bank::parse_statement(text) {
            Ok(txns) => {
                let n = txns.len();
                // Skip exact duplicates when the same statement (or overlapping ones) is added twice.
                for t in txns {
                    if !self.bank_txns.contains(&t) {
                        self.bank_txns.push(t);
                    }
                }
                self.bank_txns.sort_by_key(|t| t.date);
                self.bank_files.push(name.to_string());
                let found = bank::detect_recurring(&self.bank_txns);
                // Pre-select only the ones that don't look like something already in the budget.
                let existing: Vec<String> = self.budget.expenses.iter().map(|e| e.name.to_lowercase()).collect();
                self.bank_found = found
                    .into_iter()
                    .map(|r| {
                        let new = !already_budgeted(&existing, &r.name);
                        (r, new)
                    })
                    .collect();
                self.info(format!("Read {n} transactions from {name}."));
            }
            Err(e) => self.error(format!("{name}: {e}")),
        }
    }

    // ---------------------------------------------------------------- Settings
    fn settings(&mut self, ui: &mut egui::Ui) {
        let before = self.budget.settings.clone();
        let st = &mut self.budget.settings;
        ui.label(RichText::new("Income").strong().size(16.0));
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.income_mode, IncomeMode::Hourly, "Hourly pay");
            ui.radio_value(&mut st.income_mode, IncomeMode::Fixed, "Fixed monthly amount");
        });
        egui::Grid::new("income").num_columns(2).show(ui, |ui| {
            match st.income_mode {
                IncomeMode::Hourly => {
                    ui.label("Hourly rate");
                    ui.add(egui::DragValue::new(&mut st.hourly_rate).prefix("$").speed(0.25).range(0.0..=10_000.0).max_decimals(2));
                    ui.end_row();
                    ui.label("Hours per week");
                    ui.add(egui::DragValue::new(&mut st.hours_per_week).speed(0.5).range(0.0..=168.0).max_decimals(2));
                    ui.end_row();
                    ui.label("Weeks per month");
                    ui.add(egui::DragValue::new(&mut st.weeks_per_month).speed(0.01).range(1.0..=5.0).max_decimals(2))
                        .on_hover_text("4 is simplest. The true average is about 4.33.");
                    ui.end_row();
                    ui.label("Taxes withheld");
                    let mut pct = st.tax_rate * 100.0;
                    if ui.add(egui::DragValue::new(&mut pct).suffix("%").speed(0.25).range(0.0..=60.0).max_decimals(2)).changed() {
                        st.tax_rate = pct / 100.0;
                    }
                    ui.end_row();
                }
                IncomeMode::Fixed => {
                    ui.label("After-tax income per month");
                    ui.add(egui::DragValue::new(&mut st.fixed_monthly_income).prefix("$").speed(10.0).range(0.0..=1e7).max_decimals(2));
                    ui.end_row();
                }
            }
        });
        ui.label(format!("Monthly income after tax: {}", money(st.monthly_income())));

        ui.add_space(8.0);
        ui.label(RichText::new("Savings").strong().size(16.0));
        egui::Grid::new("savings").num_columns(2).show(ui, |ui| {
            ui.label("Save this share of income");
            let mut pct = st.savings_rate * 100.0;
            if ui.add(egui::DragValue::new(&mut pct).suffix("%").speed(0.25).range(0.0..=100.0).max_decimals(2)).changed() {
                st.savings_rate = pct / 100.0;
            }
            ui.end_row();
            ui.label("Savings goal");
            ui.add(egui::DragValue::new(&mut st.savings_goal).prefix("$").speed(50.0).range(0.0..=1e8).max_decimals(2));
            ui.end_row();
            ui.label("Saved so far");
            ui.add(egui::DragValue::new(&mut st.current_savings).prefix("$").speed(10.0).range(0.0..=1e8).max_decimals(2));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label(RichText::new("Note at the top of the exported summary").strong());
        ui.add(egui::TextEdit::multiline(&mut st.note).desired_rows(3).desired_width(f32::INFINITY));
        if self.budget.settings != before {
            self.dirty = true;
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Categories").strong().size(16.0));
        ui.label(RichText::new("Order here is the order used in the summary and export.").weak());
        let cats = self.budget.ordered_categories();
        if self.budget.categories != cats {
            self.budget.categories = cats.clone();
            self.dirty = true;
        }
        let mut action: Option<(&str, usize)> = None;
        for (i, c) in cats.iter().enumerate() {
            ui.horizontal(|ui| {
                if let Some((ri, text)) = &mut self.renaming {
                    if *ri == i {
                        ui.text_edit_singleline(text);
                        if ui.button("Save").clicked() {
                            action = Some(("rename", i));
                        }
                        return;
                    }
                }
                ui.label(c);
                let count = self.budget.expenses.iter().filter(|e| &e.category == c).count();
                ui.label(RichText::new(format!("({count})")).weak());
                if ui.small_button("Up").clicked() && i > 0 {
                    action = Some(("up", i));
                }
                if ui.small_button("Down").clicked() && i + 1 < cats.len() {
                    action = Some(("down", i));
                }
                if ui.small_button("Rename").clicked() {
                    self.renaming = Some((i, c.clone()));
                }
                if ui.small_button("Delete").clicked() {
                    action = Some(("delete", i));
                }
            });
        }
        match action {
            Some(("up", i)) => self.budget.categories.swap(i, i - 1),
            Some(("down", i)) => self.budget.categories.swap(i, i + 1),
            Some(("rename", i)) => {
                if let Some((_, new)) = self.renaming.take() {
                    let new = new.trim().to_string();
                    let old = self.budget.categories[i].clone();
                    if !new.is_empty() && new != old {
                        for e in self.budget.expenses.iter_mut().filter(|e| e.category == old) {
                            e.category = new.clone();
                        }
                        if self.budget.categories.contains(&new) {
                            self.budget.categories.remove(i);
                        } else {
                            self.budget.categories[i] = new;
                        }
                    }
                }
            }
            Some(("delete", i)) => {
                let old = self.budget.categories.remove(i);
                let mut moved = 0;
                for e in self.budget.expenses.iter_mut().filter(|e| e.category == old) {
                    e.category = "Uncategorized".into();
                    moved += 1;
                }
                if moved > 0 {
                    self.budget.ensure_category("Uncategorized");
                    self.info(format!("Deleted \"{old}\"; moved {moved} expense(s) to Uncategorized."));
                }
            }
            _ => {}
        }
        if action.is_some() {
            self.dirty = true;
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_category).hint_text("New category").desired_width(180.0));
            if ui.button("Add").clicked() && !self.new_category.trim().is_empty() {
                let n = self.new_category.trim().to_string();
                self.budget.ensure_category(&n);
                self.new_category.clear();
                self.dirty = true;
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new(format!("Your data is saved automatically to {}", storage::budget_path().display())).weak());
        if ui.button("Reset to sample data").clicked() {
            self.budget = sample_budget();
            self.dirty = true;
            self.info("Reset to sample data.");
        }
    }

    // ---------------------------------------------------------------- Import / Export
    fn import_export(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Import").strong().size(16.0));
        ui.label(
            "Accepts your monthly summary spreadsheet (exported as CSV), or a list with columns like \
             name, category, amount, frequency, start date, remaining balance, remaining payments.",
        );
        #[cfg(not(target_os = "android"))]
        if ui.button("Choose a CSV file...").clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv", "txt"]).pick_file() {
                match std::fs::read_to_string(&path) {
                    Ok(text) => self.preview_import(&text),
                    Err(e) => self.error(format!("Couldn't read {}: {e}", path.display())),
                }
            }
        }
        #[cfg(target_os = "android")]
        {
            let p = storage::export_dir().join("import.csv");
            ui.label(format!("To import a file, copy it to {} and tap below, or paste the CSV text.", p.display()));
            if ui.button("Import import.csv").clicked() {
                match std::fs::read_to_string(&p) {
                    Ok(text) => self.preview_import(&text),
                    Err(e) => self.error(format!("Couldn't read {}: {e}", p.display())),
                }
            }
        }
        ui.collapsing("Or paste CSV text", |ui| {
            egui::ScrollArea::vertical().id_salt("paste_import").max_height(160.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.import_text).desired_rows(6).desired_width(f32::INFINITY).code_editor());
            });
            if ui.button("Import pasted text").clicked() {
                let text = self.import_text.clone();
                self.preview_import(&text);
            }
        });

        let (mut do_import, mut cancel_import) = (false, false);
        if let Some(res) = &self.pending_import {
            ui.group(|ui| {
                ui.label(RichText::new(format!("Ready to import {} expenses from a {}.", res.expenses.len(), res.kind)).strong());
                for w in &res.warnings {
                    ui.label(RichText::new(format!("- {w}")).weak());
                }
                egui::ScrollArea::vertical().id_salt("preview").max_height(160.0).show(ui, |ui| {
                    egui::Grid::new("preview_grid").striped(true).show(ui, |ui| {
                        for e in &res.expenses {
                            ui.label(&e.category);
                            ui.label(&e.name);
                            ui.label(if e.is_remainder { "left-over".into() } else { money(e.amount) });
                            ui.label(e.frequency.label());
                            ui.label(e.start.format("%Y-%m-%d").to_string());
                            ui.end_row();
                        }
                    });
                });
                ui.radio_value(&mut self.import_replace, true, "Replace my current expenses");
                ui.radio_value(&mut self.import_replace, false, "Add to my current expenses");
                if res.settings.is_some() {
                    ui.checkbox(&mut self.import_settings, "Also use the income and savings numbers from the file");
                }
                ui.horizontal(|ui| {
                    if ui.button("Import").clicked() {
                        do_import = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_import = true;
                    }
                });
            });
        }
        if do_import {
            self.apply_import();
        }
        if cancel_import {
            self.pending_import = None;
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new("Export").strong().size(16.0));
        self.month_nav(ui);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Monthly summary (CSV)").clicked() {
                self.export_summary(ui.ctx());
            }
            if ui.button("All expenses (CSV)").clicked() {
                let text = csvio::export_expense_list(&self.budget);
                self.save_csv(ui.ctx(), "Budget Expenses.csv", text);
            }
            if ui.button("Blank import template").clicked() {
                let text = format!("{}\nGym,Health,40.00,Monthly,2026-11-15,,,,\n", csvio::LIST_HEADERS.join(","));
                self.save_csv(ui.ctx(), "Budget Import Template.csv", text);
            }
        });
        ui.label(RichText::new("The monthly summary uses the Monthly Costs spreadsheet layout, with totals added at the bottom.").weak());
    }

    fn preview_import(&mut self, text: &str) {
        match csvio::import(text, &self.budget, self.view_month) {
            Ok(res) => {
                self.import_settings = res.settings.is_some();
                self.pending_import = Some(res);
            }
            Err(e) => self.error(e),
        }
    }

    fn apply_import(&mut self) {
        let Some(res) = self.pending_import.take() else { return };
        if self.import_replace {
            self.budget.expenses.clear();
            self.budget.categories.clear();
        }
        if self.import_settings {
            if let Some(s) = res.settings {
                self.budget.settings = s;
            }
        }
        let n = res.expenses.len();
        for e in res.expenses {
            self.budget.add_expense(e);
        }
        self.dirty = true;
        self.import_text.clear();
        self.info(format!("Imported {n} expenses."));
    }

    fn export_summary(&mut self, ctx: &egui::Context) {
        let s = self.budget.summarize(self.view_month);
        let text = csvio::export_summary(&self.budget, &s);
        let name = format!("Budget Summary - {}.csv", self.view_month.format("%Y-%m"));
        self.save_csv(ctx, &name, text);
    }

    /// Desktop: Save dialog. Android: write to the app's export folder and copy to clipboard.
    fn save_csv(&mut self, ctx: &egui::Context, name: &str, text: String) {
        #[cfg(not(target_os = "android"))]
        {
            let _ = ctx;
            let dialog = rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(name)
                .set_directory(storage::export_dir());
            if let Some(path) = dialog.save_file() {
                match std::fs::write(&path, text) {
                    Ok(()) => self.info(format!("Saved {}", path.display())),
                    Err(e) => self.error(format!("Couldn't save: {e}")),
                }
            }
        }
        #[cfg(target_os = "android")]
        {
            ctx.copy_text(text.clone());
            match storage::write_export(name, &text) {
                Ok(p) => self.info(format!("Saved {} (also copied to clipboard)", p.display())),
                Err(e) => self.error(format!("Couldn't save: {e}. The CSV was copied to the clipboard.")),
            }
        }
    }

    fn open_editor(&mut self, id: u64) {
        if let Some(e) = self.budget.expenses.iter().find(|e| e.id == id) {
            self.editor = Some(Editor::from_expense(e));
        }
    }

    // ---------------------------------------------------------------- Dialogs
    fn editor_window(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.editor.take() else { return };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let cats = self.budget.ordered_categories();
        let width = (ctx.screen_rect().width() - 40.0).clamp(260.0, 420.0);
        egui::Window::new(if ed.id.is_some() { "Edit expense" } else { "New expense" })
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 40.0))
            .show(ctx, |ui| {
                egui::Grid::new("editor").num_columns(2).spacing(egui::vec2(10.0, 8.0)).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(200.0));
                    ui.end_row();

                    ui.label("Category");
                    ui.vertical(|ui| {
                        egui::ComboBox::from_id_salt("cat_combo")
                            .selected_text(if ed.category.is_empty() { "Choose..." } else { ed.category.as_str() })
                            .show_ui(ui, |ui| {
                                for c in &cats {
                                    ui.selectable_value(&mut ed.category, c.clone(), c);
                                }
                            });
                        ui.add(egui::TextEdit::singleline(&mut ed.new_category).hint_text("or type a new one").desired_width(200.0));
                    });
                    ui.end_row();

                    ui.label("Left-over line");
                    ui.checkbox(&mut ed.is_remainder, "Cost = whatever is left")
                        .on_hover_text("Like a rent budget: income minus savings minus every other expense.");
                    ui.end_row();

                    if !ed.is_remainder {
                        ui.label("Amount");
                        ui.add(egui::DragValue::new(&mut ed.amount).prefix("$").speed(1.0).range(0.0..=1e7).min_decimals(2).max_decimals(2));
                        ui.end_row();
                    }

                    ui.label("Repeats");
                    egui::ComboBox::from_id_salt("freq_combo")
                        .selected_text(ed.frequency.label())
                        .show_ui(ui, |ui| {
                            for f in Frequency::ALL {
                                ui.selectable_value(&mut ed.frequency, f, f.label());
                            }
                        });
                    ui.end_row();

                    ui.label("First due date");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut ed.start_text).desired_width(110.0).hint_text("2026-11-01"));
                        if ui.small_button("Today").clicked() {
                            ed.start_text = today().format("%Y-%m-%d").to_string();
                        }
                    });
                    ui.end_row();

                    if !ed.is_remainder && ed.frequency != Frequency::Once {
                        ui.label("Payments left");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut ed.has_payments, "");
                            ui.add_enabled(ed.has_payments, egui::DragValue::new(&mut ed.payments).range(1..=10_000));
                        });
                        ui.end_row();
                    }
                    if !ed.is_remainder {
                        ui.label("Balance left");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut ed.has_balance, "");
                            ui.add_enabled(
                                ed.has_balance,
                                egui::DragValue::new(&mut ed.balance).prefix("$").speed(5.0).range(0.0..=1e8).min_decimals(2).max_decimals(2),
                            );
                        });
                        ui.end_row();
                    }

                    ui.label("Notes");
                    ui.add(egui::TextEdit::multiline(&mut ed.notes).desired_rows(2).desired_width(200.0));
                    ui.end_row();
                });
                if ed.has_payments || ed.has_balance {
                    ui.label(RichText::new("Payments and balance are counted from the first due date and count down each payment.").weak().small());
                }
                if let Some(err) = &ed.error {
                    ui.colored_label(Color32::from_rgb(220, 70, 70), err);
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if let Some(id) = ed.id {
                        if ui.button("Delete").clicked() {
                            self.confirm_delete = Some(id);
                            cancel = true;
                        }
                    }
                });
            });

        if save {
            match ed.build() {
                Ok(e) => {
                    if let Some(id) = ed.id {
                        self.budget.ensure_category(&e.category);
                        if let Some(slot) = self.budget.expenses.iter_mut().find(|x| x.id == id) {
                            *slot = e;
                        }
                        self.info("Saved.");
                    } else {
                        let name = e.name.clone();
                        self.budget.add_expense(e);
                        self.info(format!("Added \"{name}\"."));
                    }
                    self.dirty = true;
                    return;
                }
                Err(msg) => ed.error = Some(msg),
            }
        }
        if open && !cancel {
            self.editor = Some(ed);
        }
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else { return };
        let name = self.budget.expenses.iter().find(|e| e.id == id).map(|e| e.name.clone()).unwrap_or_default();
        let mut close = false;
        egui::Window::new("Delete expense?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("Delete \"{name}\"? This can't be undone."));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.budget.expenses.retain(|e| e.id != id);
                        self.dirty = true;
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.confirm_delete = None;
        }
    }
}

fn stat_card(ui: &mut egui::Ui, title: &str, value: &str, color: Option<Color32>) {
    egui::Frame::group(ui.style()).inner_margin(8.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(title).weak());
            let mut t = RichText::new(value).size(20.0).strong();
            if let Some(c) = color {
                t = t.color(c);
            }
            ui.label(t);
        });
    });
}

/// Does a found payment look like an expense that's already in the budget?
fn already_budgeted(existing: &[String], name: &str) -> bool {
    let name = name.to_lowercase();
    let first = name.split_whitespace().next().unwrap_or("");
    existing.iter().any(|n| {
        !n.is_empty() && (n.contains(&name) || name.contains(n.as_str()) || (first.len() >= 4 && n.contains(first)))
    })
}

/// Roughly how many times per month a schedule charges.
fn monthly_factor(f: Frequency) -> f64 {
    match f {
        Frequency::Once => 0.0,
        Frequency::Weekly => 52.0 / 12.0,
        Frequency::Biweekly => 26.0 / 12.0,
        Frequency::Monthly => 1.0,
        Frequency::Quarterly => 1.0 / 3.0,
        Frequency::Yearly => 1.0 / 12.0,
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

impl eframe::App for BudgetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Budget").strong().size(20.0));
                ui.add_space(8.0);
                ui.selectable_value(&mut self.tab, Tab::Overview, "Overview");
                ui.selectable_value(&mut self.tab, Tab::Expenses, "Expenses");
                ui.selectable_value(&mut self.tab, Tab::Calendar, "Schedule");
                ui.selectable_value(&mut self.tab, Tab::Bank, "Bank");
                ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
                ui.selectable_value(&mut self.tab, Tab::ImportExport, "Import / Export");
            });
            ui.add_space(4.0);
        });

        if self.status.is_some() {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    match &self.status {
                        Some(Status::Info(s)) => ui.label(s),
                        Some(Status::Error(s)) => ui.colored_label(Color32::from_rgb(220, 70, 70), s),
                        None => ui.label(""),
                    };
                    if ui.small_button("x").clicked() {
                        self.status = None;
                    }
                });
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                Tab::Overview => self.overview(ui),
                Tab::Expenses => self.expenses(ui),
                Tab::Calendar => self.calendar(ui),
                Tab::Bank => self.bank_tab(ui),
                Tab::Settings => self.settings(ui),
                Tab::ImportExport => self.import_export(ui),
            });
        });

        self.editor_window(ctx);
        self.delete_dialog(ctx);

        if self.dirty {
            self.dirty = false;
            if let Err(e) = storage::save(&self.budget) {
                self.error(format!("Couldn't save your data: {e}"));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = storage::save(&self.budget);
    }
}
