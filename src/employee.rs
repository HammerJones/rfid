use struct_iterable::Iterable;

#[path="./input.rs"]
mod input;
use input::*;

#[derive(Iterable)]
pub struct Employee {
    pub first_name: String,
    pub last_name: String,
    pub department: String,
    pub id_number: String,
    pub internal_number: String,
}

impl Employee {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Employee {
            first_name: String::new(),
            last_name: String::new(),
            department: String::new(),
            id_number: String::new(),
            internal_number: String::new(),
        }
    }
    #[allow(dead_code)]
    pub fn query_employee(&mut self) {
        flush_output();
        
        print!("Please enter First Name: ");
        clear_screen();
        self.first_name = cap_input();

        print!("Please enter Last Name: ");
        clear_screen();
        self.last_name = cap_input();

        print!("Please enter Employee Department: ");
        clear_screen();
        self.department = cap_input();

        print!("Please enter ID Number: ");
        clear_screen();
        self.id_number = cap_input();

        flush_output();
    }
    #[allow(dead_code)]
    pub fn format_field_name(input: &str) -> String {
        let names = input.split("_");
        let mut output: Vec<String> = Vec::new();
        
        for name in names {
            let mut v: Vec<char> = name.chars().collect();
            v[0] = v[0].to_uppercase().nth(0).unwrap();

            let s1: String = v.into_iter().collect();
            let s2 = &s1;

            output.push(s2.to_string());
        }
        if output.len() > 1 {
            let output1 = &output[0];
            let output2 = &output[1];
    
            String::from(output1.to_owned() + " " + &output2)
        } else {
            let output1 = &output[0];

            String::from(output1)
        }
    }    
    #[allow(dead_code)]
    pub fn print_employee_details(&mut self) {
        for (key, value) in self.iter() {
            print!("{}: {}\n", 
                Employee::format_field_name(key), 
                value.downcast_ref::<String>()
                    .unwrap_or(&String::from("Data not initialized")));
        }
    }
}