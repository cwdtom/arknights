mod common;
mod get;
mod insert;
mod list;
mod list_by_tag;
mod remove;
mod search;
mod update;

const GROUP_NAME: &str = "schedule";
const GROUP_DESC: &str = "Schedule CRUD tools.";

pub use get::Get;
pub use insert::Insert;
pub use list::List;
pub use list_by_tag::ListByTag;
pub use remove::Remove;
pub use search::Search;
pub use update::Update;

#[cfg(test)]
mod tests;
