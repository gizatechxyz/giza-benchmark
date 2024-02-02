mod runner;

use clap::{App, Arg};
use crate::runner::{process_args, run};
use cairo_lang_sierra::ProgramParser;

#[tokio::main]
async fn main() {
    let matches = App::new("Cairo Runner CLI")
        .version("1.0")
        .about("Runs a Cairo program from a Sierra file with provided arguments")
        .arg(Arg::with_name("sierra_file")
            .short('f')
            .long("file")
            .value_name("SIERRA_FILE")
            .help("Sets the Sierra file to use")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("args")
            .short('a')
            .long("args")
            .value_name("ARGS")
            .help("Arguments to pass to the Cairo program, separated by spaces")
            .takes_value(true)
            .required(false))
        .get_matches();

    let sierra_file = matches.value_of("sierra_file").unwrap();
    let args = matches.value_of("args").unwrap_or("");

    let sierra_content = std::fs::read_to_string(sierra_file)
        .expect("Failed to read Sierra file");

    let sierra_program = ProgramParser::new().parse(&sierra_content)
        .expect("Failed to parse Sierra program");

    let args = process_args(args)
        .expect("Failed to process provided arguments");

    match run(&sierra_program, &None, &None, &args).await {
        Ok(result) => println!("✅ Program executed successfully: {:?}", result),
        Err(e) => eprintln!("Error executing program: {:?}", e),
    }
}
