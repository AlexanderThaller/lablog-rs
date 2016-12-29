// build.rs
#[macro_use]
extern crate clap;

use clap::{App,Shell};

fn main() {
    let yaml = load_yaml!("src/cli.yml");
    let mut app = App::from_yaml(yaml).version(crate_version!());

    app.gen_completions("lablog", Shell::Bash, "autocomplete");
    app.gen_completions("lablog", Shell::Fish, "autocomplete");
    app.gen_completions("lablog", Shell::Zsh, "autocomplete");
}
