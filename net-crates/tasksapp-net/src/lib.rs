use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Task {
    id: i32,
    title: String,
    priority: u8,
    completed: bool,
}

#[derive(Serialize, Deserialize)]
pub enum NewTaskError {
    TaskAlreadyExists,
}

#[derive(Serialize, Deserialize)]
pub enum NewTaskResult {
    Success(Task),
    Error(NewTaskError),
}

#[derive(Serialize, Deserialize)]
pub enum QueryByIdResult {
    Success(Task),
    NotFoundError,
}
