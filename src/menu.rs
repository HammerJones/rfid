use crate::{cap_input, clear_screen, flush_output, task::Task};
pub struct Menu {
    menu_title: String,
    menu_description: String,
    menu_options: Vec<String>,
}

enum MainMenu {
    AddBoat,
    DisplayBoat,
    EditTask,
    DeleteTask,
    SearchEmployee,
    Exit,
}

impl Menu {
    pub fn new() -> Self {
        Menu {
            menu_title: String::new(),
            menu_description: String::new(),
            menu_options: Vec::new(),
        }
    }
    pub fn display_task(&self, task: &Task) {

        println!("{} - {}", self.menu_title, self.menu_description);
        println!("Boat Name: {}", task.boat_name);
        println!("Task Numbers:");
        for (i, task_number) in task.task_numbers.iter().enumerate() {
            println!("  {}. {}", i + 1, task_number);
        }
        println!("Task Details:");
        for (task_number, employees) in &task.task_hash {
            println!("  Task Number: {}", task_number);
            for (i, employee) in employees.iter().enumerate() {
                println!(
                    "    {}. {} {} - {}",
                    i + 1,
                    employee.first_name,
                    employee.last_name,
                    employee.department
                );
            }
        }
    }
    pub fn display_main_menu(&self) {
        flush_output();
        print!("Welcome to MGBW-RFID!\n");
        print!("Please select from the following options:\n");
        print!("[1] - Add Boat\n");
        print!("[2] - Display Boat\n");
        print!("[3] - Edit Task\n");
        print!("[4] - Delete Task\n");
        print!("[5] - Search Employee\n");
        print!("[6] - Exit\n");
        clear_screen();

        let mut option = cap_input().parse().unwrap();
        match option {
            1 => {
               let mut new_boat = Task::new();
               new_boat.construct_boat();
               new_boat.write_to_json_file();
            }
            2 => {
                print!("Display Boat\n");
            }
            3 => {
                print!("Edit Task\n");
            }
            4 => {
                print!("Delete Task\n");
            }
            5 => {
                print!("Search Employee\n");
            }
            6 => {
                print!("Exit\n");
            }
            _ => {
                print!("Invalid option\n");
            }
        }
    }
}