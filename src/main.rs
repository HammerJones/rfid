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

mod roster;
use roster::*;

#[allow(dead_code)]

fn main() {

    let menu = Menu::new();
    menu.display_main_menu();

    //let mut roster = Roster::new();
    //roster.display_menu();
}
