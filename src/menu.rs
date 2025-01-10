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
        print!("[3] - Add Employee to Task\n");
        print!("[4] - Delete Task\n");
        print!("[5] - Search Employee\n");
        print!("[6] - Exit\n");
        clear_screen();

        self.get_menu_input();
    }
    pub fn get_menu_input(&self) {
        let option = cap_input().parse().unwrap();
        clear_screen();
        match option {
            1 => {
                flush_output();
                let mut new_boat = Task::new();
                new_boat.construct_boat();
                new_boat.write_to_json_file();

                self.display_main_menu();
            }
            2 => {
                print!("Display Boat Menu\n\n");
                print!("Please Enter Boat Name: ");
                clear_screen();

                let file_path= String::from("./src/data_files/".to_owned() + &cap_input() + ".txt");
                let mut boat = Task::read_from_json_file(&file_path).unwrap();
                flush_output();
                boat.display_boat_info();

                print!("Press [Enter] to return to main menu");
                clear_screen();
                let pause = cap_input();
                

                self.display_main_menu();
            }
            3 => {
                print!("Add Employee to Task Number\n\n");
                clear_screen();

                let file_path= String::from("./src/data_files/".to_owned() + &cap_input() + ".txt");
                let mut boat = Task::read_from_json_file(&file_path).unwrap();

                boat.display_boat_info();
                Task::append_employee_to_task(&file_path);

                self.display_main_menu();
            }
            4 => {
                print!("Delete Task\n");
            }
            5 => {
                print!("Search Employee\n\n");
                print!("Please Enter the Employee ID You Wish to Search: ");
                clear_screen();
                let id_number= &cap_input();
                let boat = Task::find_employee_by_id(id_number).unwrap();

                boat.unwrap().display_boat_info();
                print!("Press any key to return to main menu");
                clear_screen();
                let pause = cap_input();
                

                self.display_main_menu();
            }
            6 => {
                print!("Exit\n");
                std::process::exit(0);
            }
            _ => {
                print!("Invalid option\n");
            }
        }
    }
}