use std::io;
use std::io::{Read, Write};
use std::collections::HashMap;

/* ----- Input Functions ----- */
pub fn cap_input() -> String {

    let mut output = String::new();
    
    io::stdin().read_line(&mut output).expect("Failed to Capture User Input");

    output.trim().to_string()
    
}

pub fn clear_screen() {
    io::stdout().flush().unwrap();
}

pub fn flush_output() {
    print!("\x1B[2J\x1B[1;1H");
}
/* ----------- */


fn query_tasks(tasks: &mut Vec<String>) {
    let input: String;

    print!("Please enter Task Number: ");
    clear_screen();
    input = cap_input();

    tasks.push(input);
}

fn create_tasks_vec() -> Vec<String> {
    let mut tasks: Vec<String> = Vec::new();

    print!("Would you like to enter a new Task Number? [y/n]: ");
    clear_screen();
    while cap_input() == "y" {
        flush_output();
        query_tasks(&mut tasks);
        print!("Would you like to enter a new Task Number? [y/n]: ");
        clear_screen();
    }
    let mut i = 0;
    for item in &tasks {
        i += 1;
        print!("Task[{:?}]: {}\n", i, item);
        clear_screen();
    }

    tasks
}

fn main() {
    let mut azteca: HashMap<String, Vec<String>> = HashMap::new();
    print!("Please enter Boat Name: ");
    clear_screen();
    azteca.insert(cap_input(), create_tasks_vec());

    for (key, value) in azteca {
        print!("Boat: {}\n", key);
        clear_screen();
        for item in value {
            print!("Task: {}", item);
            clear_screen();
        }
    }
}
