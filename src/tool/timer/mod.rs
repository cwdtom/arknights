mod common;
mod get;
mod insert;
mod list;
mod remove;
mod update;

const GROUP_NAME: &str = "timer";
const GROUP_DESC: &str = "Use for timer-task(not for user) CRUD that triggers the agent later. It is not for calendar or schedule-event records.";

pub use get::Get;
pub use insert::Insert;
pub use list::List;
pub use remove::Remove;
pub use update::Update;

#[cfg(test)]
mod tests;
