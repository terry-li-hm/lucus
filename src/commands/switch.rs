use anyhow::Result;

use crate::commands::query::{self, BranchRef};
use crate::config::Config;

pub fn run(config: &Config, branch_ref: &BranchRef) -> Result<()> {
    query::run(config, branch_ref)
}
