/// Use this macro to define a wasm data loader you can load in a foxglove extension with the
/// extensionContext.registerDataLoader() api.
#[macro_export]
macro_rules! define_data_loader {
    ( $M:ident, $L:ident ) => {
        pub mod $M {
            ::foxglove::define_data_loader!($L);
        }
    };
    ( $L:ident ) => {
        use crate::$L as LOADER;
        wit_bindgen::generate!({
            world: "host",
            export_macro_name: "foxglove_wit_export",
            inline: r#"
                package foxglove:loader@0.1.0;

                interface console {
                    log: func(log: string);
                    error: func(log: string);
                    warn: func(log: string);
                }

                interface reader {
                    resource reader {
                        seek: func(pos: u64) -> u64;
                        position: func() -> u64;
                        read: func(target: list<u8>) -> u64;
                        size: func() -> u64;
                    }

                    open: func(path: string) -> reader;
                }

                interface loader {
                    record message-iterator-args {
                        start-nanos: option<u64>,
                        end-nanos: option<u64>,
                        channels: list<u16>,
                    }

                    record backfill-args {
                        time-nanos: u64,
                        channels: list<u16>,
                    }

                    record time-range {
                        start-nanos: u64,
                        end-nanos: u64,
                    }

                    record channel {
                        id: u16,
                        topic-name: string,
                        schema-name: string,
                        message-encoding: string,
                        schema-encoding: string,
                        schema-data: list<u8>,
                        message-count: option<u64>,
                    }

                    record message {
                        channel-id: u16,
                        // The timestamp in nanoseconds at which the message was recorded.
                        log-time: u64,
                        // The timestamp in nanoseconds at which the message was published.
                        // If not available, must be set to the log time.
                        publish-time: u64,
                        data: list<u8>
                    }

                    resource message-iterator {
                        next: func() -> option<result<message, string>>;
                    }

                    resource data-loader {
                        // The time range covered by the data.
                        time-range: func() -> result<time-range, string>;
                        // The list of channels contained in the data.
                        channels: func() -> result<list<channel>, string>;
                        // Create an iterator over the data for the requested channels and time range.
                        create-iter: func(args: message-iterator-args) -> result<message-iterator, string>;
                        // Get the messages on certain channels at a certain time
                        get-backfill: func(args: backfill-args) -> result<list<message>, string>;
                    }

                    // Create a new instance of the data loader for a list of files
                    create: func(input: list<string>) -> result<data-loader, string>;
                }

                world host {
                    import console;
                    import reader;
                    export loader;
                }
            "#,
        });

        pub use self::foxglove::loader::reader;
        pub use self::foxglove::loader::console;
        pub use self::exports::foxglove::loader::loader::{
            self,
            BackfillArgs, Channel, TimeRange,
            Message, MessageIteratorArgs,
            GuestMessageIterator as MessageIterator,
        };
        foxglove_wit_export!(LOADER);

        impl std::io::Read for reader::Reader {
            fn read(&mut self, dst: &mut [u8]) -> Result<usize, std::io::Error> {
                Ok(reader::Reader::read(&self, dst) as usize)
            }
        }

        impl std::io::Seek for reader::Reader {
            fn seek(&mut self, seek: std::io::SeekFrom) -> Result<u64, std::io::Error> {
                match seek {
                    std::io::SeekFrom::Start(offset) => {
                        reader::Reader::seek(&self, offset);
                    },
                    std::io::SeekFrom::End(offset) => {
                        let end = reader::Reader::size(&self) as i64;
                        reader::Reader::seek(&self, (end - offset) as u64);
                    },
                    std::io::SeekFrom::Current(offset) => {
                        let pos = reader::Reader::position(&self) as i64;
                        reader::Reader::seek(&self, (pos + offset) as u64);
                    },
                }
                Ok(reader::Reader::position(&self))
            }
        }

        pub trait DataLoader: 'static+Sized {
            type MessageIterator: loader::GuestMessageIterator;

            fn create(inputs: Vec<String>) -> Result<Self, String>;
            fn channels(&self) -> Result<Vec<loader::Channel>, String>;
            fn time_range(&self) -> Result<loader::TimeRange, String>;
            fn create_iter(&self, args: loader::MessageIteratorArgs) -> Result<Self::MessageIterator, String>;
            fn get_backfill(&self, args: loader::BackfillArgs) -> Result<Vec<loader::Message>, String>;
        }

        impl<T: DataLoader> loader::Guest for T {
            type DataLoader = Self;
            type MessageIterator = T::MessageIterator;

            fn create(inputs: Vec<String>) -> Result<loader::DataLoader, String> {
                T::create(inputs).map(|loader| loader::DataLoader::new(loader))
            }
        }

        impl<T: DataLoader> loader::GuestDataLoader for T {
            fn channels(&self) -> Result<Vec<loader::Channel>, String> {
                T::channels(self)
            }

            fn time_range(&self) -> Result<loader::TimeRange, String> {
                T::time_range(self)
            }

            fn create_iter(&self, args: loader::MessageIteratorArgs) -> Result<loader::MessageIterator, String> {
                T::create_iter(self, args).map(|iter| loader::MessageIterator::new(iter))
            }

            fn get_backfill(&self, args: loader::BackfillArgs) -> Result<Vec<loader::Message>, String> {
                T::get_backfill(self, args)
            }
        }
    };
}
