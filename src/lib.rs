extern crate ogg_sys;
extern crate vorbis_sys;
extern crate vorbisfile_sys;
extern crate libc;

use std::io::{self, Read, Seek};

/// Allows you to decode a sound file stream into packets.
pub struct Decoder<R>
    where R: Read + Seek
{
    // further informations are boxed so that a pointer can be passed to callbacks
    data: Box<DecoderData<R>>,
}

/// 
pub struct PacketsIter<'a, R: 'a + Read + Seek>(&'a mut Decoder<R>);

/// 
pub struct PacketsIntoIter<R: Read + Seek>(Decoder<R>);

/// Errors that can happen while decoding
#[derive(Debug)]
pub enum VorbisError {
    ReadError(io::Error),
    NotVorbis,
    VersionMismatch,
    BadHeader,
    InitialFileHeadersCorrupt,
    Hole,
    CommentFormat,
    CommentCharacter(std::string::FromUtf8Error),
}

impl std::error::Error for VorbisError {
    fn description(&self) -> &str {
        match self {
            &VorbisError::ReadError(_) => "A read from media returned an error",
            &VorbisError::NotVorbis => "Bitstream does not contain any Vorbis data",
            &VorbisError::VersionMismatch => "Vorbis version mismatch",
            &VorbisError::BadHeader => "Invalid Vorbis bitstream header",
            &VorbisError::InitialFileHeadersCorrupt => "Initial file headers are corrupt",
            &VorbisError::Hole => "Interruption of data",
            &VorbisError::CommentCharacter(_) => "Invalid Vorbis comment character encoding",
            &VorbisError::CommentFormat => "Invalid Vorbis comment format",
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match self {
            &VorbisError::ReadError(ref err) => Some(err as &std::error::Error),
            &VorbisError::CommentCharacter(ref err) => Some(err as &std::error::Error),
            _ => None,
        }
    }
}

impl std::fmt::Display for VorbisError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(fmt, "{}", std::error::Error::description(self))
    }
}

impl From<io::Error> for VorbisError {
    fn from(err: io::Error) -> VorbisError {
        VorbisError::ReadError(err)
    }
}

impl From<std::string::FromUtf8Error> for VorbisError {
    fn from(err: std::string::FromUtf8Error) -> VorbisError {
        VorbisError::CommentCharacter(err)
    }
}

struct DecoderData<R>
    where R: Read + Seek
{
    vorbis: vorbisfile_sys::OggVorbis_File,
    reader: R,
    current_logical_bitstream: libc::c_int,
    read_error: Option<io::Error>,
}

unsafe impl<R: Read + Seek + Send> Send for DecoderData<R> {}

/// Packet of data.
///
/// Each sample is an `i16` ranging from I16_MIN to I16_MAX.
///
/// The channels are interleaved in the data. For example if you have two channels, you will
/// get a sample from channel 1, then a sample from channel 2, than a sample from channel 1, etc.
#[derive(Clone, Debug)]
pub struct Packet {
    pub data: Vec<i16>,
    pub channels: u16,
    pub rate: u64,
    pub bitrate_upper: u64,
    pub bitrate_nominal: u64,
    pub bitrate_lower: u64,
    pub bitrate_window: u64,
}

pub struct Comment<'a> {
    bytes: &'a [u8],
    sep_pos: Option<usize>,
}

impl<'a> Comment<'a> {
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.bytes.len());
        bytes.clone_from_slice(self.bytes);
        bytes
    }

    pub fn key_bytes(&self) -> Result<&[u8], VorbisError> {
        self.sep_pos
            .ok_or(VorbisError::CommentFormat)
            .map(|sep_pos| &self.bytes[..sep_pos])
    }

    pub fn value_bytes(&self) -> Result<&[u8], VorbisError> {
        self.sep_pos
            .ok_or(VorbisError::CommentFormat)
            .map(|sep_pos| &self.bytes[sep_pos + 1..])
    }

    pub fn has_key(&self, key: &[u8]) -> Result<bool, VorbisError> {
        self.key_bytes().map(|key0| key0 == key)
    }

    pub fn key(&self) -> Result<String, VorbisError> {
        self.key_bytes().and_then(|key0| {
            if key0.iter().any(|&b| b < 0x20 || b > 0x7D || b == 0x3D) {
                Err(VorbisError::CommentFormat)
            } else {
                Ok(String::from_utf8(key0.to_vec()).unwrap())
            }
        })
    }

    pub fn value(&self) -> Result<String, VorbisError> {
        self.value_bytes()
            .and_then(|value0| Ok(try!(String::from_utf8(value0.to_vec()))))
    }
}

pub struct CommentsIter<'a, R: 'a + Read + Seek> {
    decoder: &'a Decoder<R>,
    index: i32,
}

impl<'a, R> Iterator for CommentsIter<'a, R> where R: 'a + Read + Seek
{
    type Item = Comment<'a>;

    fn next(&mut self) -> Option<Comment<'a>> {
        let comment = self.decoder.comment_at(self.index);
        if comment.is_some() {
            self.index += 1;
        }
        comment
    }
}


impl<R> Decoder<R> where R: Read + Seek
{
    pub fn new(input: R) -> Result<Decoder<R>, VorbisError> {
        extern "C" fn read_func<R>(ptr: *mut libc::c_void,
                                   size: libc::size_t,
                                   nmemb: libc::size_t,
                                   datasource: *mut libc::c_void)
                                   -> libc::size_t
            where R: Read + Seek
        {
            use std::slice;

            // In practice libvorbisfile always sets size to 1.
            // This assumption makes things much simpler
            //
            assert_eq!(size, 1);

            let ptr = ptr as *mut u8;

            let data: &mut DecoderData<R> = unsafe { std::mem::transmute(datasource) };

            let buffer = unsafe { slice::from_raw_parts_mut(ptr as *mut u8, nmemb as usize) };

            loop {
                match data.reader.read(buffer) {
                    Ok(nb) => return nb as libc::size_t,
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => (),
                    Err(e) => {
                        data.read_error = Some(e);
                        return 0;
                    }
                }
            }
        }

        extern "C" fn seek_func<R>(datasource: *mut libc::c_void,
                                   offset: ogg_sys::ogg_int64_t,
                                   whence: libc::c_int)
                                   -> libc::c_int
            where R: Read + Seek
        {
            let data: &mut DecoderData<R> = unsafe { std::mem::transmute(datasource) };

            let result = match whence {
                libc::SEEK_SET => data.reader.seek(io::SeekFrom::Start(offset as u64)),
                libc::SEEK_CUR => data.reader.seek(io::SeekFrom::Current(offset)),
                libc::SEEK_END => data.reader.seek(io::SeekFrom::End(offset)),
                _ => unreachable!(),
            };

            match result {
                Ok(_) => 0,
                Err(_) => -1,
            }
        }

        extern "C" fn tell_func<R>(datasource: *mut libc::c_void) -> libc::c_long
            where R: Read + Seek
        {
            let data: &mut DecoderData<R> = unsafe { std::mem::transmute(datasource) };
            data.reader.seek(io::SeekFrom::Current(0)).map(|v| v as libc::c_long).unwrap_or(-1)
        }

        let callbacks = {
            let mut callbacks: vorbisfile_sys::ov_callbacks = unsafe { std::mem::zeroed() };
            callbacks.read_func = read_func::<R>;
            callbacks.seek_func = seek_func::<R>;
            callbacks.tell_func = tell_func::<R>;
            callbacks
        };

        let mut data = Box::new(DecoderData {
            vorbis: unsafe { std::mem::uninitialized() },
            reader: input,
            current_logical_bitstream: 0,
            read_error: None,
        });

        // initializing
        unsafe {
            let data_ptr = &mut *data as *mut DecoderData<R>;
            let data_ptr = data_ptr as *mut libc::c_void;
            try!(check_errors(vorbisfile_sys::ov_open_callbacks(data_ptr,
                                                                &mut data.vorbis,
                                                                std::ptr::null(),
                                                                0,
                                                                callbacks)));
        }

        Ok(Decoder { data: data })
    }

    pub fn time_seek(&mut self, s: f64) -> Result<(), VorbisError> {
        unsafe { check_errors(vorbisfile_sys::ov_time_seek(&mut self.data.vorbis, s)) }
    }

    pub fn time_tell(&mut self) -> Result<f64, VorbisError> {
        unsafe { Ok(vorbisfile_sys::ov_time_tell(&mut self.data.vorbis)) }
    }

    pub fn packets(&mut self) -> PacketsIter<R> {
        PacketsIter(self)
    }

    pub fn into_packets(self) -> PacketsIntoIter<R> {
        PacketsIntoIter(self)
    }

    pub fn vendor(&self) -> Result<String, VorbisError> {
        let vendor_buf = unsafe {
            let vc = &*self.data.vorbis.vc;
            std::ffi::CStr::from_ptr(vc.vendor).to_bytes()
        };
        Ok(try!(String::from_utf8(vendor_buf.to_vec())))
    }

    pub fn comment_at<'a>(&self, index: i32) -> Option<Comment<'a>> {
        let vc = unsafe { &*self.data.vorbis.vc };
        if index >= 0 && index < vc.comments {
            let length = unsafe { *vc.comment_lengths.offset(index as isize) };
            let comment_buf = unsafe {
                let comment_ptr = *vc.user_comments.offset(index as isize);
                std::slice::from_raw_parts(comment_ptr as *const u8, length as usize)
            };
            Some(Comment {
                bytes: &comment_buf,
                sep_pos: comment_buf.iter().position(|&b| b == '=' as u8),
            })
        } else {
            None
        }
    }

    pub fn fold_comments<F, T>(&self, mut acc: T, f: F) -> T
        where F: Fn(T, &Comment) -> T
    {
        let vc = unsafe { &*self.data.vorbis.vc };
        for i in 0..vc.comments as isize {
            let length = unsafe { *vc.comment_lengths.offset(i) };
            let comment_buf = unsafe {
                let comment_ptr = *vc.user_comments.offset(i);
                std::slice::from_raw_parts(comment_ptr as *const u8, length as usize)
            };
            let comment = Comment {
                bytes: &comment_buf,
                sep_pos: comment_buf.iter().position(|&b| b == '=' as u8),
            };
            acc = f(acc, &comment);
        }
        acc
    }

    pub fn comments(&self) -> CommentsIter<R> {
        CommentsIter {
            decoder: self,
            index: 0,
        }
    }

    pub fn get_comment(&self, key: &str) -> Result<Vec<String>, VorbisError>
        where R: Read + Seek
    {
        let key_bytes = key.as_bytes();
        let mut values = vec![];
        for comment in self.comments() {
            if try!(comment.has_key(key_bytes)) {
                values.push(try!(comment.value()));
            }
        }
        Ok(values)
    }

    fn next_packet(&mut self) -> Option<Result<Packet, VorbisError>> {
        let mut buffer = std::iter::repeat(0i16).take(2048).collect::<Vec<_>>();
        let buffer_len = buffer.len() * 2;

        match unsafe {
            vorbisfile_sys::ov_read(&mut self.data.vorbis,
                                    buffer.as_mut_ptr() as *mut libc::c_char,
                                    buffer_len as libc::c_int,
                                    0,
                                    2,
                                    1,
                                    &mut self.data.current_logical_bitstream)
        } {
            0 => {
                match self.data.read_error.take() {
                    Some(err) => Some(Err(VorbisError::ReadError(err))),
                    None => None,
                }
            }

            err if err < 0 => {
                match check_errors(err as libc::c_int) {
                    Err(e) => Some(Err(e)),
                    Ok(_) => unreachable!(),
                }
            }

            len => {
                buffer.truncate(len as usize / 2);

                let infos = unsafe {
                    vorbisfile_sys::ov_info(&mut self.data.vorbis,
                                            self.data.current_logical_bitstream)
                };

                let infos: &vorbis_sys::vorbis_info = unsafe { std::mem::transmute(infos) };

                Some(Ok(Packet {
                    data: buffer,
                    channels: infos.channels as u16,
                    rate: infos.rate as u64,
                    bitrate_upper: infos.bitrate_upper as u64,
                    bitrate_nominal: infos.bitrate_nominal as u64,
                    bitrate_lower: infos.bitrate_lower as u64,
                    bitrate_window: infos.bitrate_window as u64,
                }))
            }
        }
    }
}

impl<'a, R> Iterator for PacketsIter<'a, R> where R: 'a + Read + Seek
{
    type Item = Result<Packet, VorbisError>;

    fn next(&mut self) -> Option<Result<Packet, VorbisError>> {
        self.0.next_packet()
    }
}

impl<R> Iterator for PacketsIntoIter<R> where R: Read + Seek
{
    type Item = Result<Packet, VorbisError>;

    fn next(&mut self) -> Option<Result<Packet, VorbisError>> {
        self.0.next_packet()
    }
}

impl<R> Drop for Decoder<R> where R: Read + Seek
{
    fn drop(&mut self) {
        unsafe {
            vorbisfile_sys::ov_clear(&mut self.data.vorbis);
        }
    }
}

fn check_errors(code: libc::c_int) -> Result<(), VorbisError> {
    match code {
        0 => Ok(()),

        vorbis_sys::OV_ENOTVORBIS => Err(VorbisError::NotVorbis),
        vorbis_sys::OV_EVERSION => Err(VorbisError::VersionMismatch),
        vorbis_sys::OV_EBADHEADER => Err(VorbisError::BadHeader),
        vorbis_sys::OV_EINVAL => Err(VorbisError::InitialFileHeadersCorrupt),
        vorbis_sys::OV_HOLE => Err(VorbisError::Hole),

        vorbis_sys::OV_EREAD => unimplemented!(),

        // indicates a bug or heap/stack corruption
        vorbis_sys::OV_EFAULT => panic!("Internal libvorbis error"),
        _ => panic!("Unknown vorbis error {}", code),
    }
}
