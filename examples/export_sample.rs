//! Prints the monthly summary CSV for the bundled spreadsheet: `cargo run --example export_sample`
use budget_app::{csvio, model::Budget};
use chrono::NaiveDate;

fn main() {
    let month = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
    let res = csvio::import(include_str!("../tests/sample_sheet.csv"), &Budget::default(), month).unwrap();
    let mut b = Budget { settings: res.settings.unwrap(), ..Default::default() };
    for e in res.expenses {
        b.add_expense(e);
    }
    print!("{}", csvio::export_summary(&b, &b.summarize(month)));
}
