//! Food tab: meal plan, grocery list, kitchen inventory, recipes, and the in-app recipe viewer.

use super::{stat_card, today, BudgetApp, PickFor, Tab};
use crate::csvio::money;
use crate::food::{self, fmt_qty, Recipe, Slot};
use crate::{storage, theme};
use chrono::{Duration, NaiveDate};
use eframe::egui::{self, RichText};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FoodSection {
    Plan,
    Groceries,
    Kitchen,
    Recipes,
}

/// The open recipe (shown in the app, never a browser).
pub(super) struct RecipeView {
    pub id: u64,
    pub servings: f64,
    pub date_text: String,
    pub slot: Slot,
    pub confirm_delete: bool,
}

pub(super) struct RecipeEditor {
    pub id: Option<u64>,
    pub name: String,
    pub servings: f64,
    pub ingredients: String,
    pub steps: String,
    pub notes: String,
    pub error: Option<String>,
}

impl RecipeEditor {
    fn new() -> Self {
        RecipeEditor {
            id: None,
            name: String::new(),
            servings: 4.0,
            ingredients: String::new(),
            steps: String::new(),
            notes: String::new(),
            error: None,
        }
    }
    fn from(r: &Recipe) -> Self {
        RecipeEditor {
            id: Some(r.id),
            name: r.name.clone(),
            servings: r.servings,
            ingredients: r.ingredients.iter().map(food::ingredient_line).collect::<Vec<_>>().join("\n"),
            steps: r.steps.join("\n"),
            notes: r.notes.clone(),
            error: None,
        }
    }
}

/// Inputs for "add a meal" and the kitchen/extra item forms.
pub(super) struct FoodForms {
    pub meal_date: String,
    pub meal_slot: Slot,
    pub meal_recipe: u64,
    pub item_name: String,
    pub item_qty: f64,
    pub item_unit: String,
    pub stock_name: String,
    pub stock_qty: f64,
    pub stock_unit: String,
}

impl Default for FoodForms {
    fn default() -> Self {
        FoodForms {
            meal_date: today().format("%Y-%m-%d").to_string(),
            meal_slot: Slot::Dinner,
            meal_recipe: 0,
            item_name: String::new(),
            item_qty: 1.0,
            item_unit: String::new(),
            stock_name: String::new(),
            stock_qty: 1.0,
            stock_unit: String::new(),
        }
    }
}

const OPEN_RECIPE_FILE: &str = "open_recipe.txt";
const MEALS_FILE: &str = "meals.json";

fn slot_combo(ui: &mut egui::Ui, id: &str, slot: &mut Slot) {
    egui::ComboBox::from_id_salt(id).selected_text(format!("{} {}", slot.icon(), slot.label())).show_ui(ui, |ui| {
        for s in Slot::ALL {
            ui.selectable_value(slot, s, format!("{} {}", s.icon(), s.label()));
        }
    });
}

impl BudgetApp {
    /// Something in Food changed: save, and keep the Groceries expense in step if that's on.
    pub(super) fn food_changed(&mut self) {
        food::sync_groceries_expense(&mut self.budget, today());
        self.dirty = true;
    }

    pub(super) fn add_sample_recipes(&mut self) -> usize {
        let Ok(list) = food::parse_recipes(food::SAMPLE_RECIPES) else { return 0 };
        let n = list.len();
        for r in list {
            self.budget.food.upsert_recipe(r);
        }
        self.budget.food.samples_added = true;
        n
    }

    pub(super) fn open_recipe(&mut self, id: u64) {
        let Some(r) = self.budget.food.recipe(id) else { return };
        let date = self.selected_day.filter(|d| *d >= today()).unwrap_or_else(today);
        self.recipe_view = Some(RecipeView {
            id,
            servings: r.servings,
            date_text: date.format("%Y-%m-%d").to_string(),
            slot: Slot::Dinner,
            confirm_delete: false,
        });
    }

    /// The meal notification asks for a recipe by writing its id to a file before opening the app.
    pub(super) fn check_open_recipe_request(&mut self) {
        let path = storage::data_dir().join(OPEN_RECIPE_FILE);
        if let Ok(text) = std::fs::read_to_string(&path) {
            let _ = std::fs::remove_file(&path);
            if let Ok(id) = text.trim().parse::<u64>() {
                self.tab = Tab::Food;
                self.open_recipe(id);
            }
        }
    }

    /// Keep meals.json (read by the meal notification) in step with the plan.
    pub(super) fn update_meals_file(&mut self) -> bool {
        let json = food::meals_json(&self.budget.food, today());
        if json == self.meals_written {
            return false;
        }
        let dir = storage::data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("meals.json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(tmp, dir.join(MEALS_FILE));
        }
        self.meals_written = json;
        true
    }

    /// Import a recipes CSV or a meal plan CSV.
    pub(super) fn import_food_csv(&mut self, name: &str, text: &str) {
        if food::is_recipe_csv(text) {
            match food::parse_recipes(text) {
                Ok(list) => {
                    let n = list.len();
                    for r in list {
                        self.budget.food.upsert_recipe(r);
                    }
                    self.food_changed();
                    self.food_section = FoodSection::Recipes;
                    self.info(format!("Added or updated {n} recipes from {name}."));
                }
                Err(e) => self.error(format!("{name}: {e}")),
            }
        } else if food::is_plan_csv(text) {
            match food::parse_plan(text) {
                Ok(rows) => {
                    let (mut added, mut missing) = (0, Vec::new());
                    for (date, slot, recipe, servings) in rows {
                        match self.budget.food.recipe_by_name(&recipe).map(|r| (r.id, r.servings)) {
                            Some((id, default_servings)) => {
                                self.budget.food.schedule(date, slot, id, servings.unwrap_or(default_servings));
                                added += 1;
                            }
                            None if !missing.contains(&recipe) => missing.push(recipe),
                            None => {}
                        }
                    }
                    self.food_changed();
                    self.food_section = FoodSection::Plan;
                    if missing.is_empty() {
                        self.info(format!("Added {added} meals to your plan."));
                    } else {
                        self.error(format!("Added {added} meals. No recipe found for: {}. Import those recipes first.", missing.join(", ")));
                    }
                }
                Err(e) => self.error(format!("{name}: {e}")),
            }
        } else {
            self.error(format!(
                "{name} doesn't look like a recipes CSV ({}) or a meal plan CSV ({}).",
                food::RECIPE_HEADERS.join(","),
                food::PLAN_HEADERS.join(",")
            ));
        }
    }

    fn food_import_button(&mut self, ui: &mut egui::Ui) {
        #[cfg(not(target_os = "android"))]
        if ui.button("📥 Import recipes or meal plan (CSV)...").clicked() {
            if let Some(paths) = rfd::FileDialog::new().add_filter("CSV", &["csv", "txt"]).pick_files() {
                for path in paths {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    match std::fs::read_to_string(&path) {
                        Ok(text) => self.import_food_csv(&name, &text),
                        Err(e) => self.error(format!("Couldn't read {name}: {e}")),
                    }
                }
            }
        }
        #[cfg(target_os = "android")]
        if ui.button("📥 Import recipes or meal plan (CSV)").clicked() {
            self.picker = Some(super::Picker::open(PickFor::Food));
        }
        let _ = PickFor::Food;
    }

    // ---------------------------------------------------------------- Food tab
    pub(super) fn food_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (sec, label) in [
                (FoodSection::Plan, "📅 Plan"),
                (FoodSection::Groceries, "🏪 Groceries"),
                (FoodSection::Kitchen, "🗄 Kitchen"),
                (FoodSection::Recipes, "📖 Recipes"),
            ] {
                let on = self.food_section == sec;
                let text = if on { RichText::new(label).strong().color(theme::BG) } else { RichText::new(label) };
                let mut b = egui::Button::new(text).min_size(egui::vec2(0.0, 32.0)).corner_radius(egui::CornerRadius::same(16));
                if on {
                    b = b.fill(theme::ACCENT);
                }
                if ui.add(b).clicked() {
                    self.food_section = sec;
                }
            }
        });
        ui.add_space(6.0);
        match self.food_section {
            FoodSection::Plan => self.food_plan(ui),
            FoodSection::Groceries => self.food_groceries(ui),
            FoodSection::Kitchen => self.food_kitchen(ui),
            FoodSection::Recipes => self.food_recipes(ui),
        }
    }

    /// A row for one planned meal. Returns (open, toggle cooked, delete).
    fn meal_row(&self, ui: &mut egui::Ui, m: &food::PlannedMeal, show_date: bool) -> (bool, bool, bool) {
        let f = &self.budget.food;
        let name = f.recipe(m.recipe_id).map(|r| r.name.clone()).unwrap_or_else(|| "(recipe deleted)".into());
        let cost = f.meal_cost(m);
        let (mut open, mut cook, mut del) = (false, false, false);
        let resp = theme::card()
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(m.slot.icon()).size(20.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(theme::muted("🗑")).on_hover_text("Remove from plan").clicked() {
                            del = true;
                        }
                        let tick = if m.cooked { RichText::new("✔").color(theme::GOOD) } else { theme::muted("✔") };
                        if ui.button(tick).on_hover_text("Cooked (uses ingredients from your kitchen)").clicked() {
                            cook = true;
                        }
                        ui.label(RichText::new(money(cost)).color(theme::AMBER));
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            let mut t = RichText::new(&name).color(theme::TEXT);
                            if m.cooked {
                                t = t.strikethrough().color(theme::MUTED);
                            }
                            ui.add(egui::Label::new(t).truncate());
                            let when = if show_date { format!("{} · ", m.date.format("%a %b %-d")) } else { String::new() };
                            let per = if m.servings > 0.0 { cost / m.servings } else { cost };
                            ui.add(
                                egui::Label::new(theme::muted(format!("{when}{} · {} servings · {} each", m.slot.label(), fmt_qty(m.servings), money(per))).size(12.0))
                                    .truncate(),
                            );
                        });
                    });
                });
            })
            .response
            .interact(egui::Sense::click());
        if resp.clicked() {
            open = true;
        }
        (open, cook, del)
    }

    fn apply_meal_actions(&mut self, open: Option<u64>, cook: Option<u64>, del: Option<u64>) {
        if let Some(id) = cook {
            let now = self.budget.food.plan.iter().find(|m| m.id == id).map(|m| m.cooked).unwrap_or(false);
            self.budget.food.set_cooked(id, !now);
            self.food_changed();
        }
        if let Some(id) = del {
            self.budget.food.plan.retain(|m| m.id != id);
            self.food_changed();
        }
        if let Some(rid) = open {
            self.open_recipe(rid);
        }
    }

    /// Meals for one day, used on the Dates tab under the selected day.
    pub(super) fn day_meals(&mut self, ui: &mut egui::Ui, day: NaiveDate) {
        let meals: Vec<food::PlannedMeal> = self.budget.food.meals_on(day).into_iter().cloned().collect();
        ui.add_space(4.0);
        ui.label(RichText::new("🍽  Meals").strong());
        let (mut open, mut cook, mut del) = (None, None, None);
        for m in &meals {
            let (o, c, d) = self.meal_row(ui, m, false);
            if o {
                open = Some(m.recipe_id);
            }
            if c {
                cook = Some(m.id);
            }
            if d {
                del = Some(m.id);
            }
        }
        if meals.is_empty() {
            ui.label(theme::muted("No meals planned."));
        }
        self.apply_meal_actions(open, cook, del);
        self.add_meal_form(ui, Some(day));
    }

    fn add_meal_form(&mut self, ui: &mut egui::Ui, day: Option<NaiveDate>) {
        if self.budget.food.recipes.is_empty() {
            ui.label(theme::muted("Add recipes in Food > Recipes to plan meals."));
            return;
        }
        if self.budget.food.recipe(self.food_forms.meal_recipe).is_none() {
            self.food_forms.meal_recipe = self.budget.food.recipes[0].id;
        }
        let mut add = false;
        ui.horizontal(|ui| {
            ui.label("➕");
            if day.is_none() {
                ui.add(egui::TextEdit::singleline(&mut self.food_forms.meal_date).desired_width(96.0).hint_text("2026-11-05"));
            }
            slot_combo(ui, &format!("add_slot_{day:?}"), &mut self.food_forms.meal_slot);
        });
        ui.horizontal(|ui| {
            let current = self.budget.food.recipe(self.food_forms.meal_recipe).map(|r| r.name.clone()).unwrap_or_default();
            let w = (ui.available_width() - 110.0).max(120.0);
            egui::ComboBox::from_id_salt(format!("add_recipe_{day:?}")).selected_text(current).truncate().width(w).show_ui(ui, |ui| {
                let mut list: Vec<(u64, String, bool)> = self.budget.food.recipes.iter().map(|r| (r.id, r.name.clone(), r.favorite)).collect();
                list.sort_by_key(|x| (!x.2, x.1.to_lowercase()));
                for (id, name, fav) in list {
                    let label = if fav { format!("⭐ {name}") } else { name };
                    ui.selectable_value(&mut self.food_forms.meal_recipe, id, label);
                }
            });
            if theme::primary_button(ui, "Add meal").clicked() {
                add = true;
            }
        });
        if add {
            let date = day.or_else(|| crate::csvio::parse_date(&self.food_forms.meal_date));
            match (date, self.budget.food.recipe(self.food_forms.meal_recipe).map(|r| (r.id, r.servings, r.name.clone()))) {
                (Some(d), Some((id, s, name))) => {
                    self.budget.food.schedule(d, self.food_forms.meal_slot, id, s);
                    self.food_changed();
                    self.info(format!("Planned {name} for {}.", d.format("%a %b %-d")));
                }
                (None, _) => self.error("Date should look like 2026-11-05."),
                _ => {}
            }
        }
    }

    fn food_plan(&mut self, ui: &mut egui::Ui) {
        let t = today();
        let f = &self.budget.food;
        let week_end = t + Duration::days(6);
        let week: Vec<&food::PlannedMeal> = f.plan.iter().filter(|m| m.date >= t && m.date <= week_end).collect();
        let week_cost: f64 = week.iter().map(|m| f.meal_cost(m)).sum();
        let month_cost = f.month_meal_cost(t);
        let per_meal = if week.is_empty() { 0.0 } else { week_cost / week.len() as f64 };
        let n_week = week.len();
        ui.columns(2, |c| {
            stat_card(&mut c[0], "🍽", "Meals next 7 days", &format!("{n_week} · {}", money(week_cost)), theme::GOOD);
            stat_card(&mut c[1], "💲", "Average per meal", &money(per_meal), theme::AMBER);
        });
        ui.label(theme::muted(format!("Meals planned this month cost about {}.", money(month_cost))).size(12.5));
        ui.add_space(4.0);
        theme::section_title(ui, "➕", "Add a meal");
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            self.add_meal_form(ui, None);
        });
        theme::section_title(ui, "📅", "Coming up");
        let upcoming: Vec<food::PlannedMeal> =
            self.budget.food.plan.iter().filter(|m| m.date >= t && m.date <= t + Duration::days(13)).cloned().collect();
        if upcoming.is_empty() {
            ui.label(theme::muted(
                "Nothing planned yet. Add a meal above, open a recipe and tap Add to plan, or import a meal plan CSV (date, meal, recipe, servings).",
            ));
        }
        let (mut open, mut cook, mut del) = (None, None, None);
        let mut last: Option<NaiveDate> = None;
        for m in &upcoming {
            if last != Some(m.date) {
                last = Some(m.date);
                let label = if m.date == t { "Today".to_string() } else { m.date.format("%A, %b %-d").to_string() };
                ui.add_space(4.0);
                ui.label(RichText::new(label).strong().color(theme::MUTED));
            }
            let (o, c, d) = self.meal_row(ui, m, false);
            if o {
                open = Some(m.recipe_id);
            }
            if c {
                cook = Some(m.id);
            }
            if d {
                del = Some(m.id);
            }
        }
        self.apply_meal_actions(open, cook, del);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            self.food_import_button(ui);
            if ui.button("📤 Export meal plan").clicked() {
                let text = food::export_plan(&self.budget.food);
                self.save_csv(ui.ctx(), "Meal Plan.csv", text);
            }
        });
        let mut hide = self.budget.food.hide_meal_notification;
        if ui.checkbox(&mut hide, "Hide today's meals from my notification bar").changed() {
            self.budget.food.hide_meal_notification = hide;
            self.dirty = true;
        }
    }

    fn food_groceries(&mut self, ui: &mut egui::Ui) {
        let t = today();
        let days = self.budget.food.list_days.max(1);
        let end = t + Duration::days(days as i64 - 1);
        let list = self.budget.food.grocery_list(t, end);
        let list_cost: f64 = list.iter().map(|l| l.cost).sum();
        let extras_cost: f64 = self.budget.food.extras.iter().map(|e| e.cost).sum();

        // Cost per meal for the meals this list covers.
        let meals: Vec<food::PlannedMeal> = self.budget.food.plan.iter().filter(|m| !m.cooked && m.date >= t && m.date <= end).cloned().collect();
        let meals_cost: f64 = meals.iter().map(|m| self.budget.food.meal_cost(m)).sum();
        let servings: f64 = meals.iter().map(|m| m.servings).sum();
        ui.columns(2, |c| {
            stat_card(&mut c[0], "🏪", "Still to buy", &money(list_cost + extras_cost), theme::AMBER);
            let per = if meals.is_empty() { 0.0 } else { meals_cost / meals.len() as f64 };
            stat_card(&mut c[1], "🍽", &format!("{} meals", meals.len()), &format!("{} each", money(per)), theme::GOOD);
        });
        if servings > 0.0 {
            ui.label(theme::muted(format!(
                "The meals ahead cost {} in all: {} servings at {} a serving. Compare that to eating out!",
                money(meals_cost),
                fmt_qty(servings),
                money(meals_cost / servings)
            )).size(12.5));
        }
        ui.horizontal(|ui| {
            ui.label("List covers the next");
            if ui.add(egui::DragValue::new(&mut self.budget.food.list_days).range(1..=60).suffix(" days")).changed() {
                self.dirty = true;
            }
        });

        // Cost per meal breakdown.
        if !meals.is_empty() {
            egui::CollapsingHeader::new(RichText::new("💲 Cost per meal").strong()).default_open(false).show(ui, |ui| {
                egui::Grid::new("cost_per_meal").striped(true).num_columns(4).show(ui, |ui| {
                    ui.label(theme::muted("Meal"));
                    ui.label(theme::muted("Servings"));
                    ui.label(theme::muted("Meal"));
                    ui.label(theme::muted("Serving"));
                    ui.end_row();
                    for m in &meals {
                        let name = self.budget.food.recipe(m.recipe_id).map(|r| r.name.clone()).unwrap_or_default();
                        let c = self.budget.food.meal_cost(m);
                        ui.add(egui::Label::new(format!("{} {}", m.date.format("%a"), name)).truncate());
                        ui.label(fmt_qty(m.servings));
                        ui.label(money(c));
                        ui.label(money(if m.servings > 0.0 { c / m.servings } else { c }));
                        ui.end_row();
                    }
                });
            });
        }

        theme::section_title(ui, "🏪", "Shopping list");
        if list.is_empty() && self.budget.food.extras.is_empty() {
            ui.label(theme::muted("Nothing to buy. Plan some meals and their ingredients show up here, minus what's already in your kitchen."));
        }
        let mut got: Option<food::GroceryLine> = None;
        for l in &list {
            theme::card().inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut c = false;
                    if ui.checkbox(&mut c, "").on_hover_text("Got it: moves it to your kitchen").changed() {
                        got = Some(l.clone());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(money(l.cost)).color(theme::AMBER));
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            let unit = if l.unit.is_empty() { String::new() } else { format!(" {}", l.unit) };
                            ui.add(egui::Label::new(RichText::new(format!("{}{} {}", fmt_qty(l.qty), unit, l.name)).color(theme::TEXT)).truncate());
                            ui.add(egui::Label::new(theme::muted(format!("for {}", l.for_meals.join(", "))).size(12.0)).truncate());
                        });
                    });
                });
            });
        }
        if let Some(l) = got {
            self.budget.food.stock(&l.name, l.qty, &l.unit);
            self.food_changed();
        }
        // Extra items.
        let mut done: Option<usize> = None;
        let mut remove: Option<usize> = None;
        for (i, e) in self.budget.food.extras.iter().enumerate() {
            theme::card().inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut c = false;
                    if ui.checkbox(&mut c, "").on_hover_text("Got it: moves it to your kitchen").changed() {
                        done = Some(i);
                    }
                    let unit = if e.unit.is_empty() { String::new() } else { format!(" {}", e.unit) };
                    ui.label(format!("{}{} {}", fmt_qty(e.qty), unit, e.name));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(theme::muted("🗑")).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
        }
        if let Some(i) = done {
            let e = self.budget.food.extras.remove(i);
            self.budget.food.stock(&e.name, e.qty, &e.unit);
            self.food_changed();
        }
        if let Some(i) = remove {
            self.budget.food.extras.remove(i);
            self.food_changed();
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::DragValue::new(&mut self.food_forms.item_qty).range(0.0..=1000.0).speed(0.25));
            ui.add(egui::TextEdit::singleline(&mut self.food_forms.item_unit).hint_text("unit").desired_width(50.0));
            ui.add(egui::TextEdit::singleline(&mut self.food_forms.item_name).hint_text("Add something else (milk, coffee…)").desired_width(170.0));
            if ui.button("Add").clicked() && !self.food_forms.item_name.trim().is_empty() {
                self.budget.food.extras.push(food::Item {
                    name: self.food_forms.item_name.trim().to_string(),
                    qty: self.food_forms.item_qty,
                    unit: self.food_forms.item_unit.trim().to_string(),
                    cost: 0.0,
                });
                self.food_forms.item_name.clear();
                self.food_forms.item_unit.clear();
                self.food_forms.item_qty = 1.0;
                self.food_changed();
            }
        });
        if (!list.is_empty() || !self.budget.food.extras.is_empty()) && ui.button("📤 Export shopping list").clicked() {
            let text = food::export_grocery_list(&list, &self.budget.food.extras);
            self.save_csv(ui.ctx(), "Grocery List.csv", text);
        }

        // Groceries expense.
        ui.add_space(8.0);
        theme::section_title(ui, "💳", "Groceries in my budget");
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut sync = self.budget.food.sync_groceries;
            if ui.checkbox(&mut sync, "Keep my Groceries expense up to date from the meal plan").changed() {
                self.budget.food.sync_groceries = sync;
                self.food_changed();
            }
            if sync {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Plus other groceries each month (breakfast, snacks, coffee):");
                    if ui.add(egui::DragValue::new(&mut self.budget.food.staples).prefix("$").speed(5.0).range(0.0..=5000.0).fixed_decimals(0)).changed() {
                        self.food_changed();
                    }
                });
                let month = self.budget.food.month_meal_cost(t);
                let staples = self.budget.food.staples;
                ui.label(theme::muted(format!(
                    "{} this month = {} of planned meals + {} other. Changes as you plan meals.",
                    money(month + staples),
                    money(month),
                    money(staples)
                )));
            } else {
                ui.label(theme::muted("When on, the Groceries line on the Bills and Month tabs follows what you plan to cook."));
            }
        });
    }

    fn food_kitchen(&mut self, ui: &mut egui::Ui) {
        theme::section_title(ui, "🗄", "In my kitchen");
        ui.label(theme::muted("Ticking items off the shopping list adds them here. Marking a meal cooked uses them up.").size(12.5));
        let mut remove: Option<usize> = None;
        let mut changed = false;
        for (i, item) in self.budget.food.inventory.iter_mut().enumerate() {
            theme::card().inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut item.qty).range(0.0..=10_000.0).speed(0.25).max_decimals(2)).changed();
                    if !item.unit.is_empty() {
                        ui.label(theme::muted(&item.unit));
                    }
                    ui.add(egui::Label::new(RichText::new(&item.name).color(theme::TEXT)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(theme::muted("🗑")).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
        }
        if self.budget.food.inventory.is_empty() {
            ui.label(theme::muted("Empty for now."));
        }
        if let Some(i) = remove {
            self.budget.food.inventory.remove(i);
            changed = true;
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::DragValue::new(&mut self.food_forms.stock_qty).range(0.0..=1000.0).speed(0.25));
            ui.add(egui::TextEdit::singleline(&mut self.food_forms.stock_unit).hint_text("unit").desired_width(50.0));
            ui.add(egui::TextEdit::singleline(&mut self.food_forms.stock_name).hint_text("Item (rice, soy sauce…)").desired_width(170.0));
            if ui.button("Add").clicked() && !self.food_forms.stock_name.trim().is_empty() {
                let (n, q, u) = (self.food_forms.stock_name.clone(), self.food_forms.stock_qty, self.food_forms.stock_unit.clone());
                self.budget.food.stock(&n, q, &u);
                self.food_forms.stock_name.clear();
                self.food_forms.stock_unit.clear();
                self.food_forms.stock_qty = 1.0;
                changed = true;
            }
        });
        ui.label(theme::muted("Use the same name and unit as your recipes (like \"soy sauce\", \"cup\") so the shopping list can subtract it.").size(12.0));
        if changed {
            self.budget.food.inventory.retain(|i| i.qty > 1e-6);
            self.food_changed();
        }
    }

    fn food_recipes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if theme::primary_button(ui, "➕ New recipe").clicked() {
                self.recipe_edit = Some(RecipeEditor::new());
            }
            self.food_import_button(ui);
            if !self.budget.food.recipes.is_empty() && ui.button("📤 Export recipes").clicked() {
                let text = food::export_recipes(&self.budget.food.recipes);
                self.save_csv(ui.ctx(), "Recipes.csv", text);
            }
        });
        ui.label(theme::muted(format!(
            "Recipe CSV columns: {}. Type is ingredient, step or note.",
            food::RECIPE_HEADERS.join(", ")
        )).size(12.0));
        ui.add_space(4.0);
        let mut list: Vec<(u64, String, bool, f64, f64, f64, usize)> = self
            .budget
            .food
            .recipes
            .iter()
            .map(|r| (r.id, r.name.clone(), r.favorite, r.servings, r.cost(), r.cost_per_serving(), r.ingredients.len()))
            .collect();
        list.sort_by_key(|x| (!x.2, x.1.to_lowercase()));
        let mut open = None;
        let mut fav = None;
        for (id, name, is_fav, servings, cost, per, n) in &list {
            theme::card().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let star = if *is_fav { RichText::new("⭐").color(theme::AMBER) } else { theme::muted("☆") };
                    if ui.add(egui::Button::new(star).frame(false)).on_hover_text("A meal I like").clicked() {
                        fav = Some(*id);
                    }
                    let resp = ui
                        .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{}\n{}/serving", money(*cost), money(*per))).color(theme::AMBER).size(12.5));
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.add(egui::Label::new(RichText::new(name).strong().color(theme::TEXT)).truncate());
                                ui.label(theme::muted(format!("{} servings · {n} ingredients", fmt_qty(*servings))).size(12.0));
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.clicked() {
                        open = Some(*id);
                    }
                });
            });
        }
        if list.is_empty() {
            ui.label(theme::muted("No recipes yet."));
        }
        if !self.budget.food.samples_added || list.is_empty() {
            if ui.button("Add the sample recipes").clicked() {
                let n = self.add_sample_recipes();
                self.food_changed();
                self.info(format!("Added {n} sample recipes."));
            }
        }
        if let Some(id) = fav {
            if let Some(r) = self.budget.food.recipes.iter_mut().find(|r| r.id == id) {
                r.favorite = !r.favorite;
            }
            self.dirty = true;
        }
        if let Some(id) = open {
            self.open_recipe(id);
        }
    }

    // ---------------------------------------------------------------- Recipe viewer
    pub(super) fn recipe_window(&mut self, ctx: &egui::Context) {
        let Some(mut view) = self.recipe_view.take() else { return };
        let Some(r) = self.budget.food.recipe(view.id).cloned() else { return };
        let mut open = true;
        let mut edit = false;
        let mut delete = false;
        let mut schedule: Option<(NaiveDate, Slot, f64)> = None;
        let mut fav = false;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 16.0).min(560.0);
        let top = self.budget.settings.safe_top + 8.0;
        let height = (screen.height() - top - self.budget.settings.safe_bottom - 16.0).max(300.0);
        egui::Window::new(RichText::new(&r.name).strong())
            .id(egui::Id::new("recipe_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(width, height))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, top))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let k = r.scale(view.servings);
                    ui.horizontal_wrapped(|ui| {
                        let star = if r.favorite { RichText::new("⭐ Favorite").color(theme::AMBER) } else { theme::muted("☆ Favorite") };
                        if ui.button(star).clicked() {
                            fav = true;
                        }
                        if ui.button("✏ Edit").clicked() {
                            edit = true;
                        }
                        let del_text = if view.confirm_delete { RichText::new("Tap again to delete").color(theme::BAD) } else { theme::muted("🗑 Delete") };
                        if ui.button(del_text).clicked() {
                            if view.confirm_delete {
                                delete = true;
                            } else {
                                view.confirm_delete = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Servings");
                        ui.add(egui::DragValue::new(&mut view.servings).range(0.5..=50.0).speed(0.25).max_decimals(2));
                        ui.label(RichText::new(format!("{} · {}/serving", money(r.cost() * k), money(r.cost_per_serving()))).color(theme::AMBER));
                    });
                    ui.separator();
                    ui.label(RichText::new("🍅  Ingredients").size(17.0).strong());
                    for i in &r.ingredients {
                        let unit = if i.unit.is_empty() { String::new() } else { format!(" {}", i.unit) };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("•").color(theme::ACCENT));
                            ui.label(RichText::new(format!("{}{}", fmt_qty(i.qty * k), unit)).strong());
                            ui.label(&i.name);
                            if i.cost > 0.0 {
                                ui.label(theme::muted(money(i.cost * k)).size(12.0));
                            }
                        });
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("🔪  Steps").size(17.0).strong());
                    for (n, s) in r.steps.iter().enumerate() {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(format!("{}.", n + 1)).strong().color(theme::ACCENT));
                            ui.add(egui::Label::new(s).wrap());
                        });
                        ui.add_space(3.0);
                    }
                    if !r.notes.is_empty() {
                        ui.add_space(4.0);
                        theme::tinted_card(theme::SKY).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new(format!("📈 {}", r.notes)).color(theme::TEXT));
                        });
                    }
                    ui.separator();
                    ui.label(RichText::new("📅  Add to my meal plan").size(17.0).strong());
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut view.date_text).desired_width(100.0));
                        slot_combo(ui, "view_slot", &mut view.slot);
                        if theme::primary_button(ui, "Add to plan").clicked() {
                            if let Some(d) = crate::csvio::parse_date(&view.date_text) {
                                schedule = Some((d, view.slot, view.servings));
                            }
                        }
                    });
                    let planned: Vec<String> = self
                        .budget
                        .food
                        .plan
                        .iter()
                        .filter(|m| m.recipe_id == r.id && m.date >= today())
                        .map(|m| format!("{} {}", m.date.format("%a %b %-d"), m.slot.label().to_lowercase()))
                        .collect();
                    if !planned.is_empty() {
                        ui.label(theme::muted(format!("Planned: {}", planned.join(", "))));
                    }
                });
            });
        if fav {
            if let Some(x) = self.budget.food.recipes.iter_mut().find(|x| x.id == r.id) {
                x.favorite = !x.favorite;
            }
            self.dirty = true;
        }
        if let Some((d, slot, servings)) = schedule {
            self.budget.food.schedule(d, slot, r.id, servings);
            self.food_changed();
            self.info(format!("Planned {} for {} {}.", r.name, d.format("%a %b %-d"), slot.label().to_lowercase()));
        }
        if delete {
            self.budget.food.delete_recipe(r.id);
            self.food_changed();
            self.info(format!("Deleted {}.", r.name));
            return;
        }
        if edit {
            self.recipe_edit = Some(RecipeEditor::from(&r));
            return;
        }
        if open {
            self.recipe_view = Some(view);
        }
    }

    pub(super) fn recipe_editor_window(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.recipe_edit.take() else { return };
        let mut open = true;
        let mut save = false;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 16.0).min(560.0);
        let top = self.budget.settings.safe_top + 8.0;
        egui::Window::new(if ed.id.is_some() { "Edit recipe" } else { "New recipe" })
            .id(egui::Id::new("recipe_editor"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, top))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(screen.height() - top - 120.0).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(f32::INFINITY));
                    ui.horizontal(|ui| {
                        ui.label("Makes");
                        ui.add(egui::DragValue::new(&mut ed.servings).range(0.5..=50.0).speed(0.25));
                        ui.label("servings");
                    });
                    ui.label("Ingredients, one per line: amount, unit, name, cost");
                    ui.label(theme::muted("e.g.  1.5 lb chicken thighs $5.25").size(12.0));
                    ui.add(egui::TextEdit::multiline(&mut ed.ingredients).desired_rows(8).desired_width(f32::INFINITY));
                    ui.label("Steps, one per line");
                    ui.add(egui::TextEdit::multiline(&mut ed.steps).desired_rows(6).desired_width(f32::INFINITY));
                    ui.label("Notes");
                    ui.add(egui::TextEdit::multiline(&mut ed.notes).desired_rows(2).desired_width(f32::INFINITY));
                    if let Some(e) = &ed.error {
                        ui.colored_label(theme::BAD, e);
                    }
                    if theme::primary_button(ui, "Save recipe").clicked() {
                        save = true;
                    }
                });
            });
        if save {
            let name = ed.name.trim().to_string();
            if name.is_empty() {
                ed.error = Some("Give the recipe a name.".into());
            } else {
                let r = Recipe {
                    id: ed.id.unwrap_or(0),
                    name,
                    servings: ed.servings.max(0.5),
                    ingredients: ed.ingredients.lines().filter_map(food::parse_ingredient_line).collect(),
                    steps: ed.steps.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                    notes: ed.notes.trim().to_string(),
                    favorite: false,
                };
                let id = self.budget.food.upsert_recipe(r);
                self.food_changed();
                self.open_recipe(id);
                return;
            }
        }
        if open {
            self.recipe_edit = Some(ed);
        }
    }
}

