
/* ----- Internal Dependencies ----- */
mod input;
#[allow(unused_imports)]
use input::*;

mod task;
use task::*;

mod employee;
use employee::*;

#[allow(dead_code)]
fn test_boat() {
    let mut boat1: Task = Task::new();

    boat1.query_boat();
    boat1.create_tasks_vec();
}

#[allow(dead_code)]
fn test_employee() {
    let mut test: Employee = Employee::new();

    test.query_employee();
    test.print_employee_details();
}

fn main() {
    
}
