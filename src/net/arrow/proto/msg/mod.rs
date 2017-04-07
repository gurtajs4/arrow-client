// Copyright 2017 click2stream, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod control;

use std::mem;

use bytes::{Bytes, BytesMut};

use utils;

use utils::AsAny;

use net::arrow::proto::ARROW_PROTOCOL_VERSION;
use net::arrow::proto::codec::{FromBytes, Decode, Encode};
use net::arrow::proto::error::DecodeError;

/// Common trait for message body types.
pub trait MessageBody : Encode {
    /// Get size of the body in bytes.
    fn len(&self) -> usize;
}

impl<T: AsRef<[u8]>> MessageBody for T {
    fn len(&self) -> usize {
        self.as_ref()
            .len()
    }
}

/// Arrow Message header.
#[derive(Debug, Copy, Clone)]
#[repr(packed)]
pub struct ArrowMessageHeader {
    /// Arrow Protocol major version.
    pub version: u8,
    /// Service ID.
    pub service: u16,
    /// Session ID (note: the upper 8 bits are reserved).
    pub session: u32,
    /// Payload size.
    size:        u32,
}

impl ArrowMessageHeader {
    /// Create a new Arrow Message header with a given service ID, session ID
    /// and payload size.
    fn new(service: u16, session: u32, size: u32) -> ArrowMessageHeader {
        ArrowMessageHeader {
            version: ARROW_PROTOCOL_VERSION,
            service: service,
            session: session & ((1 << 24) - 1),
            size:    size
        }
    }
}

impl Encode for ArrowMessageHeader {
    fn encode(&self, buf: &mut BytesMut) {
        let be_header = ArrowMessageHeader {
            version: self.version,
            service: self.service.to_be(),
            session: self.session.to_be(),
            size:    self.size.to_be()
        };

        buf.extend(utils::as_bytes(&be_header))
    }
}

impl FromBytes for ArrowMessageHeader {
    fn from_bytes(bytes: &[u8]) -> Result<Option<ArrowMessageHeader>, DecodeError> {
        assert_eq!(bytes.len(), mem::size_of::<ArrowMessageHeader>());

        let ptr    = bytes.as_ptr() as *const ArrowMessageHeader;
        let header = unsafe { &*ptr };

        let res = ArrowMessageHeader {
            version: header.version,
            service: u16::from_be(header.service),
            session: u32::from_be(header.session) & ((1 << 24) - 1),
            size:    u32::from_be(header.size)
        };

        if res.version == ARROW_PROTOCOL_VERSION {
            Ok(Some(res))
        } else {
            Err(DecodeError::from("unsupported Arrow Protocol version"))
        }
    }
}

/// Common trait for Arrow Message body implementations.
pub trait ArrowMessageBody : MessageBody + AsAny + Send {
}

impl ArrowMessageBody for Bytes {
}

/// Arrow Message.
pub struct ArrowMessage {
    /// Message header.
    header:  ArrowMessageHeader,
    /// Encoded message body.
    payload: Bytes,
}

impl ArrowMessage {
    /// Create a new Arrow Message with a given service ID, session ID and payload.
    pub fn new<B>(service: u16, session: u32, body: B) -> ArrowMessage
        where B: ArrowMessageBody + 'static {
        let mut payload = BytesMut::with_capacity(body.len());

        body.encode(&mut payload);

        ArrowMessage {
            header:  ArrowMessageHeader::new(service, session, 0),
            payload: payload.freeze(),
        }
    }

    /// Get reference to the message header.
    pub fn header(&self) -> ArrowMessageHeader {
        self.header
    }

    /// Get encoded message body.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }
}

impl Encode for ArrowMessage {
    fn encode(&self, buf: &mut BytesMut) {
        let header = ArrowMessageHeader::new(
            self.header.service,
            self.header.session,
            self.payload.len() as u32);

        header.encode(buf);

        buf.extend(self.payload.as_ref())
    }
}

impl Decode for ArrowMessage {
    fn decode(buf: &mut BytesMut) -> Result<Option<ArrowMessage>, DecodeError> {
        let hsize = mem::size_of::<ArrowMessageHeader>();

        if buf.len() < hsize {
            return Ok(None);
        }

        if let Some(header) = ArrowMessageHeader::from_bytes(&buf[..hsize])? {
            let msize = header.size as usize + hsize;

            if buf.len() < msize {
                return Ok(None);
            }

            let message = buf.split_to(msize);
            let payload = message.freeze()
                .split_off(hsize);

            let msg = ArrowMessage {
                header:  header,
                payload: payload,
            };

            Ok(Some(msg))
        } else {
            panic!("unable to decode an Arrow Message header")
        }
    }
}
