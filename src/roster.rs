#[path="./employee.rs"]
mod employee;
use egui::emath::easing;
use employee::*;

#[path="./task.rs"]
mod task;
use task::*;

use std::collections::HashMap;
use std::fs::{OpenOptions, File};
use std::io::{self, Read, Write};
use serde::{Deserialize, Serialize};

use crate::{cap_input, clear_screen};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Roster {
    pub roster: HashMap<Task, Vec<Employee>>,
}

impl Roster {
    pub fn new() -> Self {
        Roster {
            roster: HashMap::new(),
        }
    }
    pub fn pause() {
        print!("\n\nPress [Enter] to Continue");
        clear_screen();
        let i = cap_input();
    }
    pub fn add_employee(&mut self, task: Task, employee: Employee) {
        self.roster.entry(task).or_insert_with(Vec::new).push(employee);
    }

    pub fn search_employee(&mut self, id_number: &str) -> Option<&Task> {
        for (task, employees) in &self.roster {
            for employee in employees {
                if employee.id_number == id_number {
                    return Some(task);
                }
            }
        }
        None
    }

    pub fn edit_employee_details(&mut self, id_number: &str, new_employee: Employee) -> bool {
        for employees in self.roster.values_mut() {
            for employee in employees {
                if employee.id_number == id_number {
                    *employee = new_employee;
                    return true;
                }
            }
        }
        false
    }

    pub fn change_employee_task(&mut self, id_number: &str, new_task: Task) -> bool {
        let mut employee_opt = None;
        for employees in self.roster.values_mut() {
            if let Some(pos) = employees.iter().position(|e| e.id_number == id_number) {
                employee_opt = Some(employees.remove(pos));
                break;
            }
        }

        if let Some(employee) = employee_opt {
            self.add_employee(new_task, employee);
            true
        } else {
            false
        }
    }

    pub fn save_to_file(&self, file_path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(file_path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    pub fn load_from_file(file_path: &str) -> std::io::Result<Self> {
        let mut file = File::open(file_path)?;
        let mut json = String::new();
        file.read_to_string(&mut json)?;
        let roster: Roster = serde_json::from_str(&json)?;
        Ok(roster)
    }

    pub fn display_menu(&mut self) {
        loop {
            println!("Roster Management Menu:");
            println!("1. Add Employee");
            println!("2. Search Employee");
            println!("3. Edit Employee Details");
            println!("4. Change Employee Task");
            println!("5. Save Roster to File");
            println!("6. Load Roster from File");
            println!("7. Exit");

            print!("Enter your choice: ");
            io::stdout().flush().unwrap();

            let mut choice = String::new();
            io::stdin().read_line(&mut choice).unwrap();
            let choice = choice.trim().parse::<u32>().unwrap_or(0);

            match choice {
                1 => {
                    let mut task = Task::new();
                    task.query_boat();
                    let mut employee = Employee::new();
                    employee.query_employee();
                    self.add_employee(task, employee);

                    Roster::pause();
                }
                2 => {
                    print!("Enter Employee ID to search: ");
                    io::stdout().flush().unwrap();
                    let mut id_number = String::new();
                    io::stdin().read_line(&mut id_number).unwrap();
                    let id_number = id_number.trim();
                    if let Some(task) = self.search_employee(id_number) {
                        println!("Employee found in task: {:?}", task.boat_name);
                    } else {
                        println!("Employee not found.");
                        print!("Would you like to create a new employee? (y/n): ");
                        clear_screen();

                        
                    }

                    Roster::pause();
                }
                3 => {
                    print!("Enter Employee ID to edit: ");
                    io::stdout().flush().unwrap();
                    let mut id_number = String::new();
                    io::stdin().read_line(&mut id_number).unwrap();
                    let id_number = id_number.trim();
                    let mut new_employee = Employee::new();
                    new_employee.query_employee();
                    if self.edit_employee_details(id_number, new_employee) {
                        println!("Employee details updated.");
                    } else {
                        println!("Employee not found.");
                    }

                    Roster::pause();
                }
                4 => {
                    print!("Enter Employee ID to change task: ");
                    io::stdout().flush().unwrap();
                    let mut id_number = String::new();
                    io::stdin().read_line(&mut id_number).unwrap();
                    let id_number = id_number.trim();
                    let mut new_task = Task::new();
                    new_task.query_boat();
                    if self.change_employee_task(id_number, new_task) {
                        println!("Employee task changed.");
                    } else {
                        println!("Employee not found.");
                    }

                    Roster::pause();
                }
                5 => {
                    print!("Enter file path to save roster: ");
                    io::stdout().flush().unwrap();
                    let mut file_path = String::new();
                    io::stdin().read_line(&mut file_path).unwrap();
                    let file_path = file_path.trim();
                    if self.save_to_file(file_path).is_ok() {
                        println!("Roster saved to file.");
                    } else {
                        println!("Failed to save roster.");
                    }

                    Roster::pause();
                }
                6 => {
                    print!("Enter file path to load roster: ");
                    io::stdout().flush().unwrap();
                    let mut file_path = String::new();
                    io::stdin().read_line(&mut file_path).unwrap();
                    let file_path = file_path.trim();
                    if let Ok(roster) = Roster::load_from_file(file_path) {
                        *self = roster;
                        println!("Roster loaded from file.");
                    } else {
                        println!("Failed to load roster.");
                    }

                    Roster::pause();
                }
                7 => {
                    println!("Exiting...");
                    break;
                }
                _ => println!("Invalid choice, please try again."),
            }
        }
    }
}