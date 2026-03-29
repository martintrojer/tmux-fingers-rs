#[derive(Debug, Clone, Default)]
pub struct State {
    pub multi_mode: bool,
    pub input: String,
    pub modifier: String,
    pub selected_hints: Vec<String>,
    pub multi_matches: Vec<String>,
    pub result: String,
    pub exiting: bool,
}
