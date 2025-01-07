use std::collections::{btree_map::Values, HashMap};

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
    pub fn query_tasks(&mut self, roster: Vec<Employee>) {
        let input: String;
    
        print!("Please enter Task Number for this roster on {}: ", self.boat_name);
        clear_screen();

        self.task_hash.insert(cap_input(), roster);
    }
    pub fn query_employee_roster(&mut self) -> Vec<Employee> {
        let mut new_roster: Vec<Employee> = Vec::new();

        print!("Would you like to create an employee roster for this Task Number? [y/n]");
        clear_screen();
        while cap_input() == "y" {
            let mut new_employee: Employee = Employee::new();
            new_employee.query_employee();

            new_roster.push(new_employee);

            print!("Would you like to enter another Employee? [y/n]: ");
            clear_screen();
        }

        new_roster
    }
    pub fn create_tasks_vec(&mut self) {
        print!("Would you like to enter a new Task Number and/or Employee Roster? [y/n]: ");
        clear_screen();
        while cap_input() == "y" {
            flush_output();
            let local_query = Task::query_employee_roster(self);
            self.query_tasks(local_query);
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
    pub fn display_boat_info(&mut self) {
        print!("Boat Name: {}\n", self.boat_name);
        clear_screen();

        for (key, value) in &self.task_hash {
            print!("Task Number: {}\nRoster:\n", key);
            clear_screen();

            for item in value {
                print!("{} {} - {}\n", item.first_name, item.last_name, item.department);
                clear_screen();
            }
        }
    }
}
