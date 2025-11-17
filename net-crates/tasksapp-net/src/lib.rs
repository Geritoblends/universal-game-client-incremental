use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub id: i32,
    pub title: String,
    pub priority: i32,
    pub completed: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NewTaskError {
    TaskAlreadyExists,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NewTaskResult {
    Success(Task),
    Error(NewTaskError),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewTaskRequest {
    pub title: String,
    pub priority: i32,
    pub completed: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum QueryByIdResult {
    Success(Task),
    NotFoundError,
}
