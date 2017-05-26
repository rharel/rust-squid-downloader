//! Contains program-related structures.


/// Represents the program's execution options.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Options
{
    /// Indicates whether the program should be verbose.
    ///
    /// Defaults to `true`.
    pub be_verbose: bool,
}
impl Default for Options
{
    // Creates an `Options` structure with default values.
    fn default() -> Options
    {
        Options { be_verbose: true, }
    }
}
