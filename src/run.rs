use std::{collections::HashMap, path::PathBuf};

pub(crate) fn cix_run(
    workflow: PathBuf,
    secrets: HashMap<String, String>,
    vars: HashMap<String, String>,
) -> color_eyre::Result<()> {
    let path = if workflow.is_relative() {
        format!("./. + {}", serde_json::to_string(&workflow)?)
    } else {
        format!("/. + {}", serde_json::to_string(&workflow)?)
    };
    let secrets_json =
        serde_json::to_string(&serde_json::to_string(&secrets.keys().collect::<Vec<_>>())?)?;
    let vars_json = serde_json::to_string(&serde_json::to_string(&vars)?)?;
    let command = format!(
        "(import ./nix/workflow.nix {{ }}) ({path}) {{ secrets = builtins.fromJSON {secrets_json}; vars = builtins.fromJSON {vars_json}; }}"
    );

    Ok(())
}
