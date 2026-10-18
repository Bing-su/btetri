mod app;
mod cli;
mod config;
mod runtime;
mod storage;
mod ui;

use btetri::game;

fn main() -> miette::Result<()> {
    runtime::run()
}
