mod inner;
mod reader;
mod writer;

pub use reader::BufferReader;
use vibraudio_core::sample::Sample;
pub use writer::BufferWriter;

pub fn create_pair<const N: usize, S: Sample>() -> (BufferWriter<N, S>, BufferReader<N, S>) {
    let writer = BufferWriter::new();
    let reader = writer.reader();
    (writer, reader)
}
