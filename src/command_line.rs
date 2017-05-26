//! Handles command line operations.
//!
//! # Examples
//!
//! Parse command line arguments:
//!
//! ```
//! use squid::command_line;
//!
//! if let Ok(arguments) = command_line::parse_arguments()
//! {
//!     println!("Arguments parsed: {:?}",  arguments);
//! }
//! ```

extern crate argparse;

use program;
use download;


/// Represents parsed command line arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Arguments
{
    /// The target URL to download.
    pub target_url: String,
    /// The container for program execution options.
    pub program_options: program::Options,
    /// The container for download options.
    pub download_options: download::Options,
}
/// Enumerates possible parsing errors.
#[derive(PartialEq, Eq, Debug)]
pub enum ParseError
{
    /// The application was invoked with `--help`.
    OnlyHelpRequested,
    /// The arguments parsed have an invalid form.
    InvalidArguments,
    /// An unknown error has occurred.
    Unknown,
}

/// Parses command line arguments.
///
/// # Errors
///
/// `Err(ParsingError::OnlyHelpRequested)` when the application
/// was invoked with `--help`, as this nullifies the need to parse any other
/// arguments or to resume program execution.
///
/// `Err(ParsingError::InvalidArguments)` when one or more arguments
/// supplied have an invalid form.
///
/// `Err(ParsingError::Unknown)` when the error that has occurred could
/// not be identified. This error type exists for the sake of completion, but
/// it is not expected for you to ever encounter it.
///
/// # Examples
///
/// Parse arguments:
///
/// ```
/// # use squid::command_line::parse_arguments;
/// #
/// if let Ok(arguments) = parse_arguments()
/// {
///     println!("Arguments parsed: {:?}",  arguments);
/// }
/// ```
pub fn parse_arguments() -> Result<Arguments, ParseError>
{
    use self::argparse::ArgumentParser;
    use self::argparse::Print;
    use self::argparse::Store;
    use self::argparse::StoreFalse;
    use self::argparse::StoreTrue;

    let mut arguments = Arguments
    {
        target_url: String::new(),
        program_options: Default::default(),
        download_options: Default::default(),
    };
    let result;
    {
        let mut parser = ArgumentParser::new();

        parser.set_description("A fast downloader operating from the command line.");

        // Required arguments
        parser
            .refer(&mut arguments.target_url)
            .add_argument("url", Store, "target URL")
            .required();

        // Program options
        parser.add_option
        (
            &["-V", "--version"],
            Print(env!("CARGO_PKG_VERSION").to_string()),
            "print program version"
        );
        parser
            .refer(&mut arguments.program_options.be_verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "be verbose")
            .add_option(&["-q", "--quiet"], StoreFalse, "be quiet");

        // Download options
        parser
            .refer(&mut arguments.download_options.thread_count)
            .add_option(&["-t", "--threads"], Store, "maximum thread count")
            .metavar("N");
        parser
            .refer(&mut arguments.download_options.chunk_size)
            .add_option(&["-c", "--chunk"], Store, "maximum chunk size (in kilobytes)")
            .metavar("KB");
        parser
            .refer(&mut arguments.download_options.output_directory)
            .add_option(&["-d", "--directory"], Store, "output file directory")
            .metavar("D");
        parser
            .refer(&mut arguments.download_options.output_file_name)
            .add_option(&["-f", "--file"], Store, "output file name")
            .metavar("F");

        result = parser.parse_args();
    }
    match result
    {
        Ok(()) => Ok(arguments),
        Err(0) => Err(ParseError::OnlyHelpRequested),
        Err(2) => Err(ParseError::InvalidArguments),
        Err(_) => Err(ParseError::Unknown),
    }
}
