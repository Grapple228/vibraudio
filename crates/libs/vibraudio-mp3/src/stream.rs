use std::{fs::File, io::BufReader};

use bytes::BytesMut;

pub struct Mp3FileSource {
    reader: BufReader<File>,
    buffer: BytesMut,
    pos: usize,
}
