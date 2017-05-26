extern crate time;
extern crate squid;

use squid::download;


static SEPARATOR: &'static str = "--------------------";

fn main()
{
    use squid::command_line;
    use std::process;

    match command_line::parse_arguments()
    {
        Ok(arguments) =>
        {
            let be_verbose = arguments.program_options.be_verbose;

            if be_verbose
            {
                println!("{}", SEPARATOR);
                println!("Target URL: {:?}", &arguments.target_url);
                println!("Program options: {:?}", &arguments.program_options);
                println!("Download options: {:?}", &arguments.download_options);
            }
            match download::Task::new
            (
                arguments.target_url.as_str(),
                &arguments.download_options
            )
            {
                Ok(mut task) =>
                {
                    match begin_download(&mut task, be_verbose)
                    {
                        Ok(path) => println!("Output saved to: {}", path),
                        Err(info) => report_error(&info),
                    }
                },
                Err(info) => report_error(&info),
            }
        },
        Err(command_line::ParseError::OnlyHelpRequested) => process::exit(0),
        Err(command_line::ParseError::InvalidArguments) => process::exit(2),
        Err(command_line::ParseError::Unknown) => process::exit(1),
    }
}
fn begin_download<'a>(task: &'a mut download::Task, be_verbose: bool)
    -> Result<&'a str, download::Error>
{
    if be_verbose
    {
        println!("{}", SEPARATOR);
        println!
        (
            "Target content length: {} bytes.",
            task.target_content_length()
        );
        println!
        (
            "Target supports partial content: {}",
            task.target_supports_partial_content()
        );
        if let Some(file_name) = task.target_file_name()
        {
            println!("Target file name: '{}'", file_name)
        }
    }
    let result;
    {
        if be_verbose { println!("{}", SEPARATOR); }
        let to_stdout = |message|
        {
            if be_verbose { println!("Message: {:?}", message); }
        };

        let t0 = time::now();
        result = task.start_and_report(&to_stdout);
        let t1 = time::now();

        let duration = t1 - t0;
        println!("{}", SEPARATOR);
        println!
        (
            "Download time: {} seconds and {} milliseconds.",
            duration.num_seconds(),
            duration.num_milliseconds() - duration.num_seconds() * 1000
        );
    }
    result
}
fn report_error(error: &download::Error)
{
    println!("Failed with error: {:?}", error);
}
