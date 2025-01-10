use core::task;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::fs::OpenOptions;
use std::io; 
use std::io::{Write, Read};
use std::fs;
use std::fs::File;
use serde::{Deserialize, Serialize};

#[path="./input.rs"]
mod input;
use input::*;

#[path="./employee.rs"]
pub mod employee;
use employee::*;



#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Task {
    //pub file_path: String,
    pub boat_name: String,
    pub task_numbers: Vec<String>,
    pub task_hash: HashMap<String, Vec<Employee>>,
    #[allow(dead_code)]
    pub menu: HashMap<String, String>,
}


impl Task {
    // New Task Functions
        // new() initializes object file path. To change file path
        // use this function
    pub fn new() -> Self {
        Task {
            //file_path: String::from("./src/data_files/"),
            boat_name: String::new(),
            task_numbers: Vec::new(),
            task_hash: HashMap::new(),
            menu: HashMap::new(),
        }
    }
    
/* ----- New & Init Functions ----- */
// These 
    // Employee Functions
    fn employee_roster_query_loop(
        new_roster: &mut Vec<Employee>, 
        task_number: String) {
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
    
    // Task Query Functions
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

    // Boat Query Functions
    fn boat_name_query() {
        flush_output();
        print!("Please enter boat name: ");
        clear_screen();
    }
    pub fn query_boat(&mut self) {
        Task::boat_name_query();

        self.boat_name = cap_input();
    }
    
    // Boat Construct Function
    pub fn construct_boat(&mut self) -> &Task {
        self.query_boat();
        self.create_tasks_vec();
        self.display_boat_info();

        self
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
                print!("[{}]: ID: {} - {} {} - {}\n", i, item.id_number, item.first_name, item.last_name, item.department);
                clear_screen();
            }
        }
    }
    

    // File Reading & Writing
        // Writes to a new JSON formatted file
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
        // Reads from a specified JSON formatted file, 
    pub fn read_from_json_file(file_path: &str) -> std::io::Result<Task> {
        let mut file = File::open(file_path)?;
        let mut json = String::new();
        file.read_to_string(&mut json)?;
        let task: Task = serde_json::from_str(&json)?;
        Ok(task)
    }
        // Appends an employee to specified task number
    pub fn append_employee_to_task(file_path: &str) -> std::io::Result<()> {
        let mut task = Task::read_from_json_file(file_path)?;

        print!("Enter Task Number to append employee: ");
        io::stdout().flush().unwrap();
        let mut task_number = String::new();
        io::stdin().read_line(&mut task_number).unwrap();
        let task_number = task_number.trim().to_string();

        if let Some(employees) = task.task_hash.get_mut(&task_number) {
            let mut new_employee = Employee::new();
            new_employee.query_employee();
            employees.push(new_employee);
        } else {
            println!("Task number not found.");
            return Ok(());
        }

        task.write_to_json_file()
    }

    pub fn find_employee_by_id(id_number: &str) -> std::io::Result<Option<Task>> {
        let paths = fs::read_dir("./src/data_files")?;
        for path in paths {
            let path = path?.path();
            if path.is_file() {
                let task = Task::read_from_json_file(path.to_str().unwrap())?;
                for employees in task.task_hash.values() {
                    for employee in employees {
                        if employee.id_number == id_number {
                            return Ok(Some(task));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

impl Hash for Task {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.boat_name.hash(state);
        self.task_numbers.hash(state);
        // Note: You may need to hash other fields if they are relevant for equality
    }
}