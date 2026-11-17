use rux::Signal;

/// Information entered by the user.
#[derive(Clone, Default)]
pub struct ProfileState {
    name: Signal<String>,
}

impl ProfileState {
    pub fn name(&self) -> String {
        self.name.get()
    }

    pub fn set_name(&self, name: String) {
        self.name.set(name);
    }

    pub fn greeting(&self) -> String {
        self.name.with(|name| {
            if name.is_empty() {
                "Hello!".to_string()
            } else {
                format!("Hello, {name}!")
            }
        })
    }
}
