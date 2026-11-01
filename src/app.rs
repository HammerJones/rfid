//! The user interface (egui). Runs the same on Windows, Linux, macOS and Android.

use crate::bank::{self, Recurring, Txn};
use crate::csvio::{self, money, ImportResult};
use crate::daily::{self, Purchase};
use crate::model::{self, Budget, Expense, Frequency, IncomeMode};
use crate::storage;
use crate::theme;
use std::collections::HashMap;
use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{self, Color32, RichText};

mod food_ui;
use food_ui::{FoodForms, FoodSection, RecipeEditor, RecipeView};

const SAMPLE_SHEET: &str = include_str!("../tests/sample_sheet.csv");

const TABS: [Tab; 7] = [Tab::Calendar, Tab::Overview, Tab::Expenses, Tab::Today, Tab::Food, Tab::Settings, Tab::ImportExport];

/// (icon, full label, short label for the phone nav bar)
fn tab_info(t: Tab) -> (&'static str, &'static str, &'static str) {
    match t {
        Tab::Today => ("⛽", "Daily budget", "Daily"),
        Tab::Food => ("🍽", "Food", "Food"),
        Tab::Overview => ("📊", "Overview", "Month"),
        Tab::Expenses => ("💳", "Expenses", "Bills"),
        Tab::Calendar => ("📅", "Schedule", "Dates"),
        Tab::Settings => ("⚙", "Settings", "Settings"),
        Tab::ImportExport => ("📂", "Import / Export", "Files"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Today,
    Overview,
    Expenses,
    Calendar,
    Food,
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
    picker: Option<Picker>,
    /// Cached result of the Android "All files access" check, and when it was taken.
    #[allow(dead_code)]
    file_access: Option<(bool, f64)>,
    /// Auto-tracked and hand-added purchases (purchases.tsv), and when it was last read.
    purchases: Vec<Purchase>,
    purchases_read_at: f64,
    #[allow(dead_code)]
    notif_access: Option<(bool, f64)>,
    add_amount: f64,
    add_merchant: String,
    show_all_purchases: bool,
    /// Last widget.json written, so it's only rewritten when something changes.
    widget_written: String,
    meals_written: String,
    food_section: FoodSection,
    food_forms: FoodForms,
    recipe_view: Option<RecipeView>,
    recipe_edit: Option<RecipeEditor>,
    open_check_at: f64,
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
        theme::apply(&cc.egui_ctx);
        let (budget, status) = match storage::load() {
            Some(b) => (b, None),
            None => (
                sample_budget(),
                Some(Status::Info(
                    "Loaded sample data. Import your own spreadsheet from Import / Export, or edit anything here.".into(),
                )),
            ),
        };
        let mut budget = budget;
        if !budget.food.samples_added && budget.food.recipes.is_empty() {
            if let Ok(list) = crate::food::parse_recipes(crate::food::SAMPLE_RECIPES) {
                for r in list {
                    budget.food.upsert_recipe(r);
                }
            }
            budget.food.samples_added = true;
        }
        BudgetApp {
            budget,
            tab: Tab::Today,
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
            picker: None,
            file_access: None,
            purchases: daily::load_purchases(&storage::data_dir()),
            purchases_read_at: 0.0,
            notif_access: None,
            add_amount: 0.0,
            add_merchant: String::new(),
            show_all_purchases: false,
            widget_written: String::new(),
            meals_written: String::new(),
            food_section: FoodSection::Plan,
            food_forms: FoodForms::default(),
            recipe_view: None,
            recipe_edit: None,
            open_check_at: -10.0,
        }
    }

    fn info(&mut self, s: impl Into<String>) {
        self.status = Some(Status::Info(s.into()));
    }
    fn error(&mut self, s: impl Into<String>) {
        self.status = Some(Status::Error(s.into()));
    }

    fn month_nav(&mut self, ui: &mut egui::Ui) {
        theme::card().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("◀").size(16.0)).min_size(egui::vec2(40.0, 34.0))).clicked() {
                    self.view_month = model::add_months(self.view_month, -1);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("▶").size(16.0)).min_size(egui::vec2(40.0, 34.0))).clicked() {
                        self.view_month = model::add_months(self.view_month, 1);
                    }
                    let is_now = self.view_month == first_of(today());
                    if !is_now && ui.button(RichText::new("Today").color(theme::ACCENT)).clicked() {
                        self.view_month = first_of(today());
                    }
                    ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                        ui.label(RichText::new(self.view_month.format("%B %Y").to_string()).size(19.0).strong());
                    });
                });
            });
        });
        ui.add_space(4.0);
    }

    fn category_colors(&self) -> HashMap<String, usize> {
        self.budget.ordered_categories().into_iter().enumerate().map(|(i, c)| (c, i)).collect()
    }

    // ---------------------------------------------------------------- Today
    /// Keep widget.json in step with the budget (the home screen widget reads it).
    fn update_widget_file(&mut self) {
        let json = daily::widget_json(&self.budget, today());
        if json != self.widget_written {
            daily::write_widget_file(&storage::data_dir(), &self.budget, today());
            self.widget_written = json;
            #[cfg(target_os = "android")]
            crate::android_perm::refresh_widget();
        }
    }

    fn reload_purchases(&mut self) {
        self.purchases = daily::load_purchases(&storage::data_dir());
    }

    #[allow(unused_variables)]
    fn notification_banner(&mut self, ui: &mut egui::Ui) {
        #[cfg(target_os = "android")]
        {
            let now = ui.input(|i| i.time);
            let fresh = matches!(self.notif_access, Some((_, t)) if now - t < 2.0);
            if !fresh {
                self.notif_access = Some((crate::android_perm::has_notification_access(), now));
            }
            if matches!(self.notif_access, Some((true, _))) {
                return;
            }
            theme::tinted_card(theme::AMBER).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("🔔  Turn on auto-tracking").strong().color(theme::AMBER));
                ui.label(theme::muted(
                    "Budget logs purchases from Google Wallet and PayPal notifications. Tap below and turn on Notification access for Budget.",
                ));
                ui.add_space(2.0);
                ui.label(theme::muted(
                    "If Android says it's a restricted setting: open App info, tap ⋮ (top right) > Allow restricted settings, then try again.",
                ).size(12.5));
                ui.horizontal_wrapped(|ui| {
                    if theme::primary_button(ui, "🔔  Notification access").clicked() {
                        if let Err(e) = crate::android_perm::open_notification_settings() {
                            self.error(format!("Couldn't open settings ({e}). Look in Settings > Notifications > Device & app notifications."));
                        }
                        self.notif_access = None;
                    }
                    if ui.button("ℹ  App info").clicked() {
                        let _ = crate::android_perm::open_app_info();
                        self.notif_access = None;
                    }
                });
            });
            ui.add_space(4.0);
        }
    }

    fn today_tab(&mut self, ui: &mut egui::Ui) {
        // Pick up new purchases the background listener logged.
        let now = ui.input(|i| i.time);
        if now - self.purchases_read_at > 2.0 {
            self.purchases_read_at = now;
            self.reload_purchases();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(3));

        let day = today();
        let st = daily::status(&self.budget, &self.purchases, day);
        let fuel = st.fuel();
        let color = [theme::GOOD, theme::AMBER, theme::BAD][st.level() as usize];

        theme::tinted_card(color).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let kind = if !self.budget.daily.uses_schedule() { "" } else if st.work_day { "  ·  💼 Work day" } else { "  ·  🏠 Off day" };
            ui.label(theme::muted(format!("{}{}", day.format("%A, %B %-d"), kind)));
            ui.horizontal(|ui| {
                let w = (ui.available_width() * 0.45).clamp(130.0, 220.0);
                fuel_dial(ui, fuel, color, w);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    let (amount, caption) = if st.left_today >= 0.0 {
                        (money(st.left_today), "left today")
                    } else {
                        (money(-st.left_today), "over today")
                    };
                    ui.label(RichText::new(amount).size(30.0).strong().color(color));
                    ui.label(RichText::new(caption).size(16.0).color(theme::TEXT));
                    ui.label(theme::muted(format!("of {} you can spend today", money(st.today_max))));
                });
            });
            ui.add_space(6.0);
            ui.columns(3, |c| {
                mini_stat(&mut c[0], "Spent today", &money(st.spent_today));
                mini_stat(&mut c[1], "Days left", &st.days_left.to_string());
                mini_stat(&mut c[2], "Left this month", &money(st.month_money - st.spent_before_today - st.spent_today));
            });
        });
        ui.label(
            theme::muted(format!(
                "Spending money this month: {}. Go over today and tomorrow's amount shrinks; spend less and it grows.",
                money(st.month_money)
            ))
            .size(12.5),
        );

        // Left-over line (rent) with no real amount: the daily budget can't account for it.
        if let Some(e) = self.budget.expenses.iter().find(|e| e.is_remainder && e.amount <= 0.0).cloned() {
            if self.budget.daily.monthly_override <= 0.0 {
                theme::tinted_card(theme::AMBER).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(format!("⚠  What do you actually pay for {}?", e.name.to_lowercase())).strong().color(theme::AMBER));
                    ui.label(theme::muted(format!(
                        "\"{}\" is your left-over line, so it has no fixed amount. Enter what you really pay so the daily budget sets it aside.",
                        e.name
                    )));
                    if ui.button(format!("✏  Edit {}", e.name)).clicked() {
                        self.editor = Some(Editor::from_expense(&e));
                    }
                });
            }
        }

        self.notification_banner(ui);

        // Add by hand.
        theme::section_title(ui, "➕", "Add a purchase");
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label("$");
                ui.add(egui::DragValue::new(&mut self.add_amount).speed(0.5).range(0.0..=100_000.0).fixed_decimals(2));
                ui.add(egui::TextEdit::singleline(&mut self.add_merchant).hint_text("Where (cash, online…)").desired_width(f32::INFINITY));
            });
            ui.add_space(4.0);
            if ui.add_enabled(self.add_amount > 0.0, egui::Button::new("Add")).clicked() {
                let merchant = if self.add_merchant.trim().is_empty() { "Purchase".to_string() } else { self.add_merchant.trim().to_string() };
                let time = Local::now().format("%H:%M").to_string();
                match daily::add_manual(&storage::data_dir(), day, &time, self.add_amount, &merchant) {
                    Ok(()) => {
                        self.info(format!("Added {} at {}.", money(self.add_amount), merchant));
                        self.add_amount = 0.0;
                        self.add_merchant.clear();
                        self.reload_purchases();
                        #[cfg(target_os = "android")]
                        crate::android_perm::refresh_widget();
                    }
                    Err(e) => self.error(format!("Couldn't add it: {e}")),
                }
            }
        });

        // Purchases this month.
        theme::section_title(ui, "📋", "Purchases this month");
        let start = first_of(day);
        let list: Vec<Purchase> = self.purchases.iter().filter(|p| p.date >= start).cloned().collect();
        if list.is_empty() {
            ui.label(theme::muted(
                "Nothing logged yet. Purchases show up here when Google Wallet or PayPal notifies you, or when you add one above.",
            ));
        }
        ui.label(theme::muted(format!(
            "🏠 = a bill already in your budget (rent, subscriptions). 🚫 = don't count it. Purchases of {} or more are big purchases and aren't counted.",
            money(self.budget.daily.small_cutoff)
        )).size(12.0));
        let shown = if self.show_all_purchases { list.len() } else { list.len().min(25) };
        let mut toggle_bill: Option<String> = None;
        let mut toggle_ignore: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut last_date: Option<NaiveDate> = None;
        for p in &list[..shown] {
            if last_date != Some(p.date) {
                last_date = Some(p.date);
                let label = if p.date == day { "Today".to_string() } else { p.date.format("%a %b %-d").to_string() };
                ui.add_space(4.0);
                ui.label(RichText::new(label).strong().color(theme::MUTED));
            }
            let bill = daily::is_bill(&self.budget, &p.merchant);
            let ignored = self.budget.daily.ignored.contains(&p.id);
            let big = daily::is_big(&self.budget, p.amount);
            let counted = !bill && !ignored && !big;
            theme::card().inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut bill_btn = egui::Button::new(if bill { RichText::new("🏠").color(theme::ACCENT) } else { theme::muted("🏠") });
                    if bill {
                        bill_btn = bill_btn.fill(theme::mix(theme::CARD, theme::ACCENT, 0.25));
                    }
                    let mut ign_btn = egui::Button::new(if ignored { RichText::new("🚫").color(theme::BAD) } else { theme::muted("🚫") });
                    if ignored {
                        ign_btn = ign_btn.fill(theme::mix(theme::CARD, theme::BAD, 0.25));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if p.source == "manual" && ui.button(theme::muted("🗑")).on_hover_text("Delete").clicked() {
                            remove = Some(p.id.clone());
                        }
                        if ui.add(ign_btn).on_hover_text("Don't count this one").clicked() {
                            toggle_ignore = Some(p.id.clone());
                        }
                        if ui.add(bill_btn).on_hover_text("Bill already in my budget").clicked() {
                            toggle_bill = Some(p.merchant.clone());
                        }
                        let amount_text = if p.amount < 0.0 { format!("+{}", money(-p.amount)) } else { money(p.amount) };
                        let amount_color = if !counted {
                            theme::MUTED
                        } else if p.amount < 0.0 {
                            theme::GOOD
                        } else {
                            theme::TEXT
                        };
                        ui.label(RichText::new(amount_text).strong().color(amount_color));
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            let mut title = RichText::new(&p.merchant).color(theme::TEXT);
                            if !counted {
                                title = title.strikethrough().color(theme::MUTED);
                            }
                            ui.add(egui::Label::new(title).truncate());
                            let note = if bill { " · bill" } else if ignored { " · not counted" } else if big { " · big purchase" } else { "" };
                            ui.add(egui::Label::new(theme::muted(format!("{} · {}{}", p.time, p.source_label(), note)).size(12.0)).truncate());
                        });
                    });
                });
            });
        }
        if list.len() > shown && ui.button(format!("Show all {}", list.len())).clicked() {
            self.show_all_purchases = true;
        }
        if let Some(m) = toggle_bill {
            let key = daily::bill_key(&m);
            if daily::is_bill(&self.budget, &m) {
                let upper = m.to_ascii_uppercase();
                self.budget.daily.bill_merchants.retain(|k| !k.split(' ').all(|w| upper.contains(w)));
            } else {
                self.budget.daily.bill_merchants.push(key);
            }
            self.dirty = true;
        }
        if let Some(id) = toggle_ignore {
            if let Some(i) = self.budget.daily.ignored.iter().position(|x| x == &id) {
                self.budget.daily.ignored.remove(i);
            } else {
                self.budget.daily.ignored.push(id);
            }
            self.dirty = true;
        }
        if let Some(id) = remove {
            if let Err(e) = daily::remove_manual(&storage::data_dir(), &id) {
                self.error(format!("Couldn't delete it: {e}"));
            }
            self.reload_purchases();
            #[cfg(target_os = "android")]
            crate::android_perm::refresh_widget();
        }

        // Settings for the daily amount.
        ui.add_space(6.0);
        theme::section_title(ui, "⚙", "Daily budget settings");
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let auto = (self.budget.settings.monthly_income() - self.budget.settings.monthly_savings() - self.budget.monthly_bills(day)).max(0.0);
            let mut show = !self.budget.daily.hide_notification;
            if ui.checkbox(&mut show, "Show today's budget in my notification bar").changed() {
                self.budget.daily.hide_notification = !show;
                self.dirty = true;
            }
            #[cfg(target_os = "android")]
            if show && !crate::android_perm::has_notification_permission() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Notifications are off for Budget.").color(theme::AMBER));
                    if ui.button("🔔  Allow notifications").clicked() {
                        let _ = crate::android_perm::open_app_notification_settings();
                    }
                });
            }
            ui.separator();

            // Big-purchase cutoff.
            ui.horizontal_wrapped(|ui| {
                ui.label("Leave out purchases of");
                if ui
                    .add(egui::DragValue::new(&mut self.budget.daily.small_cutoff).prefix("$").speed(1.0).range(0.0..=1000.0).fixed_decimals(0))
                    .changed()
                {
                    self.dirty = true;
                }
                ui.label("or more");
            });
            ui.label(theme::muted("Groceries, gas and other big buys don't count against the daily amount. 0 = count everything.").size(12.5));
            ui.separator();

            // Weekly schedule.
            ui.label(RichText::new("📅  My work days").strong());
            ui.horizontal_wrapped(|ui| {
                for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
                    let on = self.budget.daily.work_days.contains(&(i as u8));
                    let text = if on { RichText::new(*name).strong().color(theme::BG) } else { RichText::new(*name) };
                    let mut btn = egui::Button::new(text).min_size(egui::vec2(40.0, 30.0));
                    if on {
                        btn = btn.fill(theme::VIOLET);
                    }
                    if ui.add(btn).clicked() {
                        if on {
                            self.budget.daily.work_days.retain(|d| *d != i as u8);
                        } else {
                            self.budget.daily.work_days.push(i as u8);
                            self.budget.daily.work_days.sort();
                        }
                        self.dirty = true;
                    }
                }
            });
            ui.add_space(4.0);
            let mut sched = self.budget.daily.uses_schedule();
            if ui.checkbox(&mut sched, "Separate work-day and off-day budgets").changed() {
                if sched {
                    let (w, o) = self.suggested_day_budgets();
                    let even = (auto / 30.0).max(1.0).round();
                    self.budget.daily.work_day_budget = if w > 0.0 { w } else { even };
                    self.budget.daily.off_day_budget = if o > 0.0 { o } else { even };
                } else {
                    self.budget.daily.work_day_budget = 0.0;
                    self.budget.daily.off_day_budget = 0.0;
                }
                self.dirty = true;
            }
            if sched {
                egui::Grid::new("day_budgets").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("💼 Work day");
                    if ui
                        .add(egui::DragValue::new(&mut self.budget.daily.work_day_budget).prefix("$").speed(0.5).range(0.0..=10_000.0).fixed_decimals(0))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.end_row();
                    ui.label("🏠 Off day");
                    if ui
                        .add(egui::DragValue::new(&mut self.budget.daily.off_day_budget).prefix("$").speed(0.5).range(0.0..=10_000.0).fixed_decimals(0))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.end_row();
                });
                let plan = daily::month_plan(&self.budget, day);
                let color = if plan > auto + 0.5 { theme::AMBER } else { theme::GOOD };
                ui.label(RichText::new(format!("This month's plan adds up to {}. Your budget leaves {} for spending.", money(plan), money(auto))).color(color).size(13.0));
                if plan > auto + 0.5 {
                    ui.label(theme::muted("That's more than your budget leaves. Lower one of the amounts or your bills.").size(12.5));
                }
            }

            // Suggestion from bank statements.
            ui.add_space(6.0);
            let d = self.budget.daily.clone();
            if d.hist_work_avg > 0.0 || d.hist_off_avg > 0.0 {
                theme::tinted_card(theme::SKY).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("📈  From your statement").strong().color(theme::SKY));
                    ui.label(theme::muted(format!(
                        "Small purchases (under {}), {}: you averaged {} per work day and {} per off day.",
                        money(d.small_cutoff),
                        d.hist_note,
                        money(d.hist_work_avg),
                        money(d.hist_off_avg)
                    )));
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Cut off-day spending by");
                        let mut pct = (self.budget.daily.off_day_cut * 100.0).round();
                        if ui.add(egui::Slider::new(&mut pct, 0.0..=90.0).step_by(5.0).suffix("%")).changed() {
                            self.budget.daily.off_day_cut = pct / 100.0;
                            self.dirty = true;
                        }
                    });
                    let (w, o) = self.suggested_day_budgets();
                    ui.label(format!("Suggested: {} per work day, {} per off day.", money(w), money(o)));
                    if theme::primary_button(ui, "Use these amounts").clicked() {
                        self.budget.daily.work_day_budget = w;
                        self.budget.daily.off_day_budget = o.max(1.0);
                        self.dirty = true;
                    }
                });
            } else {
                ui.label(theme::muted("Import a bank or PayPal statement (Files tab) and I'll suggest work-day and off-day amounts from what you actually spend.").size(12.5));
            }
            ui.separator();

            // Monthly total override (used when there's no weekly schedule).
            ui.label(theme::muted(format!(
                "Your budget leaves {} income − {} savings − {} bills = {} a month for spending.",
                money(self.budget.settings.monthly_income()),
                money(self.budget.settings.monthly_savings()),
                money(self.budget.monthly_bills(day)),
                money(auto)
            )).size(12.5));
            if !self.budget.daily.uses_schedule() {
                let mut custom = self.budget.daily.monthly_override > 0.0;
                if ui.checkbox(&mut custom, "Use my own monthly amount").changed() {
                    self.budget.daily.monthly_override = if custom { auto.max(1.0).round() } else { 0.0 };
                    self.dirty = true;
                }
                if custom {
                    ui.horizontal(|ui| {
                        ui.label("Spending money per month: $");
                        if ui
                            .add(egui::DragValue::new(&mut self.budget.daily.monthly_override).speed(5.0).range(1.0..=1_000_000.0).fixed_decimals(0))
                            .changed()
                        {
                            self.dirty = true;
                        }
                    });
                }
            }
        });
    }

    /// Work-day and off-day amounts suggested from the last statement, rounded to whole dollars.
    fn suggested_day_budgets(&self) -> (f64, f64) {
        let d = &self.budget.daily;
        (d.hist_work_avg.round(), (d.hist_off_avg * (1.0 - d.off_day_cut)).round())
    }

    // ---------------------------------------------------------------- Overview
    fn overview(&mut self, ui: &mut egui::Ui) {
        self.month_nav(ui);
        let s = self.budget.summarize(self.view_month);
        let st = self.budget.settings.clone();
        let colors = self.category_colors();

        let left_color = if s.left_over < -0.004 { theme::BAD } else { theme::GOOD };
        let cards = [
            ("💵", "Income".to_string(), money(s.income), theme::SKY),
            ("💰", format!("Savings {:.0}%", st.savings_rate * 100.0), money(s.savings), theme::VIOLET),
            ("💳", "Expenses".to_string(), money(s.total_expenses), theme::AMBER),
            ("🎯", "Left over".to_string(), money(s.left_over), left_color),
        ];
        // Four across on wide screens, two by two on phones.
        let per_row = if ui.available_width() > 620.0 { 4 } else { 2 };
        for chunk in cards.chunks(per_row) {
            ui.columns(per_row, |cols| {
                for (col, (icon, title, value, color)) in cols.iter_mut().zip(chunk) {
                    stat_card(col, icon, title, value, *color);
                }
            });
        }
        ui.add_space(4.0);

        // Highlight: the left-over line (e.g. max rent).
        if let Some(rem) = s.categories.iter().flat_map(|c| &c.items).find(|i| i.is_remainder) {
            let over = rem.monthly_cost < 0.0;
            let color = if over { theme::BAD } else { theme::ACCENT };
            theme::tinted_card(color).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if over {
                    ui.label(RichText::new(format!("⚠  Over budget by {}", money(-rem.monthly_cost))).strong().color(theme::BAD));
                    ui.label(theme::muted(format!("Your other expenses and savings already use all your income, so there's nothing left for {}.", rem.name)));
                } else {
                    ui.label(theme::muted(format!("Max {} you can afford", rem.name.to_lowercase())));
                    ui.label(RichText::new(money(rem.monthly_cost)).size(26.0).strong().color(color));
                }
            });
        }

        // Savings goal.
        let goal = st.savings_goal.max(0.01);
        let frac = (st.current_savings / goal).clamp(0.0, 1.0) as f32;
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("💰  Emergency fund").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::muted(format!("{} / {}", money(st.current_savings), money(st.savings_goal))));
                });
            });
            theme::bar(ui, frac, theme::VIOLET, 10.0);
            let months = match s.months_to_goal {
                Some(0) => "Goal reached!".to_string(),
                Some(m) => format!("{m} months to go at {}/month", money(s.savings)),
                None => "Set a savings rate in Settings to reach this goal".to_string(),
            };
            ui.label(theme::muted(months).small());
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if theme::primary_button(ui, "➕  Add expense").clicked() {
                let cat = self.budget.ordered_categories().first().cloned().unwrap_or_default();
                self.editor = Some(Editor::new(&cat, self.view_month));
            }
            if ui.button("📤  Export summary").clicked() {
                self.export_summary(ui.ctx());
            }
        });
        ui.add_space(4.0);

        let mut edit: Option<u64> = None;
        for cat in &s.categories {
            let color = theme::category_color(colors.get(&cat.name).copied().unwrap_or(0));
            let share = if s.income > 0.0 { (cat.total() / s.income).clamp(0.0, 1.0) as f32 } else { 0.0 };
            theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(theme::category_icon(&cat.name)).size(20.0).color(color));
                    ui.label(RichText::new(&cat.name).size(17.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(money(cat.total())).size(17.0).strong().color(color));
                    });
                });
                theme::bar(ui, share, color, 6.0);
                ui.label(theme::muted(format!("{:.0}% of income", share * 100.0)).small());
                ui.add_space(2.0);
                for it in &cat.items {
                    let due = it.due_dates.iter().map(|d| ordinal(d.day())).collect::<Vec<_>>().join(", ");
                    let mut sub = Vec::new();
                    if it.is_remainder {
                        sub.push("left-over".to_string());
                    }
                    if !due.is_empty() {
                        sub.push(format!("due {due}"));
                    }
                    if let Some(b) = it.remaining_balance {
                        sub.push(format!("{} left", money(b)));
                    }
                    if let Some(p) = it.remaining_payments {
                        sub.push(format!("{p} payments left"));
                    }
                    if item_row(ui, color, &it.name, &sub.join("  ·  "), &money(it.monthly_cost), theme::TEXT).clicked() {
                        edit = Some(it.expense_id);
                    }
                }
            });
        }
        if s.categories.is_empty() {
            theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(theme::muted("No expenses are due this month. Add one, or import a CSV."));
            });
        }
        if let Some(id) = edit {
            self.open_editor(id);
        }
    }

    // ---------------------------------------------------------------- Expenses
    fn expenses(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if theme::primary_button(ui, "➕  Add").clicked() {
                let cat = self.budget.ordered_categories().first().cloned().unwrap_or_default();
                self.editor = Some(Editor::new(&cat, first_of(today())));
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("🔍 Search expenses")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.add_space(4.0);
        let t = today();
        let filter = self.filter.to_lowercase();
        let colors = self.category_colors();
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
            let color = theme::category_color(colors.get(&cat).copied().unwrap_or(0));
            theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(theme::category_icon(&cat)).size(20.0).color(color));
                    ui.label(RichText::new(&cat).size(17.0).strong());
                    ui.label(theme::muted(format!("{}", items.len())));
                });
                for e in items {
                    let next = e.due_dates_between(t, t + Duration::days(800)).first().copied();
                    let when = match next {
                        Some(d) => format!("next {}", d.format("%b %-d")),
                        None => "finished".into(),
                    };
                    let amount = if e.is_remainder { "left-over".to_string() } else { money(e.amount) };
                    let sub = format!("{}  ·  {}", e.frequency.label(), when);
                    let (row, action) = expense_row(ui, color, &e.name, &sub, &amount);
                    match action {
                        Some(RowAction::Edit) => edit = Some(e.id),
                        Some(RowAction::Delete) => delete = Some(e.id),
                        None if row.clicked() => edit = Some(e.id),
                        None => {}
                    }
                }
            });
        }
        if self.budget.expenses.is_empty() {
            theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(theme::muted("No expenses yet. Tap Add, or import a CSV."));
            });
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
        let colors = self.category_colors();
        let rem_cost: f64 = s.categories.iter().flat_map(|c| &c.items).filter(|i| i.is_remainder).map(|i| i.monthly_cost).sum();
        let color_of = |e: &Expense| theme::category_color(colors.get(&e.category).copied().unwrap_or(0));
        let amount_of = |e: &Expense| if e.is_remainder { rem_cost } else { e.amount };

        let gap = 4.0;
        let cell_w = ((ui.available_width() - 6.0 * gap - 14.0) / 7.0).floor().max(36.0);
        let compact = cell_w < 90.0;
        let cell_h = if compact { 58.0 } else { 90.0 };

        let month_total: f64 = due.iter().map(|(_, e)| amount_of(e)).sum();
        ui.columns(2, |cols| {
            stat_card(&mut cols[0], "📅", "Payments due", &due.len().to_string(), theme::SKY);
            stat_card(&mut cols[1], "💳", "Total this month", &money(month_total), theme::AMBER);
        });
        ui.add_space(4.0);

        egui::Grid::new("cal").spacing(egui::vec2(gap, gap)).show(ui, |ui| {
            for w in ["S", "M", "T", "W", "T", "F", "S"] {
                ui.allocate_ui(egui::vec2(cell_w, 18.0), |ui| {
                    ui.vertical_centered(|ui| ui.label(theme::muted(w).small().strong()));
                });
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
                let meal_count = self.budget.food.plan.iter().filter(|m| m.date == day).count();
                let selected = self.selected_day == Some(day);
                let is_today = day == today();
                let busy = !todays.is_empty();
                let mut frame = egui::Frame::new()
                    .fill(if busy { theme::mix(theme::CARD, theme::AMBER, 0.10) } else { theme::CARD })
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(5))
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER));
                if selected {
                    frame = frame.stroke(egui::Stroke::new(2.0_f32, theme::VIOLET));
                } else if is_today {
                    frame = frame.stroke(egui::Stroke::new(2.0_f32, theme::ACCENT));
                }
                let resp = frame
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(cell_w - 10.0, cell_h - 10.0));
                        ui.set_max_width(cell_w - 10.0);
                        ui.vertical(|ui| {
                            let num = RichText::new(day.day().to_string()).strong();
                            ui.horizontal(|ui| {
                                ui.label(if is_today { num.color(theme::ACCENT) } else { num });
                                if meal_count > 0 {
                                    let t = if compact { "🍽".to_string() } else { format!("🍽{meal_count}") };
                                    ui.label(RichText::new(t).size(11.0).color(theme::GOOD));
                                }
                            });
                            if busy {
                                if compact {
                                    // One colored dot per payment (up to 4), then the day's total.
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(cell_w - 10.0, 8.0), egui::Sense::hover());
                                    for (i, (_, e)) in todays.iter().take(4).enumerate() {
                                        let c = rect.left_center() + egui::vec2(4.0 + i as f32 * 9.0, 0.0);
                                        ui.painter().circle_filled(c, 3.5, color_of(e));
                                    }
                                    let total: f64 = todays.iter().map(|(_, e)| amount_of(e)).sum();
                                    ui.add(egui::Label::new(RichText::new(short_money(total)).size(11.0).color(theme::AMBER)).truncate());
                                } else {
                                    for (_, e) in todays.iter().take(3) {
                                        ui.add(
                                            egui::Label::new(RichText::new(format!("● {}", e.name)).size(11.0).color(color_of(e)))
                                                .truncate(),
                                        );
                                    }
                                    if todays.len() > 3 {
                                        ui.label(theme::muted(format!("+{} more", todays.len() - 3)).size(11.0));
                                    }
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
        ui.add_space(6.0);

        let mut edit = None;
        if let Some(sel) = self.selected_day {
            let list: Vec<(String, String, f64, u64, egui::Color32)> = self
                .budget
                .schedule(sel, sel)
                .into_iter()
                .map(|(_, e)| (e.name.clone(), e.category.clone(), amount_of(e), e.id, color_of(e)))
                .collect();
            theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(sel.format("%A, %B %-d").to_string()).size(17.0).strong());
                if list.is_empty() {
                    ui.label(theme::muted("Nothing due. ✔"));
                }
                for (name, cat, amt, id, color) in &list {
                    if item_row(ui, *color, name, cat, &money(*amt), theme::AMBER).clicked() {
                        edit = Some(*id);
                    }
                }
                self.day_meals(ui, sel);
            });
        }

        let t = today();
        let upcoming: Vec<(NaiveDate, String, f64, u64, egui::Color32)> = self
            .budget
            .schedule(t, t + Duration::days(30))
            .into_iter()
            .map(|(d, e)| (d, e.name.clone(), amount_of(e), e.id, color_of(e)))
            .collect();
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("⏵  Next 30 days").size(17.0).strong());
            if upcoming.is_empty() {
                ui.label(theme::muted("Nothing due in the next 30 days."));
            }
            for (d, name, amt, id, color) in &upcoming {
                if item_row(ui, *color, name, &d.format("%a, %b %-d").to_string(), &money(*amt), theme::TEXT).clicked() {
                    edit = Some(*id);
                }
            }
        });
        if let Some(id) = edit {
            self.open_editor(id);
        }
    }

    // ---------------------------------------------------------------- Bank statements
    fn bank_tab(&mut self, ui: &mut egui::Ui) {
        theme::section_title(ui, "🏦", "Bank & card statements");
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
            if theme::primary_button(ui, "📂  Choose statement CSV").clicked() {
                self.picker = Some(Picker::open(PickFor::Statement));
            }
            if !self.bank_txns.is_empty() && ui.button("Clear").clicked() {
                self.bank_txns.clear();
                self.bank_found.clear();
                self.bank_files.clear();
            }
        });
        #[cfg(not(target_os = "android"))]
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
        theme::section_title(ui, "🔁", "Recurring payments found");
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
                    let key = r.key.to_ascii_uppercase();
                    if !self.budget.daily.bill_merchants.contains(&key) {
                        self.budget.daily.bill_merchants.push(key);
                    }
                }
                self.bank_found.retain(|(_, s)| !*s);
                self.dirty = true;
                self.info(format!("Added {} recurring payments.", chosen.len()));
            }
        }
        ui.separator();

        // Monthly totals.
        theme::section_title(ui, "📈", "Money in and out by month");
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
        let moved: f64 = months.values().map(|t| t.transfers).sum();
        if moved > 0.0 {
            ui.label(
                RichText::new(format!(
                    "🔁 {} moved between your own accounts (bank top-ups, card payments, transfers) isn't counted as money in or out. Pending holds are skipped.",
                    money(moved)
                ))
                .weak(),
            );
        }
        if months.len() > 1 {
            let avg_out: f64 = months.values().map(|t| t.money_out).sum::<f64>() / months.len() as f64;
            ui.label(format!("Average spending: {} per month. Partial first/last months pull this down.", money(avg_out)));
        }
        ui.separator();

        // Spending breakdowns.
        theme::section_title(ui, "📊", "Spending by category (best guess)");
        let cats = bank::by_category(&self.bank_txns);
        let total: f64 = cats.iter().map(|c| c.1).sum::<f64>().max(0.01);
        for (name, amt) in &cats {
            let idx = self.budget.ordered_categories().iter().position(|c| c == name).unwrap_or(7);
            let color = if name == "Uncategorized" { theme::MUTED } else { theme::category_color(idx) };
            ui.horizontal(|ui| {
                ui.label(RichText::new(theme::category_icon(name)).color(color));
                ui.label(name.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::muted(format!("{:.0}%", amt / total * 100.0)));
                    ui.label(RichText::new(money(*amt)).strong().color(color));
                });
            });
            theme::bar(ui, (amt / total) as f32, color, 6.0);
        }
        ui.add_space(6.0);
        theme::section_title(ui, "🏷", "Top merchants");
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
                // Work-day / off-day averages for the daily budget suggestion.
                let avg = daily::day_averages(&self.budget, &self.bank_txns);
                if avg.work_days + avg.off_days >= 7 {
                    self.budget.daily.hist_work_avg = avg.work_avg;
                    self.budget.daily.hist_off_avg = avg.off_avg;
                    self.budget.daily.hist_note = format!(
                        "{} to {} ({} work days, {} off days)",
                        avg.from.map(|d| d.format("%b %-d").to_string()).unwrap_or_default(),
                        avg.to.map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default(),
                        avg.work_days,
                        avg.off_days
                    );
                    self.dirty = true;
                }
                self.info(format!("Read {n} transactions from {name}."));
            }
            Err(e) => self.error(format!("{name}: {e}")),
        }
    }

    // ---------------------------------------------------------------- Settings
    fn settings(&mut self, ui: &mut egui::Ui) {
        let before = self.budget.settings.clone();
        let st = &mut self.budget.settings;
        theme::section_title(ui, "💵", "Income");
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
        theme::section_title(ui, "💰", "Savings");
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
        theme::section_title(ui, "✏", "Note at the top of the exported summary");
        ui.add(egui::TextEdit::multiline(&mut st.note).desired_rows(3).desired_width(f32::INFINITY));
        if self.budget.settings != before {
            self.dirty = true;
        }

        ui.add_space(8.0);
        theme::section_title(ui, "🏷", "Categories");
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
                if ui.small_button("⏶").on_hover_text("Move up").clicked() && i > 0 {
                    action = Some(("up", i));
                }
                if ui.small_button("⏷").on_hover_text("Move down").clicked() && i + 1 < cats.len() {
                    action = Some(("down", i));
                }
                if ui.small_button(RichText::new("✏").color(theme::ACCENT)).on_hover_text("Rename").clicked() {
                    self.renaming = Some((i, c.clone()));
                }
                if ui.small_button(RichText::new("🗑").color(theme::BAD)).on_hover_text("Delete").clicked() {
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
        theme::section_title(ui, "📱", "Screen spacing");
        ui.label(theme::muted("If the top bar is hidden behind your phone's status bar or camera, raise the top space. Raise the bottom space if the gesture bar covers the tabs."));
        egui::Grid::new("safe").num_columns(2).show(ui, |ui| {
            ui.label("Top space");
            if ui.add(egui::Slider::new(&mut self.budget.settings.safe_top, 0.0..=110.0).suffix(" pt")).changed() {
                self.dirty = true;
            }
            ui.end_row();
            ui.label("Bottom space");
            if ui.add(egui::Slider::new(&mut self.budget.settings.safe_bottom, 0.0..=110.0).suffix(" pt")).changed() {
                self.dirty = true;
            }
            ui.end_row();
        });
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Your data is saved automatically to {}", storage::budget_path().display())).weak());
        if ui.button("Reset to sample data").clicked() {
            self.budget = sample_budget();
            self.dirty = true;
            self.info("Reset to sample data.");
        }
    }

    // ---------------------------------------------------------------- Import / Export
    fn import_export(&mut self, ui: &mut egui::Ui) {
        self.file_access_banner(ui);
        theme::section_title(ui, "📥", "Import");
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
        if theme::primary_button(ui, "📂  Choose a CSV file").clicked() {
            self.picker = Some(Picker::open(PickFor::Budget));
        }
        #[cfg(not(target_os = "android"))]
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
        theme::section_title(ui, "📤", "Export");
        self.month_nav(ui);
        ui.horizontal_wrapped(|ui| {
            if theme::primary_button(ui, "Monthly summary (Excel, colored)").clicked() {
                self.export_summary_xlsx(ui.ctx());
            }
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
        ui.label(RichText::new("The monthly summary uses the Monthly Costs spreadsheet layout and colors, with totals added at the bottom. Excel files open in Google Sheets and phone spreadsheet apps too.").weak());
        ui.add_space(10.0);
        ui.separator();
        self.bank_tab(ui);
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

    fn export_summary_xlsx(&mut self, ctx: &egui::Context) {
        let s = self.budget.summarize(self.view_month);
        match crate::xlsx::export_summary(&self.budget, &s) {
            Ok(bytes) => {
                let name = format!("Budget Summary - {}.xlsx", self.view_month.format("%Y-%m"));
                self.save_bytes(ctx, &name, "Excel", "xlsx", bytes);
            }
            Err(e) => self.error(format!("Couldn't build the Excel file: {e}")),
        }
    }

    /// Like save_csv, for binary files.
    fn save_bytes(&mut self, ctx: &egui::Context, name: &str, kind: &str, ext: &str, bytes: Vec<u8>) {
        let _ = ctx;
        #[cfg(not(target_os = "android"))]
        {
            let dialog = rfd::FileDialog::new()
                .add_filter(kind, &[ext])
                .set_file_name(name)
                .set_directory(storage::export_dir());
            if let Some(path) = dialog.save_file() {
                match std::fs::write(&path, &bytes) {
                    Ok(()) => self.info(format!("Saved {}", path.display())),
                    Err(e) => self.error(format!("Couldn't save: {e}")),
                }
            }
        }
        #[cfg(target_os = "android")]
        {
            let _ = (kind, ext);
            match storage::write_export(name, &bytes) {
                Ok(p) => self.info(format!("Saved to {}", p.display())),
                Err(e) => self.error(format!("Couldn't save: {e}. {}", PERMISSION_HELP)),
            }
        }
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
            let _ = ctx;
            match storage::write_export(name, &text) {
                Ok(p) => self.info(format!("Saved to {}", p.display())),
                Err(e) => self.error(format!("Couldn't save: {e}. {}", PERMISSION_HELP)),
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
                    if theme::primary_button(ui, "✔  Save").clicked() {
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


const PERMISSION_HELP: &str = "If files don't show up, turn on All files access for Budget: Settings > Apps > Special app access > All files access > Budget.";

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickFor {
    Budget,
    Statement,
    Food,
}

/// A simple built-in file browser. Android has no native file dialog from Rust,
/// so this lists CSV files from shared storage (Downloads, Documents, ...).
struct Picker {
    purpose: PickFor,
    dir: std::path::PathBuf,
    entries: Vec<(std::path::PathBuf, String, bool)>,
    error: Option<String>,
}

impl Picker {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn open(purpose: PickFor) -> Self {
        let dir = storage::picker_roots().into_iter().next().map(|r| r.1).unwrap_or_else(|| std::path::PathBuf::from("."));
        let mut p = Picker { purpose, dir, entries: Vec::new(), error: None };
        p.refresh();
        p
    }

    fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                let mut dirs = Vec::new();
                let mut files = Vec::new();
                for e in rd.flatten() {
                    let path = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    let meta = e.metadata().ok();
                    if path.is_dir() {
                        dirs.push((path, name, true));
                    } else {
                        let lower = name.to_lowercase();
                        if lower.ends_with(".csv") || lower.ends_with(".txt") {
                            let modified = meta.and_then(|m| m.modified().ok()).unwrap_or(std::time::UNIX_EPOCH);
                            files.push((modified, (path, name, false)));
                        }
                    }
                }
                dirs.sort_by_key(|d| d.1.to_lowercase());
                files.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
                self.entries = dirs;
                self.entries.extend(files.into_iter().map(|f| f.1));
            }
            Err(e) => self.error = Some(format!("Can't open this folder ({e}). {PERMISSION_HELP}")),
        }
    }
}

impl BudgetApp {
    /// Android: warn and offer a button when the app can't see other apps' files in Downloads.
    #[allow(unused_variables)]
    fn file_access_banner(&mut self, ui: &mut egui::Ui) {
        #[cfg(target_os = "android")]
        {
            let now = ui.input(|i| i.time);
            let fresh = matches!(self.file_access, Some((_, t)) if now - t < 2.0);
            if !fresh {
                self.file_access = Some((crate::android_perm::has_all_files_access(), now));
            }
            if matches!(self.file_access, Some((true, _))) {
                return;
            }
            theme::tinted_card(theme::AMBER).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("⚠  Budget can't see your Downloads yet").strong().color(theme::AMBER));
                ui.label(theme::muted(
                    "Android only shows files saved by other apps (like a CSV from your browser) to apps with \"All files access\". Tap below, turn it on for Budget, then come back.",
                ));
                if theme::primary_button(ui, "🔓  Grant file access").clicked() {
                    if let Err(e) = crate::android_perm::open_all_files_settings() {
                        self.error(format!("Couldn't open settings ({e}). {PERMISSION_HELP}"));
                    }
                    self.file_access = None;
                }
            });
            ui.add_space(4.0);
        }
    }

    fn picker_window(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.picker.take() else { return };
        let mut open = true;
        let mut chosen: Option<std::path::PathBuf> = None;
        let mut go_to: Option<std::path::PathBuf> = None;
        let width = (ctx.screen_rect().width() - 24.0).clamp(260.0, 520.0);
        let height = (ctx.screen_rect().height() * 0.6).clamp(240.0, 520.0);
        let title = match picker.purpose {
            PickFor::Budget => "Choose a budget CSV",
            PickFor::Statement => "Choose a bank statement",
            PickFor::Food => "Choose a recipes or meal plan CSV",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, self.budget.settings.safe_top + 60.0))
            .show(ctx, |ui| {
                ui.set_width(width - 32.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, path) in storage::picker_roots() {
                        let selected = picker.dir == path;
                        if ui.selectable_label(selected, label).clicked() {
                            go_to = Some(path);
                        }
                    }
                });
                self.file_access_banner(ui);
                ui.label(theme::muted(picker.dir.display().to_string()).small());
                if let Some(parent) = picker.dir.parent() {
                    if ui.button("⬆  Up one folder").clicked() {
                        go_to = Some(parent.to_path_buf());
                    }
                }
                ui.separator();
                egui::ScrollArea::vertical().id_salt("picker_list").max_height(height).show(ui, |ui| {
                    if let Some(err) = &picker.error {
                        ui.colored_label(theme::BAD, err);
                    } else if picker.entries.is_empty() {
                        ui.label(theme::muted(format!("No CSV files here. {PERMISSION_HELP}")));
                    }
                    for (path, name, is_dir) in &picker.entries {
                        let (icon, color) = if *is_dir { ("📁", theme::AMBER) } else { ("📄", theme::ACCENT) };
                        let resp = egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(6, 8))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(icon).color(color));
                                    ui.add(egui::Label::new(RichText::new(name).color(theme::TEXT)).truncate());
                                });
                            })
                            .response
                            .interact(egui::Sense::click());
                        if resp.clicked() {
                            if *is_dir {
                                go_to = Some(path.clone());
                            } else {
                                chosen = Some(path.clone());
                            }
                        }
                    }
                });
            });

        if let Some(dir) = go_to {
            picker.dir = dir;
            picker.refresh();
        }
        if let Some(path) = chosen {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match std::fs::read_to_string(&path) {
                Ok(text) => match picker.purpose {
                    PickFor::Budget => self.preview_import(&text),
                    PickFor::Statement => self.add_statement(&name, &text),
                    PickFor::Food => self.import_food_csv(&name, &text),
                },
                Err(e) => self.error(format!("Couldn't read {name}: {e}. {PERMISSION_HELP}")),
            }
            return; // close the picker
        }
        if open {
            self.picker = Some(picker);
        }
    }
}

/// The classic fuel dial (same look as the widget): E on the left, F on the right.
fn fuel_dial(ui: &mut egui::Ui, frac: f32, color: Color32, width: f32) {
    use std::f32::consts::PI;
    let h = width * 0.66;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let p = ui.painter_at(rect.expand(4.0));
    let stroke = width * 0.1;
    let r = (width / 2.0 - stroke).min(h * 0.78 - stroke);
    let c = egui::pos2(rect.center().x, rect.top() + h * 0.5 + r * 0.42);
    // Angle t: 0 = E (left), 1 = F (right), going over the top.
    let at = |t: f32, rad: f32| {
        let a = PI + PI * t;
        egui::pos2(c.x + rad * a.cos(), c.y + rad * a.sin())
    };
    let arc = |t0: f32, t1: f32, col: Color32| {
        let n = ((t1 - t0) * 48.0).ceil().max(2.0) as usize;
        let pts: Vec<egui::Pos2> = (0..=n).map(|i| at(t0 + (t1 - t0) * i as f32 / n as f32, r)).collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(stroke, col)));
    };
    arc(0.0, 1.0, Color32::from_rgb(10, 13, 22));
    arc(0.0, 0.25, theme::BAD.gamma_multiply(0.3));
    arc(0.25, 0.5, theme::AMBER.gamma_multiply(0.3));
    arc(0.5, 1.0, theme::GOOD.gamma_multiply(0.3));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.005 {
        arc(0.0, f, color);
    }
    for i in 1..4 {
        let t = i as f32 / 4.0;
        p.line_segment([at(t, r - stroke * 0.45), at(t, r + stroke * 0.45)], egui::Stroke::new(stroke * 0.16, Color32::from_white_alpha(150)));
    }
    let needle = egui::Stroke::new(stroke * 0.42, theme::TEXT);
    p.line_segment([c, at(f, r * 0.82)], needle);
    p.circle_filled(c, stroke * 0.52, theme::TEXT);
    let font = egui::FontId::proportional(stroke * 1.15);
    p.text(egui::pos2(c.x - r, c.y + stroke * 1.0), egui::Align2::CENTER_CENTER, "E", font.clone(), theme::BAD);
    p.text(egui::pos2(c.x + r, c.y + stroke * 1.0), egui::Align2::CENTER_CENTER, "F", font, theme::GOOD);
}

fn mini_stat(ui: &mut egui::Ui, title: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(theme::muted(title).size(12.0));
        ui.label(RichText::new(value).strong().color(theme::TEXT));
    });
}

fn stat_card(ui: &mut egui::Ui, icon: &str, title: &str, value: &str, color: Color32) {
    theme::tinted_card(color).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{icon}  {title}")).size(13.0).color(theme::mix(theme::MUTED, color, 0.35)));
            ui.add(egui::Label::new(RichText::new(value).size(21.0).strong().color(color)).truncate());
        });
    });
}

/// "$1.2k" style for tiny calendar cells.
fn short_money(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("${:.1}k", v / 1000.0)
    } else {
        format!("${:.0}", v)
    }
}

/// A tappable list row: colored dot, title with a muted subtitle, amount on the right.
fn item_row(ui: &mut egui::Ui, color: Color32, title: &str, subtitle: &str, amount: &str, amount_color: Color32) -> egui::Response {
    let resp = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(4, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.5, color);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(amount).strong().color(amount_color));
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.add(egui::Label::new(RichText::new(title).color(theme::TEXT)).truncate());
                        if !subtitle.is_empty() {
                            ui.add(egui::Label::new(theme::muted(subtitle).size(12.0)).truncate());
                        }
                    });
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_stroke(resp.rect, egui::CornerRadius::same(8), egui::Stroke::new(1.0_f32, theme::BORDER), egui::StrokeKind::Inside);
    }
    resp
}

enum RowAction {
    Edit,
    Delete,
}

/// Like `item_row`, with edit and delete buttons.
fn expense_row(ui: &mut egui::Ui, color: Color32, title: &str, subtitle: &str, amount: &str) -> (egui::Response, Option<RowAction>) {
    let mut action = None;
    let resp = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(4, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.5, color);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("🗑").color(theme::BAD)).frame(false)).on_hover_text("Delete").clicked() {
                        action = Some(RowAction::Delete);
                    }
                    if ui.add(egui::Button::new(RichText::new("✏").color(theme::ACCENT)).frame(false)).on_hover_text("Edit").clicked() {
                        action = Some(RowAction::Edit);
                    }
                    ui.label(RichText::new(amount).strong());
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.add(egui::Label::new(RichText::new(title).color(theme::TEXT)).truncate());
                        ui.add(egui::Label::new(theme::muted(subtitle).size(12.0)).truncate());
                    });
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    (resp, action)
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
        let narrow = ctx.screen_rect().width() < 720.0;
        let top_pad = self.budget.settings.safe_top.clamp(0.0, 110.0);
        let bottom_pad = self.budget.settings.safe_bottom.clamp(0.0, 110.0);

        // Header. On phones it sits below the status bar and camera cutout.
        egui::TopBottomPanel::top("header")
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin { left: 16, right: 16, top: (top_pad + 10.0) as i8, bottom: 10 }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("💰").size(24.0).color(theme::ACCENT));
                    ui.label(RichText::new("Budget").size(22.0).strong().color(theme::TEXT));
                    if narrow {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (icon, label, _) = tab_info(self.tab);
                            ui.label(theme::muted(format!("{icon} {label}")).size(15.0));
                        });
                    } else {
                        ui.add_space(16.0);
                        for t in TABS {
                            let (icon, label, _) = tab_info(t);
                            let selected = self.tab == t;
                            let text = RichText::new(format!("{icon}  {label}")).color(if selected { theme::ACCENT } else { theme::MUTED });
                            if ui.add(egui::Button::new(text).selected(selected).frame(selected)).clicked() {
                                self.tab = t;
                            }
                        }
                    }
                });
            });

        // Phones: a bottom navigation bar with icons.
        if narrow {
            egui::TopBottomPanel::bottom("nav")
                .show_separator_line(false)
                .frame(
                    egui::Frame::new()
                        .fill(theme::PANEL)
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                        .inner_margin(egui::Margin { left: 4, right: 4, top: 6, bottom: (bottom_pad + 6.0) as i8 }),
                )
                .show(ctx, |ui| {
                    ui.columns(TABS.len(), |cols| {
                        for (col, t) in cols.iter_mut().zip(TABS) {
                            let (icon, _, short) = tab_info(t);
                            let selected = self.tab == t;
                            let color = if selected { theme::ACCENT } else { theme::MUTED };
                            // Paint the icon and label ourselves so the whole cell is one tap target.
                            let (rect, resp) = col.allocate_exact_size(egui::vec2(col.available_width(), 46.0), egui::Sense::click());
                            let painter = col.painter_at(rect.expand(6.0));
                            painter.text(
                                egui::pos2(rect.center().x, rect.top() + 15.0),
                                egui::Align2::CENTER_CENTER,
                                icon,
                                egui::FontId::proportional(22.0),
                                color,
                            );
                            painter.text(
                                egui::pos2(rect.center().x, rect.bottom() - 7.0),
                                egui::Align2::CENTER_CENTER,
                                short,
                                egui::FontId::proportional(11.0),
                                color,
                            );
                            if selected {
                                let r = resp.rect;
                                let bar = egui::Rect::from_center_size(egui::pos2(r.center().x, r.top() - 4.0), egui::vec2(24.0, 3.0));
                                col.painter().rect_filled(bar, egui::CornerRadius::same(2), theme::ACCENT);
                            }
                            if resp.clicked() {
                                self.tab = t;
                            }
                        }
                    });
                });
        }

        // Messages ("Saved", errors) as a small colored banner above the nav bar.
        if self.status.is_some() {
            let pad = if narrow { 0.0 } else { bottom_pad };
            egui::TopBottomPanel::bottom("status")
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin {
                    left: 10,
                    right: 10,
                    top: 6,
                    bottom: (pad + 6.0) as i8,
                }))
                .show(ctx, |ui| {
                    let (icon, text, color) = match &self.status {
                        Some(Status::Info(s)) => ("✔", s.clone(), theme::ACCENT),
                        Some(Status::Error(s)) => ("⚠", s.clone(), theme::BAD),
                        None => ("", String::new(), theme::MUTED),
                    };
                    let mut close = false;
                    theme::tinted_card(color).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(icon).color(color));
                            let w = (ui.available_width() - 30.0).max(50.0);
                            ui.allocate_ui(egui::vec2(w, 0.0), |ui| {
                                ui.add(egui::Label::new(RichText::new(text).color(theme::TEXT)).wrap());
                            });
                            if ui.add(egui::Button::new(theme::muted("✖")).frame(false)).clicked() {
                                close = true;
                            }
                        });
                    });
                    if close {
                        self.status = None;
                    }
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                Tab::Today => self.today_tab(ui),
                Tab::Overview => self.overview(ui),
                Tab::Expenses => self.expenses(ui),
                Tab::Calendar => self.calendar(ui),
                Tab::Food => self.food_tab(ui),
                Tab::Settings => self.settings(ui),
                Tab::ImportExport => self.import_export(ui),
            });
        });

        self.editor_window(ctx);
        self.recipe_window(ctx);
        self.recipe_editor_window(ctx);
        self.delete_dialog(ctx);
        self.picker_window(ctx);

        if self.dirty {
            self.dirty = false;
            if let Err(e) = storage::save(&self.budget) {
                self.error(format!("Couldn't save your data: {e}"));
            }
        }
        self.update_widget_file();
        if self.update_meals_file() {
            #[cfg(target_os = "android")]
            crate::android_perm::refresh_widget();
        }
        // A tap on a meal in the notification asks for its recipe.
        let now = ctx.input(|i| i.time);
        if now - self.open_check_at > 1.0 {
            self.open_check_at = now;
            self.check_open_recipe_request();
        }
        #[cfg(target_os = "android")]
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = storage::save(&self.budget);
    }
}
