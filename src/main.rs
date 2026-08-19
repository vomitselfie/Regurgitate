use anyhow::Result;

fn main() -> Result<()> {
    if regurgitate::aoe_plugin::is_worker_invocation() {
        return regurgitate::aoe_plugin::run();
    }
    regurgitate::runtime::execute(regurgitate::cli::Cli::from_env())
}
