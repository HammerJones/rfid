use std::collections::binary_heap::Iter;
//use k_board::{keyboard::Keyboard, keys::Keys};
use std::io::Write;
use std::fs;
use std::fs::OpenOptions;

/* ----- Internal Dependencies ----- */

mod input;
#[allow(unused_imports)]
use input::*;

mod file_handling;
use file_handling::*;

mod task;
use task::*;

mod employee;
use employee::*;

pub struct Menu {

}



#[allow(dead_code)]
fn test_boat() {
    let mut boat1: Task = Task::new();

    boat1.query_boat();
    boat1.create_tasks_vec();
    boat1.display_boat_info();
}

#[allow(dead_code)]
fn test_employee() {
    let mut test: Employee = Employee::new();

    test.query_employee();
    test.print_employee_details();
}

fn write_file(file_name: &String, data: &str) {
    let mut f = OpenOptions::new()
        .append(true)
        .create(true) // Optionally create the file if it doesn't already exist
        .open(&file_name)
        .expect("Unable to open file");
    f.write_all(data.as_bytes()).expect("Unable to write data");
}

fn main() {
    let mut boat = Task::new();
    boat.construct_boat();
    let file = String::from(boat.boat_name.to_owned() + ".txt");

    boat.write_to_json_file();

    
    let mut boat2 = Task::read_from_json_file(&file).unwrap();
    print!("Read from file:\n---------------\n");
    boat2.display_boat_info();
}
