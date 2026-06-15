use std::fmt;

struct User {
    id: i32,
}

impl User {
    fn new(id: i32) -> Self {
        User { id }
    }
}

fn helper() {
    User::new(1);
}
