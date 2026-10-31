//! Food: recipes, a meal plan, a grocery list built from the plan, and a kitchen inventory.
//!
//! Recipes and meal plans can be imported and exported as CSV:
//! - Recipes: `recipe,servings,type,item,quantity,unit,cost` where type is `ingredient`, `step` or `note`.
//!   For steps and notes, `item` holds the text.
//! - Meal plan: `date,meal,recipe,servings`.

use crate::model::{month_end, month_start, Budget, Expense, Frequency};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RECIPE_HEADERS: [&str; 7] = ["recipe", "servings", "type", "item", "quantity", "unit", "cost"];
pub const PLAN_HEADERS: [&str; 4] = ["date", "meal", "recipe", "servings"];
pub const GROCERIES_EXPENSE: &str = "Groceries";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Food {
    #[serde(default)]
    pub recipes: Vec<Recipe>,
    #[serde(default)]
    pub plan: Vec<PlannedMeal>,
    #[serde(default)]
    pub inventory: Vec<Item>,
    /// Things to buy that aren't from a recipe.
    #[serde(default)]
    pub extras: Vec<Item>,
    #[serde(default)]
    pub next_id: u64,
    /// Keep the "Groceries" expense equal to this month's meal plan cost plus `staples`.
    #[serde(default)]
    pub sync_groceries: bool,
    /// Monthly groceries not tied to recipes (snacks, breakfast, coffee…).
    #[serde(default)]
    pub staples: f64,
    /// How many days ahead the grocery list covers.
    #[serde(default = "default_list_days")]
    pub list_days: u32,
    #[serde(default)]
    pub hide_meal_notification: bool,
    #[serde(default)]
    pub samples_added: bool,
}

/// Two weeks: one grocery run every other week.
fn default_list_days() -> u32 {
    14
}

impl Default for Food {
    fn default() -> Self {
        Food {
            recipes: vec![],
            plan: vec![],
            inventory: vec![],
            extras: vec![],
            next_id: 0,
            sync_groceries: false,
            staples: 0.0,
            list_days: default_list_days(),
            hide_meal_notification: false,
            samples_added: false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
    pub id: u64,
    pub name: String,
    pub servings: f64,
    pub ingredients: Vec<Ingredient>,
    pub steps: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Ingredient {
    pub name: String,
    pub qty: f64,
    pub unit: String,
    /// Rough cost of this amount.
    pub cost: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Breakfast,
    Lunch,
    Dinner,
    Snack,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::Breakfast, Slot::Lunch, Slot::Dinner, Slot::Snack];
    pub fn label(self) -> &'static str {
        match self {
            Slot::Breakfast => "Breakfast",
            Slot::Lunch => "Lunch",
            Slot::Dinner => "Dinner",
            Slot::Snack => "Snack",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Slot::Breakfast => "🍳",
            Slot::Lunch => "🍱",
            Slot::Dinner => "🍽",
            Slot::Snack => "🍎",
        }
    }
    pub fn parse(s: &str) -> Option<Slot> {
        let s = s.trim().to_ascii_lowercase();
        Slot::ALL.into_iter().find(|x| x.label().to_ascii_lowercase() == s || (s == "supper" && *x == Slot::Dinner))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlannedMeal {
    pub id: u64,
    pub date: NaiveDate,
    pub slot: Slot,
    pub recipe_id: u64,
    pub servings: f64,
    #[serde(default)]
    pub cooked: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub name: String,
    pub qty: f64,
    pub unit: String,
    #[serde(default)]
    pub cost: f64,
}

/// One line of the grocery list.
#[derive(Clone, Debug, PartialEq)]
pub struct GroceryLine {
    pub name: String,
    pub unit: String,
    /// Still to buy (after what's in the kitchen).
    pub qty: f64,
    pub cost: f64,
    /// Recipes that need it.
    pub for_meals: Vec<String>,
}

pub fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Units are compared loosely: "cups" = "cup", "lbs" = "lb".
pub fn norm_unit(s: &str) -> String {
    let u = norm(s);
    match u.as_str() {
        "cups" => "cup".into(),
        "lbs" | "pound" | "pounds" => "lb".into(),
        "ounce" | "ounces" => "oz".into(),
        "tablespoon" | "tablespoons" | "tbs" => "tbsp".into(),
        "teaspoon" | "teaspoons" => "tsp".into(),
        "cloves" => "clove".into(),
        "cans" => "can".into(),
        "heads" => "head".into(),
        "slices" => "slice".into(),
        "sticks" => "stick".into(),
        "loaves" => "loaf".into(),
        _ => u,
    }
}

const UNITS: [&str; 40] = [
    "tsp", "teaspoon", "teaspoons", "tbsp", "tablespoon", "tablespoons", "cup", "cups", "oz", "ounce", "ounces", "lb", "lbs",
    "pound", "pounds", "g", "kg", "ml", "l", "clove", "cloves", "can", "cans", "head", "heads", "loaf", "loaves", "pinch",
    "slice", "slices", "bunch", "pkg", "package", "each", "stick", "sticks", "jar", "bag", "box", "dash",
];

impl Recipe {
    pub fn cost(&self) -> f64 {
        self.ingredients.iter().map(|i| i.cost).sum()
    }
    pub fn cost_per_serving(&self) -> f64 {
        if self.servings > 0.0 { self.cost() / self.servings } else { self.cost() }
    }
    /// Cost of making `servings` servings.
    pub fn cost_for(&self, servings: f64) -> f64 {
        self.cost() * self.scale(servings)
    }
    pub fn scale(&self, servings: f64) -> f64 {
        if self.servings > 0.0 { servings / self.servings } else { 1.0 }
    }
}

/// "1 1/2" or "0.5" or "3/4" → number.
pub fn parse_qty(s: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    for part in s.split_whitespace() {
        if let Some((a, b)) = part.split_once('/') {
            let (a, b) = (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?);
            if b == 0.0 {
                return None;
            }
            total += a / b;
        } else {
            total += part.parse::<f64>().ok()?;
        }
        any = true;
    }
    any.then_some(total)
}

pub fn fmt_qty(q: f64) -> String {
    let whole = q.trunc();
    let frac = q - whole;
    let f = [(0.25, "¼"), (0.333, "⅓"), (0.5, "½"), (0.667, "⅔"), (0.75, "¾")]
        .iter()
        .find(|(v, _)| (frac - v).abs() < 0.02)
        .map(|(_, s)| *s);
    match (whole as i64, f) {
        (0, Some(s)) => s.to_string(),
        (w, Some(s)) => format!("{w}{s}"),
        _ if (q - q.round()).abs() < 0.01 => format!("{}", q.round() as i64),
        _ => format!("{:.2}", q).trim_end_matches('0').trim_end_matches('.').to_string(),
    }
}

/// Parse an ingredient line typed in the app: "1.5 lb chicken thighs $6".
pub fn parse_ingredient_line(line: &str) -> Option<Ingredient> {
    let line = line.trim().trim_start_matches(['-', '•', '*']).trim();
    if line.is_empty() {
        return None;
    }
    let mut words: Vec<&str> = line.split_whitespace().collect();
    let mut cost = 0.0;
    if let Some(last) = words.last() {
        if let Some(c) = last.strip_prefix('$').and_then(|c| c.replace(',', "").parse::<f64>().ok()) {
            cost = c;
            words.pop();
        }
    }
    // Leading quantity: up to two number-ish words ("1 1/2").
    let mut n = 0;
    while n < words.len().min(2) && parse_qty(words[n]).is_some() {
        n += 1;
    }
    let qty = if n > 0 { parse_qty(&words[..n].join(" ")).unwrap_or(1.0) } else { 1.0 };
    let mut rest = &words[n..];
    let mut unit = String::new();
    if let Some(w) = rest.first() {
        if UNITS.contains(&w.to_ascii_lowercase().trim_end_matches('.')) && rest.len() > 1 {
            unit = w.to_ascii_lowercase().trim_end_matches('.').to_string();
            rest = &rest[1..];
        }
    }
    let name = rest.join(" ");
    if name.is_empty() {
        return None;
    }
    Some(Ingredient { name, qty, unit, cost })
}

pub fn ingredient_line(i: &Ingredient) -> String {
    let mut s = fmt_qty(i.qty);
    if !i.unit.is_empty() {
        s.push(' ');
        s.push_str(&i.unit);
    }
    s.push(' ');
    s.push_str(&i.name);
    if i.cost > 0.0 {
        s.push_str(&format!(" ${:.2}", i.cost));
    }
    s
}

impl Food {
    fn new_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(self.recipes.iter().map(|r| r.id).chain(self.plan.iter().map(|p| p.id)).max().unwrap_or(0)) + 1;
        self.next_id
    }

    pub fn recipe(&self, id: u64) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }

    pub fn recipe_by_name(&self, name: &str) -> Option<&Recipe> {
        let n = norm(name);
        self.recipes.iter().find(|r| norm(&r.name) == n)
    }

    /// Add or replace (by name) a recipe. Returns its id.
    pub fn upsert_recipe(&mut self, mut r: Recipe) -> u64 {
        if let Some(existing) = self.recipes.iter_mut().find(|x| (r.id != 0 && x.id == r.id) || norm(&x.name) == norm(&r.name)) {
            r.id = existing.id;
            r.favorite = r.favorite || existing.favorite;
            *existing = r;
            return existing.id;
        }
        r.id = self.new_id();
        let id = r.id;
        self.recipes.push(r);
        id
    }

    pub fn delete_recipe(&mut self, id: u64) {
        self.recipes.retain(|r| r.id != id);
        self.plan.retain(|m| m.recipe_id != id);
    }

    pub fn schedule(&mut self, date: NaiveDate, slot: Slot, recipe_id: u64, servings: f64) -> u64 {
        let id = self.new_id();
        self.plan.push(PlannedMeal { id, date, slot, recipe_id, servings, cooked: false });
        self.plan.sort_by_key(|m| (m.date, m.slot));
        id
    }

    pub fn meals_on(&self, date: NaiveDate) -> Vec<&PlannedMeal> {
        self.plan.iter().filter(|m| m.date == date).collect()
    }

    pub fn meal_cost(&self, m: &PlannedMeal) -> f64 {
        self.recipe(m.recipe_id).map(|r| r.cost_for(m.servings)).unwrap_or(0.0)
    }

    /// Estimated cost of every meal planned in d's month.
    pub fn month_meal_cost(&self, d: NaiveDate) -> f64 {
        let start = month_start(d.year(), d.month());
        let end = month_end(start);
        self.plan.iter().filter(|m| m.date >= start && m.date <= end).map(|m| self.meal_cost(m)).sum()
    }

    fn have(&self, name: &str, unit: &str) -> f64 {
        let (n, u) = (norm(name), norm_unit(unit));
        self.inventory.iter().filter(|i| norm(&i.name) == n && norm_unit(&i.unit) == u).map(|i| i.qty).sum()
    }

    /// What to buy for uncooked meals from `from` through `to`, minus what's in the kitchen.
    pub fn grocery_list(&self, from: NaiveDate, to: NaiveDate) -> Vec<GroceryLine> {
        let mut need: BTreeMap<(String, String), GroceryLine> = BTreeMap::new();
        for m in self.plan.iter().filter(|m| !m.cooked && m.date >= from && m.date <= to) {
            let Some(r) = self.recipe(m.recipe_id) else { continue };
            let k = r.scale(m.servings);
            for i in &r.ingredients {
                let key = (norm(&i.name), norm_unit(&i.unit));
                let line = need.entry(key).or_insert_with(|| GroceryLine {
                    name: i.name.clone(),
                    unit: norm_unit(&i.unit),
                    qty: 0.0,
                    cost: 0.0,
                    for_meals: vec![],
                });
                line.qty += i.qty * k;
                line.cost += i.cost * k;
                if !line.for_meals.contains(&r.name) {
                    line.for_meals.push(r.name.clone());
                }
            }
        }
        need.into_values()
            .filter_map(|mut l| {
                let have = self.have(&l.name, &l.unit);
                if have >= l.qty - 1e-9 {
                    return None;
                }
                let still = l.qty - have;
                l.cost *= still / l.qty;
                l.qty = still;
                Some(l)
            })
            .collect()
    }

    /// Add to the kitchen inventory, merging with a matching item.
    pub fn stock(&mut self, name: &str, qty: f64, unit: &str) {
        let (n, u) = (norm(name), norm_unit(unit));
        if let Some(i) = self.inventory.iter_mut().find(|i| norm(&i.name) == n && norm_unit(&i.unit) == u) {
            i.qty += qty;
        } else {
            self.inventory.push(Item { name: name.trim().to_string(), qty, unit: u, cost: 0.0 });
            self.inventory.sort_by_key(|i| norm(&i.name));
        }
    }

    /// Mark a meal cooked (or not) and take its ingredients out of (or back into) the inventory.
    pub fn set_cooked(&mut self, meal_id: u64, cooked: bool) {
        let Some(m) = self.plan.iter().find(|m| m.id == meal_id).cloned() else { return };
        if m.cooked == cooked {
            return;
        }
        if let Some(r) = self.recipe(m.recipe_id).cloned() {
            let k = r.scale(m.servings);
            for i in &r.ingredients {
                let (n, u) = (norm(&i.name), norm_unit(&i.unit));
                if let Some(inv) = self.inventory.iter_mut().find(|x| norm(&x.name) == n && norm_unit(&x.unit) == u) {
                    inv.qty = if cooked { (inv.qty - i.qty * k).max(0.0) } else { inv.qty + i.qty * k };
                }
            }
            self.inventory.retain(|x| x.qty > 1e-6);
        }
        if let Some(p) = self.plan.iter_mut().find(|p| p.id == meal_id) {
            p.cooked = cooked;
        }
    }
}

/// Set the "Groceries" expense to this month's meal plan cost plus staples (creates it if needed).
pub fn sync_groceries_expense(b: &mut Budget, d: NaiveDate) -> Option<f64> {
    if !b.food.sync_groceries {
        return None;
    }
    let amount = ((b.food.month_meal_cost(d) + b.food.staples) * 100.0).round() / 100.0;
    if let Some(e) = b.expenses.iter_mut().find(|e| norm(&e.name) == norm(GROCERIES_EXPENSE)) {
        if (e.amount - amount).abs() > 0.004 {
            e.amount = amount;
        }
        e.frequency = Frequency::Monthly;
    } else {
        b.ensure_category("Food");
        b.add_expense(Expense {
            id: 0,
            name: GROCERIES_EXPENSE.into(),
            category: "Food".into(),
            amount,
            frequency: Frequency::Monthly,
            start: month_start(d.year(), d.month()),
            remaining_payments: None,
            remaining_balance: None,
            is_remainder: false,
            notes: "Kept up to date from the meal plan (Food tab).".into(),
        });
    }
    Some(amount)
}

// ---------------------------------------------------------------- CSV

fn header_index(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|h| norm(h).trim_start_matches('\u{feff}') == name)
}

pub fn is_recipe_csv(text: &str) -> bool {
    let first = text.lines().next().unwrap_or("").to_lowercase();
    first.contains("recipe") && first.contains("type") && first.contains("item")
}

pub fn is_plan_csv(text: &str) -> bool {
    let first = text.lines().next().unwrap_or("").to_lowercase();
    first.contains("date") && first.contains("recipe") && !first.contains("item")
}

/// Read recipes from CSV. Recipes are listed in file order.
pub fn parse_recipes(text: &str) -> Result<Vec<Recipe>, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(text.as_bytes());
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let idx = |n: &str| header_index(&headers, n);
    let (Some(rc), Some(tc), Some(ic)) = (idx("recipe"), idx("type"), idx("item")) else {
        return Err("Recipe CSV needs recipe, type and item columns.".into());
    };
    let (sc, qc, uc, cc) = (idx("servings"), idx("quantity"), idx("unit"), idx("cost"));
    let mut out: Vec<Recipe> = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        let get = |c: Option<usize>| c.and_then(|c| rec.get(c)).unwrap_or("").trim().to_string();
        let name = get(Some(rc));
        if name.is_empty() {
            continue;
        }
        if !out.last().is_some_and(|r| norm(&r.name) == norm(&name)) {
            if let Some(pos) = out.iter().position(|r| norm(&r.name) == norm(&name)) {
                let r = out.remove(pos);
                out.push(r);
            } else {
                out.push(Recipe { name: name.clone(), servings: 0.0, ..Default::default() });
            }
        }
        let r = out.last_mut().unwrap();
        if let Some(s) = parse_qty(&get(sc)) {
            if s > 0.0 {
                r.servings = s;
            }
        }
        let item = get(Some(ic));
        match norm(&get(Some(tc))).as_str() {
            "ingredient" | "ing" | "" if !item.is_empty() => r.ingredients.push(Ingredient {
                name: item,
                qty: parse_qty(&get(qc)).unwrap_or(1.0),
                unit: get(uc),
                cost: crate::csvio::parse_money(&get(cc)).unwrap_or(0.0),
            }),
            "step" | "instruction" | "direction" if !item.is_empty() => r.steps.push(item),
            "note" | "notes" if !item.is_empty() => {
                if !r.notes.is_empty() {
                    r.notes.push('\n');
                }
                r.notes.push_str(&item);
            }
            _ => {}
        }
    }
    for r in &mut out {
        if r.servings <= 0.0 {
            r.servings = 1.0;
        }
    }
    if out.is_empty() {
        return Err("No recipes found.".into());
    }
    Ok(out)
}

pub fn export_recipes(recipes: &[Recipe]) -> String {
    let mut w = csv::Writer::from_writer(vec![]);
    let _ = w.write_record(RECIPE_HEADERS);
    for r in recipes {
        let s = fmt_qty(r.servings);
        for i in &r.ingredients {
            let cost = if i.cost > 0.0 { format!("{:.2}", i.cost) } else { String::new() };
            let _ = w.write_record([r.name.as_str(), &s, "ingredient", &i.name, &fmt_num(i.qty), &i.unit, &cost]);
        }
        for st in &r.steps {
            let _ = w.write_record([r.name.as_str(), &s, "step", st, "", "", ""]);
        }
        for n in r.notes.lines().filter(|l| !l.trim().is_empty()) {
            let _ = w.write_record([r.name.as_str(), &s, "note", n, "", "", ""]);
        }
    }
    String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
}

fn fmt_num(q: f64) -> String {
    format!("{:.3}", q).trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Read a meal plan CSV. Returns (date, slot, recipe name, servings) rows.
pub fn parse_plan(text: &str) -> Result<Vec<(NaiveDate, Slot, String, Option<f64>)>, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(text.as_bytes());
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let idx = |n: &str| header_index(&headers, n);
    let (Some(dc), Some(rc)) = (idx("date"), idx("recipe")) else {
        return Err("Meal plan CSV needs date and recipe columns.".into());
    };
    let (mc, sc) = (idx("meal"), idx("servings"));
    let mut out = vec![];
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        let get = |c: Option<usize>| c.and_then(|c| rec.get(c)).unwrap_or("").trim().to_string();
        let Some(date) = crate::csvio::parse_date(&get(Some(dc))) else { continue };
        let recipe = get(Some(rc));
        if recipe.is_empty() {
            continue;
        }
        let slot = Slot::parse(&get(mc)).unwrap_or(Slot::Dinner);
        out.push((date, slot, recipe, parse_qty(&get(sc)).filter(|s| *s > 0.0)));
    }
    Ok(out)
}

pub fn export_plan(f: &Food) -> String {
    let mut w = csv::Writer::from_writer(vec![]);
    let _ = w.write_record(PLAN_HEADERS);
    for m in &f.plan {
        let name = f.recipe(m.recipe_id).map(|r| r.name.as_str()).unwrap_or("");
        let _ = w.write_record([m.date.format("%Y-%m-%d").to_string().as_str(), m.slot.label(), name, &fmt_num(m.servings)]);
    }
    String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
}

pub fn export_grocery_list(lines: &[GroceryLine], extras: &[Item]) -> String {
    let mut w = csv::Writer::from_writer(vec![]);
    let _ = w.write_record(["item", "quantity", "unit", "estimated cost", "for"]);
    for l in lines {
        let _ = w.write_record([l.name.as_str(), &fmt_num(l.qty), &l.unit, &format!("{:.2}", l.cost), &l.for_meals.join("; ")]);
    }
    for e in extras {
        let _ = w.write_record([e.name.as_str(), &fmt_num(e.qty), &e.unit, &format!("{:.2}", e.cost), ""]);
    }
    String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
}

/// Upcoming meals for the Android notification: today and the next 13 days.
pub fn meals_json(f: &Food, today: NaiveDate) -> String {
    let end = today + chrono::Duration::days(13);
    let meals: Vec<String> = f
        .plan
        .iter()
        .filter(|m| m.date >= today && m.date <= end)
        .filter_map(|m| {
            let r = f.recipe(m.recipe_id)?;
            Some(format!(
                "{{\"date\":\"{}\",\"slot\":\"{}\",\"order\":{},\"name\":{},\"recipe\":{},\"cooked\":{}}}",
                m.date.format("%Y-%m-%d"),
                m.slot.label(),
                m.slot as u8,
                serde_json::to_string(&r.name).unwrap_or_default(),
                r.id,
                m.cooked
            ))
        })
        .collect();
    format!("{{\"hide\":{},\"meals\":[{}]}}\n", f.hide_meal_notification, meals.join(","))
}

/// The recipes CSV that ships as samples (chicken teriyaki, spaghetti marinara, steak and fries).
pub const SAMPLE_RECIPES: &str = include_str!("../assets/sample_recipes.csv");

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn samples_parse() {
        let r = parse_recipes(SAMPLE_RECIPES).unwrap();
        let names: Vec<&str> = r.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Chicken Teriyaki & Veggies", "Spaghetti Marinara & Garlic Bread", "Steak & Fries with Steamed Broccoli"]);
        for x in &r {
            assert!(x.ingredients.len() >= 6, "{}", x.name);
            assert!(x.steps.len() >= 4, "{}", x.name);
            assert!(x.cost() > 5.0 && x.cost() < 25.0, "{} costs {}", x.name, x.cost());
        }
        // Round trip.
        assert_eq!(parse_recipes(&export_recipes(&r)).unwrap(), r);
    }

    #[test]
    fn ingredient_lines() {
        let i = parse_ingredient_line("1 1/2 lb chicken thighs $6").unwrap();
        assert_eq!((i.qty, i.unit.as_str(), i.name.as_str(), i.cost), (1.5, "lb", "chicken thighs", 6.0));
        let i = parse_ingredient_line("2 garlic cloves").unwrap();
        assert_eq!((i.qty, i.unit.as_str(), i.name.as_str()), (2.0, "", "garlic cloves"));
        let i = parse_ingredient_line("salt").unwrap();
        assert_eq!((i.qty, i.name.as_str()), (1.0, "salt"));
        assert_eq!(fmt_qty(1.5), "1½");
        assert_eq!(fmt_qty(0.25), "¼");
        assert_eq!(parse_ingredient_line(&ingredient_line(&parse_ingredient_line("0.5 cup soy sauce $0.90").unwrap())).unwrap().cost, 0.9);
    }

    #[test]
    fn plan_list_and_inventory() {
        let mut f = Food::default();
        for r in parse_recipes(SAMPLE_RECIPES).unwrap() {
            f.upsert_recipe(r);
        }
        let steak = f.recipe_by_name("steak & fries with steamed broccoli").unwrap().clone();
        let teri = f.recipe_by_name("Chicken Teriyaki & Veggies").unwrap().clone();
        let m1 = f.schedule(d(2026, 11, 3), Slot::Dinner, steak.id, steak.servings);
        f.schedule(d(2026, 11, 5), Slot::Dinner, teri.id, teri.servings * 2.0);
        f.schedule(d(2026, 12, 1), Slot::Dinner, teri.id, teri.servings);

        // Broccoli is in both: one head for steak night + two (double batch) for teriyaki.
        let list = f.grocery_list(d(2026, 11, 1), d(2026, 11, 7));
        let broc = list.iter().find(|l| l.name.to_lowercase() == "broccoli").unwrap();
        assert_eq!(broc.qty, 3.0);
        assert_eq!(broc.for_meals.len(), 2);

        // Stock 1 head; the list needs 2 more.
        f.stock("Broccoli", 1.0, "head");
        let list = f.grocery_list(d(2026, 11, 1), d(2026, 11, 7));
        assert_eq!(list.iter().find(|l| l.name.to_lowercase() == "broccoli").unwrap().qty, 2.0);

        // Cooking steak night uses the head we have.
        f.set_cooked(m1, true);
        assert!(f.inventory.iter().all(|i| norm(&i.name) != "broccoli"));
        let list = f.grocery_list(d(2026, 11, 1), d(2026, 11, 7));
        assert_eq!(list.iter().find(|l| l.name.to_lowercase() == "broccoli").unwrap().qty, 2.0);

        // November cost = steak + double teriyaki; December isn't included.
        let nov = f.month_meal_cost(d(2026, 11, 20));
        assert!((nov - (steak.cost() + teri.cost() * 2.0)).abs() < 1e-9);
    }

    #[test]
    fn groceries_expense_follows_plan() {
        let mut b = Budget::default();
        b.food.sync_groceries = true;
        b.food.staples = 100.0;
        let id = b.food.upsert_recipe(parse_recipes(SAMPLE_RECIPES).unwrap().remove(0));
        let cost = b.food.recipe(id).unwrap().cost();
        b.food.schedule(d(2026, 11, 3), Slot::Dinner, id, 4.0);
        let amt = sync_groceries_expense(&mut b, d(2026, 11, 1)).unwrap();
        let e = b.expenses.iter().find(|e| e.name == "Groceries").unwrap();
        assert!((e.amount - amt).abs() < 1e-9);
        assert!((amt - (cost + 100.0)).abs() < 0.01);
        b.food.schedule(d(2026, 11, 4), Slot::Dinner, id, 4.0);
        sync_groceries_expense(&mut b, d(2026, 11, 1));
        assert_eq!(b.expenses.iter().filter(|e| e.name == "Groceries").count(), 1);
        assert!((b.expenses.iter().find(|e| e.name == "Groceries").unwrap().amount - (2.0 * cost + 100.0)).abs() < 0.01);
    }

    #[test]
    fn plan_csv_round_trip() {
        let mut f = Food::default();
        for r in parse_recipes(SAMPLE_RECIPES).unwrap() {
            f.upsert_recipe(r);
        }
        let id = f.recipes[1].id;
        f.schedule(d(2026, 11, 6), Slot::Lunch, id, 2.0);
        let rows = parse_plan(&export_plan(&f)).unwrap();
        assert_eq!(rows, vec![(d(2026, 11, 6), Slot::Lunch, f.recipes[1].name.clone(), Some(2.0))]);
        let j: serde_json::Value = serde_json::from_str(&meals_json(&f, d(2026, 11, 1))).unwrap();
        assert_eq!(j["meals"][0]["recipe"], id);
    }
}
