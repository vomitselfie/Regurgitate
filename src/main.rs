use anyhow::Result;

fn main() -> Result<()> {
    praxis::runtime::execute(praxis::cli::Cli::from_env())
}
