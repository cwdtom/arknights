mod bash;
mod date;

pub use bash::BashTool;
pub use date::DateTool;

const GROUP_NAME: &str = "system";
const GROUP_DESC: &str = "System tools(include `date`, `bash command`).";

#[cfg(test)]
mod tests;
