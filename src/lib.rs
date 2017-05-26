//! Contains application logic for a fast downloader of content over HTTP.
//!
//! # Examples
//!
//! Initiate a download:
//!
//! ```no-run
//! use squid::command_line;
//! use squid::download;
//!
//! let arguments = command_line::parse_arguments().unwrap();
//! let task = download::Task::new
//! (
//!     "https://www.google.com/",
//!     &arguments.download_options
//! )
//!     .unwrap();
//!
//! if let Ok(path) = task.start()
//! {
//!     println!("Download complete, file saved to: {}", path);
//! }
//! ```

#![deny(missing_docs)]
#![doc(test(attr(allow(unused_variables), deny(warnings))))]

pub mod command_line;
pub mod program;
pub mod download;
