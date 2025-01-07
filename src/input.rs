#![allow(dead_code)]
#[allow(unused_imports)]
use std::io::Write;


/* ----- Input Functions ----- */

pub fn cap_input() -> String {
    let mut output = String::new();
    std::io::stdin().read_line(&mut output).expect("Failed to Capture User Input");
    output.trim().to_string()
}
pub fn clear_screen() {
    std::io::stdout().flush().unwrap();
}
pub fn flush_output() {
    print!("\x1B[2J\x1B[1;1H");
}

/* ----------- */