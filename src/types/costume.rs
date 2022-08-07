#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Costume {
    pub body_name: String,
    pub cap_name: String,
}

impl Default for Costume {
    fn default() -> Self {
        Self {
            body_name: "Mario".to_string(),
            cap_name: "Mario".to_string(),
        }
    }
}
