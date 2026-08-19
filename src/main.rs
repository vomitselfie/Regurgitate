use anyhow::Result;

fn main() -> Result<()> {
    if praxis::aoe_plugin::is_worker_invocation() {
        return praxis::aoe_plugin::run();
    }
    praxis::runtime::execute(praxis::cli::Cli::from_env())
}
