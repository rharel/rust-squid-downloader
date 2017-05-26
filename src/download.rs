//! Handles download operations.
//!
//! # Examples
//!
//! Initiate a download:
//!
//! ```
//! use squid::download;
//!
//! let options = download::Options
//! {
//!     output_directory: String::from("./Downloads/"),
//!
//!     ..Default::default()
//! };
//! // let task = download::Task::new("https://www.google.com/", &options).unwrap();
//! // if let Ok(path) = task.start()
//! // {
//! //     println!("Download complete, file saved to: {:?}", path);
//! // }
//! ```

extern crate hyper_native_tls;
extern crate hyper;
extern crate threadpool;

use self::hyper::{Client, header};

use std::cmp;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::ops;
use std::sync;

type Vector<T> = Vec<T>;


/// Represents a download task's options.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Options
{
    /// The maximum threads to use.
    ///
    /// Defaults to `4`.
    pub thread_count: u16,
    /// The maximum size of a chunk (in kilobytes).
    ///
    /// Defaults to `5000`.
    pub chunk_size: u64,
    /// The output directory path where the downloaded file is to be saved.
    ///
    /// Defaults to `"./"` (the working directory).
    pub output_directory: String,
    /// The name given to the downloaded file.
    ///
    /// Defaults to `""`.
    ///
    /// If the empty string is specified, the name suggested by the server
    /// will be used instead. If the server does not provide a suggestion,
    /// the file is named `"untitled"`.
    pub output_file_name: String,
}
impl Default for Options
{
    /// Creates an `Options` structure with default values.
    fn default() -> Options
    {
        Options
        {
            thread_count: 4,
            chunk_size: 5000,  // in kilobytes
            output_directory: "./".to_string(),
            output_file_name: String::new(),
        }
    }
}

/// Enumerates possible download strategies.
#[derive(Debug)]
pub enum Strategy
{
    /// Download the entire content in one chunk.
    SingleChunk,
    /// Download the content across several chunks sequentially.
    SequentialMultiChunk,
    /// Download the content across several chunks concurrently.
    ConcurrentMultiChunk,
}
/// Enumerates possible messages emitted by a download task.
#[derive(Debug)]
pub enum Message
{
    /// Details the chosen download strategy.
    ChosenStrategy(Strategy),

    /// The task has begun its operation.
    TaskStarted,
    /// The task has concluded its operation.
    TaskEnded,

    /// Received a chunk of bytes from the server.
    ReceivedChunk
    {
        /// The chunk's index.
        index: u64,
        /// The chunk's byte range.
        content_byte_range: ops::Range<u64>,
    },
    /// Wrote a chunk of bytes to the filesystem.
    WroteChunk
    {
        /// The chunk's index.
        index: u64,
        /// The number of bytes written.
        byte_count: u64,
    },
}

/// Enumerates possible download errors.
#[derive(Debug)]
pub enum Error
{
    /// An option is not properly configured.
    InvalidOption(String),
    /// An HTTP-related error has occurred.
    Http(hyper::Error),
    /// A TLS-related error has occurred.
    Tls(hyper_native_tls::native_tls::Error),
    /// The response from the server was missing a header.
    MissingResponseHeader(String),
    /// The response from the server has different length than expected.
    UnexpectedResponseLength
    {
        /// The expected number of bytes to receive.
        expected: u64,
        /// The actual number of bytes received.
        actual: u64,
    },
    /// The target's content length is zero bytes.
    EmptyTarget(String),
    /// A filesystem IO-related error has occurred.
    Io(io::Error),
    /// An internal error has occurred.
    Internal,
}

/// Represents a single file's download task.
#[derive(Debug)]
pub struct Task
{
    options: Options,

    target_url: String,
    target_content_length: Option<u64>,  // in bytes
    target_supports_partial_content: Option<bool>,
    target_file_name: Option<String>,

    is_initialized: bool,

    client: sync::Arc<Client>,

    output_file_name: String,
    output_file_path: Option<String>,
    output_file: Option<File>,
}
impl Task
{
    /// Creates a new download task.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::InvalidOption(...))` when an option is improperly configured.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Tls(...))` when the establishment of the TLS client fails.
    ///
    /// Returns `Err(Error::MissingResponseHeader(...))` when the server response is missing a
    /// required header.
    ///
    /// Returns `Err(Error::EmptyTarget(...))` when the target content is reported by the server to
    /// have a length of zero bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use squid::download;
    /// #
    /// let options = download::Options
    /// {
    ///     output_directory: "./Downloads/".to_string(),
    ///
    ///     ..Default::default()
    /// };
    /// // let task = download::Task::new("https://www.google.com/", &options).unwrap();
    /// ```
    pub fn new<T>(target_url: T, options: &Options) -> Result<Task, Error>
        where T: Into<String>
    {
        use self::hyper::net::HttpsConnector;
        use self::hyper_native_tls::NativeTlsClient;

        if options.thread_count == 0
        {
            return Err(Error::InvalidOption("Thread must be at least one.".into()));
        }
        else if options.chunk_size == 0
        {
            return Err(Error::InvalidOption("Chunk size must be at least one.".into()));
        }

        let tls_client =
            NativeTlsClient::new()
                .or_else(|info| Err(Error::Tls(info)))?;

        let https_connector = HttpsConnector::new(tls_client);
        let client = Client::with_connector(https_connector);

        let mut task = Task
        {
            options: options.clone(),

            target_url: target_url.into(),
            target_content_length: None,
            target_supports_partial_content: None,
            target_file_name: None,

            is_initialized: false,

            client: sync::Arc::new(client),

            output_file_name: "untitled".into(),
            output_file_path: None,
            output_file: None,
        };
        task.initialize().and(Ok(task))
    }

    /// Starts the download.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    ///
    /// Returns `Err(Error::UnexpectedResponseLength(...))` when a response from the server
    /// is not of the expected length.
    ///
    /// Returns `Err(Error::Internal)` when an internal unknown error has occurred. This usually
    /// indicates a problem in thread management in concurrent download strategies.
    ///
    /// # Examples
    ///
    /// ```
    /// # use squid::download;
    /// #
    /// # let options = download::Options
    /// # {
    /// #     output_directory: String::from("./Downloads/"),
    /// #
    /// #     ..Default::default()
    /// # };
    /// // let task = download::Task::new("https://www.google.com/", &options).unwrap();
    /// // let output_file_path = task.start().unwrap();
    /// ```
    pub fn start(&mut self) -> Result<&str, Error>
    {
        debug_assert!(self.is_initialized);

        let report_dummy = |_| { };
        self.start_and_report(&report_dummy)
    }
    /// Starts the download and reports back status changes via a supplied
    /// callback function.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    ///
    /// Returns `Err(Error::UnexpectedResponseLength(...))` when a response from the server
    /// is not of the expected length.
    ///
    /// Returns `Err(Error::Internal)` when an internal unknown error has occurred. This usually
    /// indicates a problem in thread management in concurrent download strategies.
    ///
    /// # Examples
    ///
    /// ```
    /// # use squid::download;
    /// #
    /// # let options = download::Options
    /// # {
    /// #     output_directory: String::from("./Downloads/"),
    /// #
    /// #     ..Default::default()
    /// # };
    /// // let task = download::Task::new("https://www.google.com/", &options).unwrap();
    /// // let output_file_path =
    /// //    task
    /// //        .start_and_report(|message| println!(message))
    /// //        .unwrap();
    /// ```
    pub fn start_and_report<F>(&mut self, report: &F) -> Result<&str, Error>
        where F: ops::Fn(Message)
    {
        debug_assert!(self.is_initialized);

        report(Message::TaskStarted);

        // Open output file
        match File::create(self.output_file_path())
        {
            Ok(file) => self.output_file = Some(file),
            Err(info) => return Err(Error::Io(info)),
        }

        // Choose strategy and begin download
        let content_byte_count = self.target_content_length();
        let chunk_byte_count = self.options.chunk_size * 1000;  // chunk_size is in kilobytes

        if content_byte_count <= chunk_byte_count ||
           !self.target_supports_partial_content()
        {
            report(Message::ChosenStrategy(Strategy::SingleChunk));
            self.download_in_a_single_chunk
            (
                content_byte_count,
                report,
            )?;
        }
        else if self.options.thread_count <= 1
        {
            report(Message::ChosenStrategy(Strategy::SequentialMultiChunk));
            self.download_sequentially_in_multiple_chunks
            (
                content_byte_count,
                chunk_byte_count,
                report,
            )?;
        }
        else
        {
            report(Message::ChosenStrategy(Strategy::ConcurrentMultiChunk));
            self.download_concurrently_in_multiple_chunks
            (
                content_byte_count,
                chunk_byte_count,
                report,
            )?;
        }

        report(Message::TaskEnded);
        Ok(self.output_file_path())
    }

    /// Gets a reference to the options container.
    pub fn options(&self) -> &Options
    {
        &self.options
    }

    /// Gets a reference to the target URL.
    pub fn target_url(&self) -> &str
    {
        self.target_url.as_str()
    }
    /// Gets the target's content length (in bytes).
    pub fn target_content_length(&self) -> u64
    {
        self.target_content_length.unwrap()
    }
    /// Gets a reference to the target's file name.
    pub fn target_file_name(&self) -> Option<&str>
    {
        self.target_file_name.as_ref().map(|string| string.as_str())
    }
    /// Indicates whether the target supports partial content fetching.
    pub fn target_supports_partial_content(&self) -> bool
    {
        self.target_supports_partial_content.unwrap()
    }

    /// Gets the output file's name.
    pub fn output_file_name(&self) -> &str
    {
        self.output_file_name.as_str()
    }
    /// Gets the output file's path.
    pub fn output_file_path(&self) -> &str
    {
        self.output_file_path.as_ref().unwrap().as_str()
    }

    /// Initializes the task.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::MissingResponseHeader(...))` when the server response is missing a
    /// required header.
    ///
    /// Returns `Err(Error::EmptyTarget(...))` when the target content is reported by the server to
    /// have a length of zero bytes.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    fn initialize(&mut self) -> Result<(), Error>
    {
        debug_assert!(!self.is_initialized);
        {
            let request = self.client.head(self.target_url.as_str());
            request.send()
        }
        .or_else(|info| Err(Error::Http(info)))
        .and_then(|response|
        {
            // Extract content length
            response.headers.get::<header::ContentLength>()
            .ok_or(Error::MissingResponseHeader("Content-Length".to_string()))
            .and_then(|header| self.extract_content_length(header))?;

            // Extract file name
            if let Some(header) = response.headers.get::<header::ContentDisposition>()
            {
                self.extract_file_name(header);
            }

            self.check_for_partial_content_support()?;

            self.initialize_output_file_name();
            self.initialize_output_file_path();
            self.validate_output_directory()?;

            self.is_initialized = true;

            Ok(())
        })
    }

    /// Checks whether the target server supports partial content queries.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    fn check_for_partial_content_support(&mut self) -> Result<(), Error>
    {
        debug_assert!(self.target_supports_partial_content.is_none());
        {
            let request =
                self.client
                    .get(self.target_url.as_str())
                    .header(header::Range::bytes(0, 9));  // fetch the first ten bytes
            request.send()
        }
        .or_else(|info| Err(Error::Http(info)))
        .and_then(|response|
        {
            use self::hyper::status::StatusCode;

            self.target_supports_partial_content =
                Some(response.status == StatusCode::PartialContent);
            Ok(())
        })
    }
    /// Initializes the output file path field.
    fn initialize_output_file_path(&mut self)
    {
        debug_assert!(self.output_file_path.is_none());

        let mut path = self.options.output_directory.clone();
        if !path.ends_with("/") { path.push_str("/"); }
        path.push_str(self.output_file_name.as_str());

        self.output_file_path = Some(path);
    }
    /// Initializes the output file name field.
    fn initialize_output_file_name(&mut self)
    {
        if !self.options.output_file_name.is_empty()
        {
            self.output_file_name = self.options.output_file_name.clone();
        }
        else if let Some(ref file_name) = self.target_file_name
        {
            if !file_name.is_empty()
            {
                self.output_file_name = file_name.clone();
            }
        }
        debug_assert!(!self.output_file_name.is_empty());
    }
    /// Validates the output directory.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    fn validate_output_directory(&self) -> Result<(), Error>
    {
        use std::path::Path;

        let path = Path::new(self.options.output_directory.as_str());
        if !path.is_dir() || !path.exists()
        {
            return Err(Error::Io(io::Error::new
            (
                io::ErrorKind::NotFound,
                "The specified path is either not a directory or does not exist."
            )));
        }
        else { Ok(()) }
    }

    /// Extracts the content length (in bytes) from the `"Content Length"` header.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Error::EmptyTarget(...)` if the content length is zero bytes.
    fn extract_content_length(&mut self, header: &header::ContentLength)
        -> Result<(), Error>
    {
        debug_assert!(self.target_content_length.is_none());

        let &header::ContentLength(byte_count) = header;
        self.target_content_length = Some(byte_count);

        if byte_count == 0
        {
            Err(Error::EmptyTarget(self.target_url.clone()))
        }
        else { Ok(()) }
    }
    /// Extracts the file name from the `"Content Disposition"` header, if possible.
    fn extract_file_name(&mut self, header: &header::ContentDisposition)
        -> bool
    {
        debug_assert!(self.target_file_name.is_none());

        for item in &header.parameters
        {
            if let &header::DispositionParam::Filename(_, _, ref bytes) = item
            {
                self.target_file_name = String::from_utf8(bytes.clone()).ok();
                return true;
            }
        }
        false
    }

    /// Downloads the target content in one chunk.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    ///
    /// Returns `Err(Error::UnexpectedResponseLength(...))` when a response from the server
    /// is not of the expected length.
    fn download_in_a_single_chunk<F>
    (
        &mut self,
        content_byte_count: u64,
        report: &F
    )
        -> Result<(), Error>
        where F: ops::Fn(Message)
    {
        let byte_range_start = 0;
        let byte_range_end = content_byte_count - 1;
        let mut bytes = Vector::with_capacity(content_byte_count as usize);
        Task::get_content_byte_range
        (
            self.target_url(), &self.client,
            byte_range_start, byte_range_end,
            content_byte_count - 1, &mut bytes
        )?;

        report(Message::ReceivedChunk
        {
            index: 0,
            content_byte_range: 0..(byte_range_end + 1)
        });
        {
            let expected = content_byte_count;
            let actual = bytes.len() as u64;
            if actual != expected
            {
                return Err(Error::UnexpectedResponseLength { expected, actual });
            }
        }

        self.write_to_output(&bytes)?;

        report(Message::WroteChunk
        {
            index: 0,
            byte_count: bytes.len() as u64
        });

        Ok(())
    }
    /// Downloads the target content in several chunks sequentially.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    ///
    /// Returns `Err(Error::UnexpectedResponseLength(...))` when a response from the server
    /// is not of the expected length.
    fn download_sequentially_in_multiple_chunks<F>
    (
        &mut self,
        content_byte_count: u64,
        chunk_byte_count: u64,
        report: &F
    )
        -> Result<(), Error>
        where F: ops::Fn(Message)
    {
        let mut bytes = Vector::with_capacity(chunk_byte_count as usize);
        let mut chunk_index = 0;
        let mut byte_range_start = 0;

        while byte_range_start <= content_byte_count - 1
        {
            bytes.clear();
            let byte_range_end = cmp::min
            (
                byte_range_start + chunk_byte_count - 1,
                content_byte_count - 1
            );
            Task::get_content_byte_range
            (
                self.target_url(), &self.client,
                byte_range_start, byte_range_end,
                content_byte_count - 1, &mut bytes
            )?;

            report(Message::ReceivedChunk
            {
               index: chunk_index,
               content_byte_range: byte_range_start..(byte_range_end + 1),
            });
            {
                let expected = byte_range_end - byte_range_start + 1;
                let actual = bytes.len() as u64;
                if actual != expected
                {
                    return Err(Error::UnexpectedResponseLength { expected, actual });
                }
            }

            self.write_to_output(&bytes)?;

            report(Message::WroteChunk
            {
                index: chunk_index,
                byte_count: bytes.len() as u64
            });

            chunk_index += 1;
            byte_range_start = byte_range_end + 1;
        }

        Ok(())
    }
    /// Downloads the target content in several chunks concurrently.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    ///
    /// Returns `Err(Error::UnexpectedResponseLength(...))` when a response from the server
    /// is not of the expected length.
    fn download_concurrently_in_multiple_chunks<F>
    (
        &mut self,
        content_byte_count: u64,
        chunk_byte_count: u64,
        report: &F
    )
        -> Result<(), Error>
        where F: ops::Fn(Message)
    {
        use self::threadpool::ThreadPool;
        use std::sync::atomic;
        use std::sync::atomic::AtomicBool;
        use std::sync::mpsc;

        let threadpool = ThreadPool::new(self.options.thread_count as usize);

        // We call this thread the 'master' thread, and we are about to spawn
        // several 'slave' threads to fetch chunk data for us.
        let (transmitter_to_master, receiver_from_slaves) = mpsc::channel();
        // We use this flag solely as an indicator for slaves to know when
        // we would like them to terminate without doing work. This happens to be the case
        // when one of the previous slaves could not complete its task successfully.
        // That is, if one slave cannot fetch its chunk's data, there is no point
        // to bother fetching data for other chunks any longer.
        let slaves_should_abort = sync::Arc::new(AtomicBool::new(false));

        // Spawn a thread to download each chunk
        {
            let mut chunk_index = 0 as u64;
            let mut byte_range_start = 0 as u64;
            while byte_range_start <= content_byte_count - 1
            {
                let transmitter = transmitter_to_master.clone();
                let slaves_should_abort = slaves_should_abort.clone();

                let url = self.target_url.clone();
                let client = self.client.clone();
                let byte_range_end = cmp::min
                (
                    byte_range_start + chunk_byte_count - 1,
                    content_byte_count - 1
                );

                #[allow(unused_must_use)]
                threadpool.execute(move ||
                {
                    if slaves_should_abort.load(atomic::Ordering::Acquire)
                    {
                        return;
                    }

                    let mut bytes = Vector::with_capacity(chunk_byte_count as usize);
                    let message = Task::get_content_byte_range
                    (
                        url.as_str(), &client,
                        byte_range_start, byte_range_end,
                        content_byte_count - 1, &mut bytes
                    )
                    .and_then(|_|
                    {
                        let expected = byte_range_end - byte_range_start + 1;
                        let actual = bytes.len() as u64;
                        if actual != expected
                        {
                            Err(Error::UnexpectedResponseLength { expected, actual })
                        }
                        else { Ok(()) }
                    })
                    .and(Ok(
                    (
                        chunk_index,
                        byte_range_start, byte_range_end,
                        bytes
                    )));

                    transmitter.send(message);
                });
                chunk_index += 1;
                byte_range_start = byte_range_end + 1;
            }
        }
        // Now, we receive chunks and make sure to write them to the output file in
        // the right order.
        {
            use std::collections::{BinaryHeap, HashMap};

            // The total number of chunks we expect to receive, rounded up.
            let expected_chunk_count =
                (content_byte_count + chunk_byte_count - 1) / chunk_byte_count;

            // A priority queue for received chunks, ordered by their index.
            let mut pending_chunk_index_queue =
                BinaryHeap::with_capacity(expected_chunk_count as usize);

            // A mapping between a chunk's index and its data.
            let mut chunk_data_by_index =
                HashMap::with_capacity(expected_chunk_count as usize);

            let mut result: Result<(), Error> = Ok(());
            let mut next_chunk_index = 0 as u64;  // The next chunk to write to output
            while result.is_ok() &&
                  next_chunk_index < expected_chunk_count
            {
                let message = receiver_from_slaves.recv();
                let mut chunk_data = None;
                {
                    result = match message
                    {
                        Ok(Ok(content)) => { chunk_data = Some(content); Ok(()) },
                        Ok(Err(info)) => Err(info),
                        Err(_) => Err(Error::Internal),
                    };
                    if result.is_err()
                    {
                        slaves_should_abort.store(true, atomic::Ordering::Release);
                        break;
                    }
                }
                {
                    let chunk_data = chunk_data.unwrap();
                    let (chunk_index, byte_range_start, byte_range_end, _) = chunk_data;

                    chunk_data_by_index.insert(chunk_index, chunk_data);

                    let key = expected_chunk_count - chunk_index;  // reverses order in queue
                    pending_chunk_index_queue.push(key);

                    report(Message::ReceivedChunk
                    {
                        index: chunk_index,
                        content_byte_range: byte_range_start..(byte_range_end + 1),
                    });
                }
                while !pending_chunk_index_queue.is_empty() &&
                      *pending_chunk_index_queue.peek().unwrap() ==
                       expected_chunk_count - next_chunk_index
                {
                    let key = pending_chunk_index_queue.pop().unwrap();
                    let chunk_index = expected_chunk_count - key;
                    let (_, _, _, bytes) =
                        chunk_data_by_index.remove(&chunk_index).unwrap();

                    result = self.write_to_output(&bytes);
                    if result.is_ok()
                    {
                        report(Message::WroteChunk
                        {
                            index: chunk_index,
                            byte_count: bytes.len() as u64
                        });
                        next_chunk_index += 1;
                    }
                }
            }
            result
        }
    }

    /// Gets the specified content byte range from the server.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Http(...))` when an HTTP-related operation fails.
    ///
    /// Returns `Err(Error::Io(...))` when a filesystem IO-related operation fails.
    fn get_content_byte_range
    (
        url: &str, client: &Client,
        start: u64, end: u64, max: u64,
        bytes: &mut Vector<u8>
    )
        -> Result<(), Error>
    {
        debug_assert!(start <= end);
        debug_assert!(end <= max);
        {
            let request =
            {
                if start == 0 && end == max
                {
                    client.get(url)
                }
                else
                {
                    client
                        .get(url)
                        .header(header::Range::bytes(start, end))
                }
            };
            request.send()
        }
        .or_else(|info| Err(Error::Http(info)))
        .and_then(|mut response|
        {
            response.read_to_end(bytes)
            .or_else(|info| Err(Error::Io(info)))
            .and(Ok(()))
        })
    }
    /// Writes the specified bytes to the output file.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Io(...))` when the write fails.
    fn write_to_output(&mut self, bytes: &[u8]) -> Result<(), Error>
    {
        self.output_file.as_ref().unwrap()
            .write_all(bytes)
            .or_else(|info| Err(Error::Io(info)))
            .and(Ok(()))
    }
}

#[cfg(test)]
mod tests
{
    static GOOGLE_LOGO_URL: &'static str =
        "https://www.google.nl/images/branding/googlelogo/2x/googlelogo_color_272x92dp.png";

    use super::*;

    #[test]
    fn new_with_invalid_target_url()
    {
        let options: Options = Default::default();
        let result = Task::new("!@#$", &options);

        if let Err(Error::Http(_)) = result { }
        else { panic!("Expected an HTTP-related error, got '{:?}' instead.", result) }
    }
    #[test]
    fn new_with_invalid_output_directory()
    {
        let options = Options
        {
            output_directory: "!@#$".into(),

            ..Default::default()
        };
        let result = Task::new(GOOGLE_LOGO_URL, &options);

        if let Err(Error::Io(_)) = result { }
        else { panic!("Expected an output-related error, got '{:?}' instead.", result) }
    }
    #[test]
    fn new()
    {
        let options = Options
        {
            output_directory: "./Downloads".into(),
            output_file_name: "output.png".into(),

            ..Default::default()
        };
        let task = Task::new(GOOGLE_LOGO_URL, &options).unwrap();

        assert_eq!(&options, task.options());

        assert_eq!(GOOGLE_LOGO_URL, task.target_url());
        assert!(task.target_content_length() > 0);
        assert_eq!("output.png", task.output_file_name());
        assert_eq!("./Downloads/output.png", task.output_file_path());
    }
}
