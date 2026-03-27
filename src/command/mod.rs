#[path = "command.rs"]
mod command_impl;

pub(crate) use command_impl::execute;
