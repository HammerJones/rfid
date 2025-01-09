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

mod menu;
use menu::*;



#[allow(dead_code)]


fn test() {
    //let mut boat = Task::new();
    //boat.construct_boat();
    let file = String::from("Azteca.txt");

    //boat.write_to_json_file();

    
    let mut boat2 = Task::read_from_json_file(&file).unwrap();
    print!("Read from file:\n---------------\n");
    boat2.display_boat_info();

    let employee = Task::find_employee_by_id(&file, "521").unwrap();
    match employee {
        Some(employee) => {
            print!("Found Employee:\n---------------\n");
            print!("{} {} - {}\n", employee.first_name, employee.last_name, employee.department);
        },
        None => {
            print!("Employee not found\n");
        }
    }
}

fn main() {
    let file = String::from("./src/data_files/Azteca.txt");
    let mut boat2 = Task::read_from_json_file(&file).unwrap();

    let menu = Menu::new();
    menu.display_main_menu();
    
}
