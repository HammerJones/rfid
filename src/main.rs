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

fn main() {
    let file = String::from("./src/data_files/Azteca.txt");
    let mut boat2 = Task::read_from_json_file(&file).unwrap();

    let menu = Menu::new();
    menu.display_main_menu();
}
