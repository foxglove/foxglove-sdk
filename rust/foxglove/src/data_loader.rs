/// Use this macro to define a wasm data loader you can load in a foxglove extension with the
/// extensionContext.registerDataLoader() api.
#[macro_export]
macro_rules! define_data_loader {
    ( $T:ident, $M:ident ) => {
        wit_bindgen::generate!({
            world: "data-loader",
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
                        schema-data: list<u8>
                    }

                    record message {
                        channel-id: u16,
                        timestamp-nanos: u64,
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

                world data-loader {
                    import console;
                    import reader;
                    export loader;
                }
            "#,
        });

        mod $M {
            pub use crate::foxglove::loader::reader;
            pub use crate::exports::foxglove::loader::loader::{
                self,
                BackfillArgs, Channel, DataLoader, TimeRange,
                Guest, GuestDataLoader, GuestMessageIterator,
                Message, MessageIterator, MessageIteratorArgs,
            };
        }
        foxglove_wit_export!($T);

        impl std::io::Read for $M::reader::Reader {
            fn read(&mut self, dst: &mut [u8]) -> Result<usize, std::io::Error> {
                use crate::foxglove::loader::reader;
                Ok(reader::Reader::read(&self, dst) as usize)
            }
        }

        impl std::io::Seek for $M::reader::Reader {
            fn seek(&mut self, seek: std::io::SeekFrom) -> Result<u64, std::io::Error> {
                use crate::foxglove::loader::reader;
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
                        reader::Reader::seek(&self, (pos - offset) as u64);
                    },
                }
                Ok(reader::Reader::position(&self))
            }
        }
    };
}
