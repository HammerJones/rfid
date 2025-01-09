use core::task;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Write, Read};
use std::fs::File;
use serde::{Deserialize, Serialize};

#[path="./input.rs"]
mod input;
use input::*;

#[path="./employee.rs"]
pub mod employee;
use employee::*;



#[derive(Serialize, Deserialize)]
pub struct Task {
    pub boat_name: String,
    pub task_numbers: Vec<String>,
    pub task_hash: HashMap<String, Vec<Employee>>,
    #[allow(dead_code)]
    pub menu: HashMap<String, String>,
}


impl Task {
    /* ----------------------------- */
    /* ----- Private Functions ----- */
    /* ----------------------------- */
    fn boat_name_query() {
        flush_output();
        print!("Please enter boat name: ");
        clear_screen();
    }

    fn employee_roster_query_loop(new_roster: &mut Vec<Employee>, task_number: String) {
        print!("Would you like to create an employee roster for Task Number: [{}]? [y/n] ", task_number);
        clear_screen();
        while cap_input() == "y" {
            let mut new_employee: Employee = Employee::new();
            new_employee.query_employee();

            new_roster.push(new_employee);

            print!("Would you like to enter another Employee? [y/n]: ");
            clear_screen();
        }
    }
    /* -------------------------- */
    /* ----- Struct Methods ----- */
    /* -------------------------- */
    pub fn new() -> Self {
        Task {
            boat_name: String::new(),
            task_numbers: Vec::new(),
            task_hash: HashMap::new(),
            menu: HashMap::new(),
        }
    }
    pub fn query_boat(&mut self) {
        Task::boat_name_query();

        self.boat_name = cap_input();
    }
    pub fn query_tasks(&mut self) -> String {
        print!("Please enter Task Number for this roster on {}: ", self.boat_name);
        clear_screen();
        let output = cap_input();
        self.task_hash.insert(output.clone(), Vec::new());

        output
    }
    pub fn query_employee_roster(&mut self, task_number: String) -> Vec<Employee> {
        let mut new_roster: Vec<Employee> = Vec::new();

        Task::employee_roster_query_loop(&mut new_roster, task_number);

        new_roster
    }
    pub fn create_tasks_vec(&mut self) {
        print!("Would you like to enter a new Task Number? [y/n]: ");
        clear_screen();
        while cap_input() == "y" {
            flush_output();
            let task = self.query_tasks();
            let local_query = Task::query_employee_roster(self, task.clone());

            self.task_hash.insert(task, local_query);
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

            let mut i = 0;
            for item in value {
                i += 1;
                print!("[{}]: {} {} - {}\n", i, item.first_name, item.last_name, item.department);
                clear_screen();
            }
        }
    }
    pub fn construct_boat(&mut self) -> &Task {
        self.query_boat();
        self.create_tasks_vec();
        self.display_boat_info();

        self
    }
    pub fn write_to_json_file(&self) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        let file_name = format!("./src/data_files/{}.txt", self.boat_name);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(file_name)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
    pub fn read_from_json_file(file_path: &str) -> std::io::Result<Task> {
        let mut file = File::open(file_path)?;
        let mut json = String::new();
        file.read_to_string(&mut json)?;
        let task: Task = serde_json::from_str(&json)?;
        Ok(task)
    }
    pub fn find_employee_by_id(file_path: &str, id_number: &str) -> std::io::Result<Option<Employee>> {
        let task = Task::read_from_json_file(file_path)?;
        for employees in task.task_hash.values() {
            for employee in employees {
                if employee.id_number == id_number {
                    return Ok(Some(employee.clone()));
                }
            }
        }
        Ok(None)
    }
    /* -------------------------- */
    /* ----- Menu Functions ----- */
    /* -------------------------- */
}
