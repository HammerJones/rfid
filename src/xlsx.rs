//! Monthly summary as an Excel file, laid out and colored like the original "Monthly Costs" sheet.

use crate::model::{Budget, IncomeMode, MonthSummary};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};

/// Colors taken from the original Google Sheet.
const NAVY: u32 = 0x0B295B; // notes column, title, income rows
const NAVY_DARK: u32 = 0x091836; // income values
const NAVY_LIGHT: u32 = 0x173D61; // filler beside the savings rows
const SAVINGS: u32 = 0x664C05;
const SAVINGS_DARK: u32 = 0x342601;
const HEADER: u32 = 0x201F25; // column headings row

/// (category, item rows) colors. Known categories keep the sheet's colors; others cycle.
fn category_colors(name: &str, index: usize) -> (u32, u32) {
    const PALETTE: [(u32, u32); 6] = [
        (0x362957, 0x1E162B), // purple (Transportation)
        (0x651514, 0x320C0C), // red (Food)
        (0x365728, 0x1A2C14), // green (Housing)
        (0x945009, 0x633406), // orange (Monthly Subscriptions)
        (0x1F4E5F, 0x10303B), // teal
        (0x5B2C6F, 0x351A42), // plum
    ];
    let n = name.to_lowercase();
    let fixed = if n.contains("transport") {
        Some(0)
    } else if n.contains("food") || n.contains("grocer") {
        Some(1)
    } else if n.contains("hous") || n.contains("home") || n.contains("rent") {
        Some(2)
    } else if n.contains("subscri") {
        Some(3)
    } else {
        None
    };
    PALETTE[fixed.unwrap_or(index % PALETTE.len())]
}

fn base(bg: u32) -> Format {
    Format::new()
        .set_background_color(Color::RGB(bg))
        .set_font_color(Color::White)
        .set_font_name("Arial")
        .set_font_size(10)
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::White)
        .set_align(FormatAlign::VerticalCenter)
}

fn label(bg: u32) -> Format {
    base(bg).set_bold()
}

fn centered(bg: u32) -> Format {
    base(bg).set_align(FormatAlign::Center)
}

fn money_fmt(bg: u32) -> Format {
    centered(bg).set_num_format("$#,##0.00")
}

fn write_money(ws: &mut Worksheet, r: u32, c: u16, v: f64, bg: u32) -> Result<(), XlsxError> {
    ws.write_number_with_format(r, c, (v * 100.0).round() / 100.0, &money_fmt(bg))?;
    Ok(())
}

pub fn export_summary(budget: &Budget, s: &MonthSummary) -> Result<Vec<u8>, XlsxError> {
    let st = &budget.settings;
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Monthly Costs")?;
    for (c, w) in [24.0, 26.0, 24.0, 16.0, 18.0, 18.0].iter().enumerate() {
        ws.set_column_width(c as u16, *w)?;
    }

    // Title.
    ws.merge_range(
        0,
        0,
        0,
        5,
        &format!("Monthly Costs - {}", s.month_start.format("%B %Y")),
        &label(NAVY).set_font_size(13).set_align(FormatAlign::Center),
    )?;

    let mut r: u32 = 1;
    for (ci, cat) in s.categories.iter().enumerate() {
        let (cat_bg, item_bg) = category_colors(&cat.name, ci);
        let first = r;
        // Category header row: the name, then column headings on the first one (like the sheet).
        if ci == 0 {
            for (c, h) in ["Monthly Cost", "Remaining Balance", "Remaining Payments"].iter().enumerate() {
                ws.write_string_with_format(r, 3 + c as u16, *h, &label(HEADER).set_align(FormatAlign::Center))?;
            }
            ws.write_blank(r, 2, &base(cat_bg))?;
        } else {
            for c in 2..=5 {
                ws.write_blank(r, c, &base(cat_bg))?;
            }
        }
        r += 1;
        for it in &cat.items {
            ws.write_string_with_format(r, 2, &it.name, &label(item_bg))?;
            write_money(ws, r, 3, it.monthly_cost, item_bg)?;
            match it.remaining_balance {
                Some(b) => write_money(ws, r, 4, b, item_bg)?,
                None => {
                    ws.write_string_with_format(r, 4, "n/a", &centered(item_bg))?;
                }
            }
            match it.remaining_payments {
                Some(p) => {
                    ws.write_number_with_format(r, 5, p as f64, &centered(item_bg))?;
                }
                None => {
                    ws.write_string_with_format(r, 5, "n/a", &centered(item_bg))?;
                }
            }
            r += 1;
        }
        let last = r - 1;
        let cat_fmt = label(cat_bg).set_font_size(11).set_align(FormatAlign::Center).set_align(FormatAlign::Top);
        if last > first {
            ws.merge_range(first, 1, last, 1, &cat.name, &cat_fmt)?;
        } else {
            ws.write_string_with_format(first, 1, &cat.name, &cat_fmt)?;
        }
    }
    if s.categories.is_empty() {
        for (c, h) in ["Monthly Cost", "Remaining Balance", "Remaining Payments"].iter().enumerate() {
            ws.write_string_with_format(r, 3 + c as u16, *h, &label(HEADER).set_align(FormatAlign::Center))?;
        }
        r += 1;
    }

    // Savings.
    let goal = if st.savings_goal >= 1000.0 {
        format!("Months to ${}k", (st.savings_goal / 1000.0 * 10.0).round() / 10.0)
    } else {
        format!("Months to ${:.0}", st.savings_goal)
    };
    ws.merge_range(r, 1, r + 1, 1, "Emergency Fund/Savings", &label(SAVINGS).set_align(FormatAlign::Bottom).set_align(FormatAlign::Right))?;
    ws.write_string_with_format(r, 2, "Monthly Payment", &label(SAVINGS).set_align(FormatAlign::Center))?;
    ws.write_string_with_format(r, 3, &goal, &label(SAVINGS).set_align(FormatAlign::Center))?;
    ws.merge_range(r, 4, r + 1, 5, "", &base(NAVY_LIGHT))?;
    r += 1;
    write_money(ws, r, 2, s.savings, SAVINGS_DARK)?;
    match s.months_to_goal {
        Some(m) => {
            ws.write_number_with_format(r, 3, m as f64, &centered(SAVINGS_DARK))?;
        }
        None => {
            ws.write_string_with_format(r, 3, "n/a", &centered(SAVINGS_DARK))?;
        }
    }
    r += 1;

    // Income.
    let right = label(NAVY).set_align(FormatAlign::Right);
    match st.income_mode {
        IncomeMode::Hourly => {
            ws.write_string_with_format(r, 1, "Current Hourly Income", &label(NAVY).set_align(FormatAlign::Center))?;
            write_money(ws, r, 2, st.hourly_rate, NAVY_DARK)?;
            ws.write_string_with_format(r, 3, "Monthly Income:", &label(NAVY))?;
            r += 1;
            ws.write_string_with_format(r, 1, "Current Hours/week", &label(NAVY).set_align(FormatAlign::Center))?;
            ws.write_number_with_format(r, 2, st.hours_per_week, &centered(NAVY_DARK).set_num_format("0.00"))?;
            write_money(ws, r, 3, s.income, NAVY_DARK)?;
        }
        IncomeMode::Fixed => {
            ws.write_string_with_format(r, 1, "Monthly Income (after tax)", &label(NAVY).set_align(FormatAlign::Center))?;
            write_money(ws, r, 2, s.income, NAVY_DARK)?;
            ws.write_string_with_format(r, 3, "Monthly Income:", &label(NAVY))?;
            r += 1;
            ws.write_blank(r, 1, &base(NAVY))?;
            ws.write_blank(r, 2, &base(NAVY))?;
            write_money(ws, r, 3, s.income, NAVY_DARK)?;
        }
    }
    ws.merge_range(r - 1, 4, r, 5, "", &base(NAVY))?;
    r += 1;

    // Totals (added below the sheet's layout, same as the CSV export).
    ws.write_string_with_format(r, 1, "Total Expenses", &right)?;
    write_money(ws, r, 2, s.total_expenses, NAVY_DARK)?;
    r += 1;
    ws.write_string_with_format(r, 1, "Left Over", &right)?;
    let left_bg = if s.left_over < -0.004 { 0x651514 } else { 0x1A2C14 };
    write_money(ws, r, 2, s.left_over, left_bg)?;

    // Notes column down the left, like the sheet.
    let note_fmt = base(NAVY).set_text_wrap().set_align(FormatAlign::Top).set_align(FormatAlign::Left);
    if r > 1 {
        ws.merge_range(1, 0, r, 0, &st.note, &note_fmt)?;
    } else {
        ws.write_string_with_format(1, 0, &st.note, &note_fmt)?;
    }

    ws.set_freeze_panes(1, 0)?;
    // Print on one landscape page.
    ws.set_landscape();
    ws.set_print_fit_to_pages(1, 1);
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_xlsx() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let mut b = Budget::default();
        let res = crate::csvio::import(include_str!("../tests/sample_sheet.csv"), &b, day).unwrap();
        b.settings = res.settings.unwrap();
        for e in res.expenses {
            b.add_expense(e);
        }
        let s = b.summarize(chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
        let bytes = export_summary(&b, &s).unwrap();
        // xlsx is a zip file.
        assert_eq!(&bytes[..2], b"PK");
        assert!(bytes.len() > 4000);
    }

    #[test]
    fn known_categories_keep_sheet_colors() {
        assert_eq!(category_colors("Transportation", 3).0, 0x362957);
        assert_eq!(category_colors("Monthly Subscriptions", 0).0, 0x945009);
    }
}
