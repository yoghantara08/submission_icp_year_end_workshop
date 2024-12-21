#[macro_use]
extern crate serde;
use candid::{Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Todo {
    id: u64,
    title: String,
    description: String,
    status: TaskStatus,
    priority: Priority,
    due_date: Option<u64>,
    created_at: u64,
    updated_at: Option<u64>,
    owner: String,
}

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
}

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
enum Priority {
    #[default]
    Low,
    Medium,
    High,
    Urgent,
}

// Implement Storable for Todo
impl Storable for Todo {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// Implement BoundedStorable for Todo
impl BoundedStorable for Todo {
    const MAX_SIZE: u32 = 2048;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static STORAGE: RefCell<StableBTreeMap<u64, Todo, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));
}

#[derive(candid::CandidType, Serialize, Deserialize)]
struct TodoPayload {
    title: String,
    description: String,
    priority: Priority,
    due_date: Option<u64>,
}

#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
    InvalidInput { msg: String },
}

#[ic_cdk::query]
fn get_todo(id: u64) -> Result<Todo, Error> {
    match _get_todo(&id) {
        Some(todo) => Ok(todo),
        None => Err(Error::NotFound {
            msg: format!("Todo with id={} not found", id),
        }),
    }
}

#[ic_cdk::update]
fn add_todo(payload: TodoPayload) -> Result<Todo, Error> {
    // Input validation
    if payload.title.trim().is_empty() {
        return Err(Error::InvalidInput {
            msg: "Title cannot be empty".to_string(),
        });
    }

    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter");

    let todo = Todo {
        id,
        title: payload.title,
        description: payload.description,
        status: TaskStatus::Pending,
        priority: payload.priority,
        due_date: payload.due_date,
        created_at: time(),
        updated_at: None,
        owner: ic_cdk::caller().to_string(),
    };

    do_insert(&todo);
    Ok(todo)
}

#[ic_cdk::update]
fn update_todo(id: u64, payload: TodoPayload) -> Result<Todo, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut todo) => {
            // Verify owner
            if todo.owner != ic_cdk::caller().to_string() {
                return Err(Error::NotFound {
                    msg: format!("Not authorized to update todo with id={}", id),
                });
            }

            todo.title = payload.title;
            todo.description = payload.description;
            todo.priority = payload.priority;
            todo.due_date = payload.due_date;
            todo.updated_at = Some(time());

            do_insert(&todo);
            Ok(todo)
        }
        None => Err(Error::NotFound {
            msg: format!("Couldn't update todo with id={}. Todo not found", id),
        }),
    }
}

#[ic_cdk::update]
fn delete_todo(id: u64) -> Result<Todo, Error> {
    match STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(todo) => {
            // Verify owner
            if todo.owner != ic_cdk::caller().to_string() {
                return Err(Error::NotFound {
                    msg: format!("Not authorized to delete todo with id={}", id),
                });
            }
            Ok(todo)
        }
        None => Err(Error::NotFound {
            msg: format!("Couldn't delete todo with id={}. Todo not found.", id),
        }),
    }
}

#[ic_cdk::update]
fn update_status(id: u64, status: TaskStatus) -> Result<Todo, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut todo) => {
            // Verify owner
            if todo.owner != ic_cdk::caller().to_string() {
                return Err(Error::NotFound {
                    msg: format!("Not authorized to update todo with id={}", id),
                });
            }

            todo.status = status;
            todo.updated_at = Some(time());

            do_insert(&todo);
            Ok(todo)
        }
        None => Err(Error::NotFound {
            msg: format!("Couldn't update todo status with id={}. Todo not found", id),
        }),
    }
}

// Helper function to insert todo
fn do_insert(todo: &Todo) {
    STORAGE.with(|service| service.borrow_mut().insert(todo.id, todo.clone()));
}

// Helper function to get todo
fn _get_todo(id: &u64) -> Option<Todo> {
    STORAGE.with(|service| service.borrow().get(id))
}

// Export Candid interface
ic_cdk::export_candid!();
