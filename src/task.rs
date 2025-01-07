use std::collections::HashMap;

#[path="./input.rs"]
mod input;
use input::*;

#[path="./employee.rs"]
pub mod employee;
use employee::*;

pub struct Task {
    pub boat_name: String,
    pub task_numbers: Vec<String>,
    pub task_hash: HashMap<String, Vec<Employee>>,
    pub menu: HashMap<String, String>,
}


impl Task {
    pub fn new() -> Self {
        Task {
            boat_name: String::new(),
            task_numbers: Vec::new(),
            task_hash: HashMap::new(),
            menu: HashMap::new(),
        }
    }
    pub fn query_boat(&mut self) {
        flush_output();
        print!("Please enter boat name: ");
        clear_screen();

        self.boat_name = cap_input();
    }
    pub fn query_tasks(&mut self) {
        let input: String;
    
        print!("Please enter Task Number for {}: ", self.boat_name);
        clear_screen();
        input = cap_input();
    
        self.task_numbers.push(input);
    }
    pub fn create_tasks_vec(&mut self) {
        print!("Would you like to enter a new Task Number? [y/n]: ");
        clear_screen();
        while cap_input() == "y" {
            flush_output();
            self.query_tasks();
            print!("Would you like to enter a new Task Number? [y/n]: ");
            clear_screen();
        }
        let mut i = 0;
        for item in &self.task_numbers {
            i += 1;
            print!("Task[{:?}]: {}\n", i, item);
            clear_screen();
        }
    }
}
