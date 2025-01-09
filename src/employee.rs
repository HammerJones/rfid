#![allow(dead_code)]

use struct_iterable::Iterable;
use serde::{Deserialize, Serialize};


#[path="./input.rs"]
mod input;
use input::*;

#[derive(Iterable, Serialize, Deserialize)]
pub struct Employee {
    pub first_name: String,
    pub last_name: String,
    pub department: String,
    pub id_number: String,
}

impl Employee {
    //#[allow(dead_code)]
    /* ----------------------------- */
    /* ----- Private Functions ----- */
    /* ----------------------------- */
    fn first_name_query(&mut self) {
        print!("Please enter First Name: ");
        clear_screen();
        self.first_name = cap_input();
    }
    
    fn last_name_query(&mut self) {
        print!("Please enter Last Name: ");
        clear_screen();
        self.last_name = cap_input();
    }
    
    fn department_query(&mut self) {
        print!("Please enter Employee Department: ");
        clear_screen();
        self.department = cap_input();
    }
    
    fn id_query(&mut self) {
        print!("Please enter ID Number: ");
        clear_screen();
        self.id_number = cap_input();
    }
    


    fn capitalize(name: &str, output: &mut Vec<String>) {
        let mut v: Vec<char> = name.chars().collect();
            v[0] = v[0].to_uppercase().nth(0).unwrap();

            let s1: String = v.into_iter().collect();
            let s2 = &s1;

            output.push(s2.to_string());
    }
    
    fn output_conditional(output: &mut Vec<String>) -> String {
        if output.len() > 1 {
            let output1 = &output[0];
            let output2 = &output[1];
    
            String::from(output1.to_owned() + " " + &output2)
        } else {
            let output1 = &output[0];

            String::from(output1)
        }
    }
    /* -------------------------- */
    /* ----- Struct Methods ----- */
    /* -------------------------- */

        // Returns empty 'Task' Object
    pub fn new() -> Self {
        Employee {
            first_name: String::new(),
            last_name: String::new(),
            department: String::new(),
            id_number: String::new(),
        }
    }
        // Gathers data for 'Employee' Object
    pub fn query_employee(&mut self) {
        //flush_output();
        /* Calls cap_input() to gather data from user */
        self.first_name_query();
        self.last_name_query();
        self.department_query();
        self.id_query();

        flush_output();
    }
    
    pub fn construct_employee(&mut self,
            first_name: String, 
            last_name: String, 
            department: String, 
            id_number: String) -> Employee {
                Employee {
                    first_name,
                    last_name,
                    department,
                    id_number,
                }
            }
    
    pub fn format_field_name(input: &str) -> String {
        let names = input.split("_");
        let mut output: Vec<String> = Vec::new();
        
        for name in names {
            Employee::capitalize(name, &mut output);
        }

        Employee::output_conditional(&mut output)
    }    
    
    pub fn print_employee_details(&mut self) {
        for (key, value) in self.iter() {
            
            print!("{}: {}\n", 
                
                Employee::format_field_name(key), 

                value.downcast_ref::<String>()
                    .unwrap_or(&String::from("Data not initialized")));
        }
    }
}